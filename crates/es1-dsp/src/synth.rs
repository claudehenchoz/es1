// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! The complete instrument: parameter dispatch, keyboard, voice
//! allocation, unison, portamento, LFOs, MIDI-controller modulation, arpeggiator, master
//! effects and volume.

use crate::arp::Arp;
use crate::env::{attack_time, decay_time};
use crate::filter::FilterMode;
use crate::fx::{EffectType, MasterFx};
use crate::lfo::{free_rate, sync_rate, Lfo};
use crate::osc::Wave;
use crate::params::{self, Values, NUM_SY1};
use crate::util::{enable_ftz, restore_ftz, Smoother};
use crate::voice::{Voice, VoiceMods, VoiceParams};

pub const MAX_VOICES: usize = 32;
/// Control-rate chunk: LFOs, pitch and cutoff are updated every `CHUNK` samples.
pub const CHUNK: usize = 16;

/// Unison detune in cents for a 0..127 value (exponential law), 0..100 ct.
pub fn unison_detune_cents(v: f32) -> f32 {
    let e2 = core::f32::consts::E * core::f32::consts::E;
    100.0 * ((2.0 * v / 127.0).exp() - 1.0) / (e2 - 1.0)
}

/// Master volume 0..127 → linear gain: 64 ≈ −1.7 dB, 127 ≈ +9.2 dB.
pub fn master_gain(v: f32) -> f32 {
    if v <= 0.0 {
        return 0.0;
    }
    let x = v / 127.0 + 0.01;
    let db = 20.0 * x - 1.0 / x - 10.0;
    10f32.powf(db / 20.0)
}

#[inline]
fn x01(v: f32) -> f32 {
    (v / 127.0).clamp(0.0, 1.0)
}

#[inline]
fn bipolar64(v: f32) -> f32 {
    if v < 64.0 {
        v / 64.0 - 1.0
    } else {
        (v - 64.0) / 63.0
    }
}

/// Envelope sustain / amp gain curve: `2048^(x^0.3)/2048`, 0 at v = 0.
#[inline]
fn amp_curve(v: f32) -> f32 {
    if v <= 0.0 {
        0.0
    } else {
        2048f32.powf(x01(v).powf(0.3)) / 2048.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayMode {
    Poly,
    Mono,
    Legato,
}

#[derive(Clone, Copy, Debug, Default)]
struct LfoCfg {
    on: bool,
    wave: u8,
    dest: u8,
    speed: f32,
    depth: f32,
    sync: bool,
    key: bool,
}

#[derive(Clone, Copy, Debug)]
struct CtlSlot {
    src: i32,
    sens: f32,
    assign: i32,
    value: f32,
}

/// Note stack + velocities.
#[derive(Clone, Debug)]
struct Keys {
    vel: [u8; 128],
    stack: Vec<u8>,
}

impl Keys {
    fn new() -> Self {
        Self { vel: [0; 128], stack: Vec::with_capacity(128) }
    }
    fn press(&mut self, n: u8, v: u8) {
        self.release(n);
        self.vel[n as usize] = v.max(1);
        self.stack.push(n);
    }
    fn release(&mut self, n: u8) {
        self.vel[n as usize] = 0;
        self.stack.retain(|&x| x != n);
    }
    fn clear(&mut self) {
        self.vel = [0; 128];
        self.stack.clear();
    }
}

pub struct Synth {
    sr: f32,
    bpm: f32,
    base: [f32; NUM_SY1],
    eff: [f32; NUM_SY1],
    vp: VoiceParams,
    voices: Vec<Voice>,
    age: u64,
    last_slot: usize,

    // keyboard (KB1) and allocator input (KB1 or the arp's KB2)
    kb: Keys,
    akeys: Keys,
    sustained: [bool; 128],
    pedal: bool,
    mono_vel: u8,

    mode: PlayMode,
    polyphony: usize,
    uni_on: bool,
    uni_n: usize,

    lfo: [Lfo; 2],
    lcfg: [LfoCfg; 2],
    arp: Arp,
    arp_on: bool,
    arp_held: Vec<u8>,

    fx: MasterFx,
    pan_base: f32,
    cutoff_target: f32,
    cutoff_step: f32,

    bend_target: f32,
    bend: f32,
    bend_range: f32,
    ctl: [CtlSlot; 2],

    master: Smoother,
    l: [f32; CHUNK],
    r: [f32; CHUNK],
}

impl Synth {
    pub fn new(sample_rate: f32) -> Self {
        let mut s = Self {
            sr: sample_rate,
            bpm: 120.0,
            base: [0.0; NUM_SY1],
            eff: [0.0; NUM_SY1],
            vp: VoiceParams::default(),
            voices: (0..MAX_VOICES).map(|i| Voice::new(i as u32 + 1)).collect(),
            age: 0,
            last_slot: MAX_VOICES - 1,
            kb: Keys::new(),
            akeys: Keys::new(),
            sustained: [false; 128],
            pedal: false,
            mono_vel: 100,
            mode: PlayMode::Poly,
            polyphony: 16,
            uni_on: false,
            uni_n: 2,
            lfo: [Lfo::new(11), Lfo::new(23)],
            lcfg: [LfoCfg::default(); 2],
            arp: Arp::default(),
            arp_on: false,
            arp_held: Vec::with_capacity(128),
            fx: MasterFx::new(sample_rate),
            pan_base: 64.0,
            cutoff_target: 0.0,
            cutoff_step: 0.0,
            bend_target: 0.0,
            bend: 0.0,
            bend_range: 12.0,
            ctl: [CtlSlot { src: 0xB001, sens: 0.0, assign: -1, value: 0.0 }; 2],
            master: Smoother::new(master_gain(64.0)),
            l: [0.0; CHUNK],
            r: [0.0; CHUNK],
        };
        s.master.set_time(sample_rate, 0.003);
        s.load_values(&params::template());
        s
    }

    pub fn sample_rate(&self) -> f32 {
        self.sr
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        if sr == self.sr {
            return;
        }
        self.sr = sr;
        self.fx.set_sample_rate(sr);
        self.master.set_time(sr, 0.003);
        let vals = self.base;
        for (i, v) in vals.iter().enumerate() {
            self.set_param(i, *v);
        }
    }

    pub fn set_tempo(&mut self, bpm: f32) {
        if bpm > 0.0 && (bpm - self.bpm).abs() > 1e-3 {
            self.bpm = bpm;
            self.apply(35);
            for i in 0..2 {
                self.update_lfo_rate(i);
            }
        }
    }

    pub fn set_master_volume(&mut self, v: f32) {
        self.master.set(master_gain(v));
    }

    /// Load a complete patch (current-version values). Silences all voices.
    pub fn load_values(&mut self, v: &Values) {
        for (i, &x) in v.iter().enumerate() {
            self.base[i] = x as f32;
        }
        self.ctl.iter_mut().for_each(|c| c.value = 0.0);
        self.refresh_ctl_targets();
        for i in 0..NUM_SY1 {
            self.refresh(i);
        }
        self.vp.cutoff_idx = self.cutoff_target;
        self.panic();
    }

    /// Current stored (un-modulated) value of a sy1 parameter.
    pub fn value(&self, sy1: usize) -> f32 {
        self.base[sy1]
    }

    /// Set a parameter in sy1 units (fractional allowed for knobs).
    pub fn set_param(&mut self, sy1: usize, v: f32) {
        if sy1 >= NUM_SY1 {
            return;
        }
        let v = if let Some(p) = params::by_sy1(sy1) {
            if matches!(sy1, 86 | 88) {
                v
            } else {
                v.clamp(p.min as f32, p.max as f32)
            }
        } else {
            v
        };
        if self.base[sy1] == v {
            return;
        }
        self.base[sy1] = v;
        self.refresh(sy1);
        if matches!(sy1, 50 | 51 | 86..=89) {
            self.refresh_ctl_targets();
        }
    }

    // ---- MIDI ---------------------------------------------------------------------------

    pub fn note_on(&mut self, note: u8, vel: u8) {
        if vel == 0 {
            return self.note_off(note);
        }
        let note = note.min(127);
        if self.kb.stack.is_empty() {
            if self.arp_on {
                self.arp.reset(vel);
            }
            for i in 0..2 {
                if self.lcfg[i].on && self.lcfg[i].key {
                    self.lfo[i].retrigger();
                }
            }
        }
        self.sustained[note as usize] = false;
        self.kb.press(note, vel);
        if self.arp_on {
            self.update_arp_held();
        } else {
            self.alloc_on(note, vel);
        }
    }

    pub fn note_off(&mut self, note: u8) {
        let note = note.min(127);
        if self.kb.vel[note as usize] == 0 {
            return;
        }
        if self.pedal {
            self.sustained[note as usize] = true;
            return;
        }
        self.kb.release(note);
        if self.arp_on {
            self.update_arp_held();
        } else {
            self.alloc_off(note);
        }
    }

    pub fn sustain_pedal(&mut self, down: bool) {
        self.pedal = down;
        if !down {
            for n in 0..128u8 {
                if self.sustained[n as usize] {
                    self.sustained[n as usize] = false;
                    self.note_off(n);
                }
            }
        }
    }

    /// Pitch bend, -1..1.
    pub fn pitch_bend(&mut self, v: f32) {
        self.bend_target = v.clamp(-1.0, 1.0);
        self.controller(0xE000, v.clamp(-1.0, 1.0));
    }

    /// MIDI CC 0..127.
    pub fn midi_cc(&mut self, cc: u8, value: u8) {
        match cc {
            64 => self.sustain_pedal(value >= 64),
            120 | 123 => self.all_notes_off(),
            1..=11 => self.controller(0xB000 | cc as i32, value as f32 / 127.0),
            _ => {}
        }
    }

    /// Channel pressure 0..1.
    pub fn aftertouch(&mut self, v: f32) {
        self.controller(0xD000, v);
    }

    fn controller(&mut self, src: i32, value: f32) {
        for s in 0..2 {
            if self.ctl[s].src == src && self.ctl[s].value != value {
                self.ctl[s].value = value;
                let t = self.ctl[s].assign;
                if t >= 0 {
                    self.refresh(t as usize);
                }
            }
        }
    }

    pub fn all_notes_off(&mut self) {
        self.kb.clear();
        self.akeys.clear();
        self.sustained = [false; 128];
        self.arp_held.clear();
        let _ = self.arp.stop();
        for i in 0..MAX_VOICES {
            if self.voices[i].gate {
                self.voices[i].release(&self.vp);
            }
        }
    }

    /// Hard reset: silence everything, clear effect tails.
    pub fn panic(&mut self) {
        self.all_notes_off();
        for v in &mut self.voices {
            v.kill();
        }
        self.fx.reset();
    }

    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.is_sounding()).count()
    }

    // ---- parameter plumbing -------------------------------------------------------------

    fn modulated(&self, sy1: usize) -> f32 {
        let mut v = self.base[sy1];
        let Some(p) = params::by_sy1(sy1) else { return v };
        let span = (p.max - p.min + 1) as f32;
        let mut moved = false;
        for s in &self.ctl {
            if s.assign == sy1 as i32 && s.value != 0.0 && s.sens != 0.0 {
                v += span * s.value * s.sens;
                moved = true;
            }
        }
        if moved {
            v = v.clamp(p.min as f32, p.max as f32);
        }
        v
    }

    fn refresh(&mut self, sy1: usize) {
        self.eff[sy1] = self.modulated(sy1);
        self.apply(sy1);
    }

    fn refresh_ctl_targets(&mut self) {
        let old = [self.ctl[0].assign, self.ctl[1].assign];
        for s in 0..2 {
            let b = if s == 0 { (86, 50, 87) } else { (88, 51, 89) };
            self.ctl[s].src = self.base[b.0] as i32;
            self.ctl[s].sens = bipolar_sens(self.base[b.1]);
            let a = self.base[b.2] as i32;
            let valid = a >= 0 && params::by_sy1(a as usize).is_some_and(|p| p.modulatable);
            let a = if valid { a } else { -1 };
            if a != self.ctl[s].assign {
                self.ctl[s].value = 0.0;
            }
            self.ctl[s].assign = a;
        }
        for t in old.into_iter().chain([self.ctl[0].assign, self.ctl[1].assign]) {
            if t >= 0 {
                self.refresh(t as usize);
            }
        }
    }

    fn recompute_mix(&mut self) {
        let m = x01(self.eff[5]);
        let sub = self.eff[95] * 4.0 / 127.0;
        let n = (sub + 1.0) * (1.0 - m) + m;
        self.vp.g1 = (1.0 - m) / n;
        self.vp.g2 = m / n;
        self.vp.gsub = sub * (1.0 - m) / n;
    }

    fn update_lfo_rate(&mut self, i: usize) {
        let c = self.lcfg[i];
        let hz = if c.sync { sync_rate(c.speed, self.bpm) } else { free_rate(c.speed) };
        self.lfo[i].set_rate(hz);
    }

    fn uni_offsets(&self, k: usize, n: usize) -> (f32, f32, f32) {
        if !self.uni_on || n <= 1 {
            return (0.0, 0.0, 0.0);
        }
        let d = unison_detune_cents(self.eff[75]);
        let kf = k as f32;
        let nf = n as f32;
        let pitch = if k % 2 == 1 { (self.eff[85].round() - 24.0) * 100.0 } else { 0.0 };
        let osc = d * kf / (nf - 1.0) - d / 2.0 + pitch;
        let flt = d * (kf / nf - 0.5);
        let sp = (self.eff[84] - 64.0) / 64.0;
        let pan = (kf / (nf - 1.0)) * 2.0 * sp - sp;
        (osc, flt, pan)
    }

    fn refresh_unison(&mut self) {
        let n = if self.uni_on { self.uni_n } else { 1 };
        let ph = x01(self.eff[92]);
        for i in 0..MAX_VOICES {
            let k = self.voices[i].uni_index;
            let (c, f, p) = self.uni_offsets(k, n);
            let v = &mut self.voices[i];
            v.uni_cents = c;
            v.uni_filter_cents = f;
            v.uni_pan = p;
            v.uni_phase = ph;
        }
    }

    fn active_count(&self) -> usize {
        match self.mode {
            PlayMode::Poly => self.polyphony,
            _ => (if self.uni_on { self.uni_n } else { 1 }).min(self.polyphony.max(1)).max(1),
        }
    }

    fn apply(&mut self, sy1: usize) {
        let v = self.eff[sy1];
        let vi = v.round() as i32;
        let x = x01(v);
        let sr = self.sr;
        let vp = &mut self.vp;
        match sy1 {
            0 => vp.osc1_wave = [Wave::Sine, Wave::Saw, Wave::Pulse, Wave::Triangle][vi.clamp(0, 3) as usize],
            1 => vp.osc2_wave = [Wave::Saw, Wave::Saw, Wave::Pulse, Wave::Triangle, Wave::Noise][vi.clamp(0, 4) as usize],
            96 => vp.sub_wave = [Wave::Sine, Wave::Triangle, Wave::Saw, Wave::Square][vi.clamp(0, 3) as usize],
            97 => vp.sub_down = vi == 1,
            6 => vp.sync = vi != 0,
            7 => vp.ring = vi != 0,
            4 => vp.track = vi != 0,
            2 => vp.osc2_semis = (v * 120.0 / 127.0).floor() - 60.0,
            3 => vp.osc2_cents = fine_cents(vi),
            72 => vp.fine_cents = fine_cents(vi),
            9 => vp.key_shift = vi,
            5 | 95 => self.recompute_mix(),
            8 => vp.pw = x,
            45 => vp.fm = v,
            76 => vp.detune_cents = x * 20.0,
            10 => vp.menv_on = vi != 0,
            12 => vp.menv.attack = attack_time(v),
            13 => vp.menv.decay = decay_time(v),
            11 => vp.menv_amount = bipolar64(v),
            71 => {
                vp.menv_dest = vi.clamp(0, 2) as u8;
                vp.menv.linear = vi == 1;
            }
            91 => vp.phase = if vi <= 0 { None } else { Some((v - 1.0) / 126.0) },
            14 => vp.flt_mode = FilterMode::from_index(vi),
            19 => {
                self.cutoff_target = x * 1023.0;
                self.cutoff_step = ((self.cutoff_target - vp.cutoff_idx) / (0.05 * sr / CHUNK as f32)).abs();
            }
            20 => vp.reso = x,
            21 => vp.env_amount = ((v - 63.0) / 64.0).min(1.0),
            22 => vp.key_track = x,
            23 => vp.sat = ((x - 1.0) * 5.8).exp() * x,
            24 => vp.vel_to_filter = vi != 0,
            15 => vp.fenv.attack = attack_time(v),
            16 => vp.fenv.decay = decay_time(v),
            17 => vp.fenv.sustain = x,
            18 => vp.fenv.release = decay_time(v),
            25 => vp.aenv.attack = attack_time(v),
            26 => vp.aenv.decay = decay_time(v),
            27 => vp.aenv.sustain = amp_curve(v),
            28 => vp.aenv.release = decay_time(v),
            29 => vp.amp_gain = amp_curve(v),
            30 => vp.vel_sens = x,
            39 => vp.porta_time = if v <= 0.0 { 0.0 } else { 2.5 * x * (2.5 * (x - 1.0)).exp() },
            74 => vp.porta_auto = vi != 0,
            // effect block
            77 => self.fx.effector.on = vi != 0,
            78 => self.fx.effector.kind = EffectType::from_index(vi),
            79..=81 => self.fx.effector.set(self.eff[79], self.eff[80], self.eff[81]),
            61..=63 => self.fx.set_eq(self.eff[61], self.eff[62], self.eff[63]),
            60 => self.fx.set_tone(v),
            90 => {
                self.pan_base = v;
                self.fx.set_pan(v);
            }
            // delay
            65 => self.fx.delay.on = vi != 0,
            82 => self.fx.delay.kind = vi.clamp(0, 2) as u8,
            35 | 83 => self.fx.delay.set_time(self.eff[35].round() as i32, self.eff[83], self.bpm),
            36 => self.fx.delay.set_feedback(v),
            98 => self.fx.delay.set_tone(v),
            37 => self.fx.delay.set_dry_wet(v),
            // chorus
            66 => self.fx.chorus.on = vi != 0,
            64 => self.fx.chorus.set_kind(vi),
            52..=56 => self.fx.chorus.set(self.eff[52], self.eff[53], self.eff[54], self.eff[55], self.eff[56]),
            // voice
            38 => {
                let m = match vi {
                    1 => PlayMode::Mono,
                    2 => PlayMode::Legato,
                    _ => PlayMode::Poly,
                };
                if m != self.mode {
                    self.mode = m;
                    self.all_notes_off();
                }
            }
            94 => self.polyphony = (vi.clamp(1, MAX_VOICES as i32)) as usize,
            73 => {
                self.uni_on = vi != 0;
                self.refresh_unison();
            }
            93 => {
                self.uni_n = vi.clamp(2, 8) as usize;
                self.refresh_unison();
            }
            75 | 84 | 85 | 92 => self.refresh_unison(),
            40 => self.bend_range = v,
            50 | 51 | 86..=89 => {}
            // arp
            59 => {
                let on = vi != 0;
                if on != self.arp_on {
                    self.arp_on = on;
                    self.all_notes_off();
                }
            }
            31 => self.arp.kind = vi.clamp(1, 4),
            32 => self.arp.range = vi.clamp(0, 3),
            33 => self.arp.beat = vi.clamp(0, 18) as usize,
            34 => self.arp.gate = x,
            // LFOs
            57 | 42 | 41 | 43 | 44 | 67 | 68 => self.set_lfo(0, sy1, v),
            58 | 47 | 46 | 48 | 49 | 69 | 70 => self.set_lfo(1, sy1, v),
            _ => {}
        }
    }

    fn set_lfo(&mut self, i: usize, sy1: usize, v: f32) {
        let c = &mut self.lcfg[i];
        let vi = v.round() as i32;
        match sy1 {
            57 | 58 => c.on = vi != 0,
            42 | 47 => c.wave = vi.clamp(0, 5) as u8,
            41 | 46 => {
                let was_pan = c.dest == 7;
                c.dest = vi.clamp(1, 7) as u8;
                if was_pan && c.dest != 7 {
                    let base = self.pan_base;
                    self.fx.set_pan(base);
                }
            }
            43 | 48 => c.speed = v,
            44 | 49 => c.depth = v,
            67 | 69 => c.sync = vi != 0,
            68 | 70 => c.key = vi != 0,
            _ => {}
        }
        self.update_lfo_rate(i);
    }

    // ---- voice allocation ---------------------------------------------------------------

    fn update_arp_held(&mut self) {
        self.arp_held.clear();
        self.arp_held.extend(self.kb.stack.iter().copied());
        self.arp_held.sort_unstable();
        self.arp_held.dedup();
    }

    fn alloc_on(&mut self, note: u8, vel: u8) {
        match self.mode {
            PlayMode::Poly => self.poly_on(note, vel),
            _ => {
                self.akeys.press(note, vel);
                self.mono_update(true);
            }
        }
    }

    fn alloc_off(&mut self, note: u8) {
        match self.mode {
            PlayMode::Poly => {
                for i in 0..MAX_VOICES {
                    if self.voices[i].gate && self.voices[i].note == note {
                        self.voices[i].release(&self.vp);
                    }
                }
            }
            _ => {
                let was_top = self.akeys.stack.last() == Some(&note);
                self.akeys.release(note);
                if was_top || self.akeys.stack.is_empty() {
                    self.mono_update(false);
                }
            }
        }
    }

    fn poly_on(&mut self, note: u8, vel: u8) {
        let active = self.active_count();
        // Same-note retrigger: release voices already playing this note.
        for i in 0..MAX_VOICES {
            if self.voices[i].gate && self.voices[i].note == note {
                self.voices[i].release(&self.vp);
            }
        }
        let g = if self.uni_on { self.uni_n } else { 1 };
        for k in 0..g {
            // Round-robin over voices that are not gated (released voices count as free).
            let mut pick = None;
            for j in 1..=active {
                let s = (self.last_slot + j) % active;
                if !self.voices[s].gate {
                    pick = Some(s);
                    break;
                }
            }
            let s = pick.unwrap_or_else(|| {
                // Steal the oldest gated voice.
                (0..active).min_by_key(|&i| self.voices[i].age).unwrap_or(0)
            });
            self.last_slot = s;
            self.start_voice(s, note, vel, k, g, None);
        }
    }

    fn start_voice(&mut self, s: usize, note: u8, vel: u8, k: usize, g: usize, retrigger: Option<bool>) {
        let (c, f, p) = self.uni_offsets(k, g);
        self.age += 1;
        let ph = x01(self.eff[92]);
        let vp = &self.vp;
        let v = &mut self.voices[s];
        v.uni_index = k;
        v.uni_cents = c;
        v.uni_filter_cents = f;
        v.uni_pan = p;
        v.uni_phase = ph;
        let glide = if vp.porta_auto { v.gate } else { true };
        v.note_on(vp, note, vel, glide);
        let trig = retrigger.unwrap_or(true);
        if trig {
            v.trigger(vp);
        }
        v.age = self.age;
    }

    fn mono_update(&mut self, new_note: bool) {
        let g = self.active_count();
        let Some(&top) = self.akeys.stack.last() else {
            for i in 0..g {
                if self.voices[i].gate {
                    self.voices[i].release(&self.vp);
                }
            }
            return;
        };
        if new_note {
            self.mono_vel = self.akeys.vel[top as usize];
        } else if self.voices[0].gate && self.voices[0].note == top {
            return;
        }
        let vel = self.mono_vel;
        for k in 0..g {
            let retrig = match self.mode {
                PlayMode::Mono => true,
                _ => !self.voices[k].gate,
            };
            self.start_voice(k, top, vel, k, g, Some(retrig));
        }
        // Voices above the active count stay silent.
        for i in g..MAX_VOICES {
            if self.voices[i].gate {
                self.voices[i].release(&self.vp);
            }
        }
    }

    // ---- audio --------------------------------------------------------------------------

    fn lfo_mods(&mut self, n: usize) -> (VoiceMods, f32) {
        let mut m = VoiceMods { amp: 1.0, ..Default::default() };
        let mut pan = 0.0;
        for i in 0..2 {
            let c = self.lcfg[i];
            let w = self.lfo[i].tick_n(c.wave, self.sr, n);
            if !c.on || c.depth <= 0.0 {
                continue;
            }
            let x = x01(c.depth);
            let d = if c.dest <= 2 { (10f32.powf(x) - 1.0) / 9.0 } else { x };
            let u = 0.5 * (w + 1.0) * d;
            match c.dest {
                1 => m.pitch2 += w * 60.0 * d,
                2 => {
                    m.pitch12 += w * 60.0 * d;
                    if !self.vp.track {
                        m.pitch2 += w * 60.0 * d;
                    }
                }
                3 => m.filter += u,
                4 => m.amp *= 1.0 - u,
                5 => m.pw += w * d,
                6 => m.fm += u,
                _ => pan += w * 64.0 * d,
            }
        }
        m.filter = m.filter.min(1.0);
        (m, pan)
    }

    /// Render a block. MIDI events must be applied between calls (split blocks at events).
    pub fn process(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let ftz = enable_ftz();
        let n = out_l.len().min(out_r.len());
        let mut pos = 0;
        while pos < n {
            let len = CHUNK.min(n - pos);
            self.process_chunk(&mut out_l[pos..pos + len], &mut out_r[pos..pos + len]);
            pos += len;
        }
        restore_ftz(ftz);
    }

    fn process_chunk(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let n = out_l.len();
        // Arpeggiator events (quantised to the control chunk).
        if self.arp_on {
            let o = self.arp.advance(n, self.sr, self.bpm, &self.arp_held);
            if let Some(off) = o.off {
                self.alloc_off(off);
            }
            if let Some(on) = o.on {
                let vel = self.arp.velocity;
                self.alloc_on(on, vel);
            }
        }
        // Bend: slew toward the target (~4 ms).
        let k = 1.0 - (-(n as f32) / (0.004 * self.sr)).exp();
        self.bend += (self.bend_target - self.bend) * k;
        // Cutoff knob glide (50 ms linear).
        let c = self.vp.cutoff_idx;
        let t = self.cutoff_target;
        if c != t {
            let s = self.cutoff_step.max(1e-3);
            self.vp.cutoff_idx = if (t - c).abs() <= s { t } else { c + s * (t - c).signum() };
        }
        let (mut m, pan_mod) = self.lfo_mods(n);
        m.bend = self.bend * self.bend_range;
        if pan_mod != 0.0 {
            self.fx.set_pan((self.pan_base + pan_mod).clamp(0.0, 127.0));
        }

        let (l, r) = (&mut self.l[..n], &mut self.r[..n]);
        l.fill(0.0);
        r.fill(0.0);
        for v in self.voices.iter_mut() {
            v.render(&self.vp, &m, self.sr, l, r);
        }
        for i in 0..n {
            let (a, b) = self.fx.tick(self.l[i], self.r[i]);
            let g = self.master.next();
            out_l[i] = a * g;
            out_r[i] = b * g;
        }
    }
}

fn fine_cents(v: i32) -> f32 {
    if v < 62 {
        (v - 62) as f32
    } else if v <= 66 {
        0.0
    } else {
        (v - 66) as f32
    }
}

/// MIDI-controller sensitivity: ±1 with a dead centre at 63/64.
fn bipolar_sens(v: f32) -> f32 {
    if v <= 63.0 {
        (v - 63.0) / 63.0
    } else {
        (v - 64.0) / 63.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(s: &mut Synth, secs: f32) -> (Vec<f32>, Vec<f32>) {
        let n = (s.sample_rate() * secs) as usize;
        let mut l = vec![0.0; n];
        let mut r = vec![0.0; n];
        for (cl, cr) in l.chunks_mut(256).zip(r.chunks_mut(256)) {
            s.process(cl, cr);
        }
        (l, r)
    }

    #[test]
    fn init_patch_plays() {
        let mut s = Synth::new(48_000.0);
        s.note_on(60, 100);
        s.note_on(64, 100);
        s.note_on(67, 100);
        let (l, _) = render(&mut s, 0.5);
        let peak = l.iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak > 0.05 && peak < 4.0, "peak {peak}");
        assert_eq!(s.active_voices(), 3);
        s.all_notes_off();
        let _ = render(&mut s, 3.0);
        assert_eq!(s.active_voices(), 0);
    }

    #[test]
    fn unison_and_mono() {
        let mut s = Synth::new(44_100.0);
        s.set_param(73, 1.0);
        s.set_param(93, 4.0);
        s.note_on(48, 90);
        let _ = render(&mut s, 0.1);
        assert_eq!(s.active_voices(), 4);
        s.set_param(38, 1.0);
        s.note_on(50, 90);
        s.note_on(53, 90);
        let _ = render(&mut s, 0.1);
        assert_eq!(s.voices.iter().filter(|v| v.gate).count(), 4);
        assert!(s.voices.iter().filter(|v| v.gate).all(|v| v.note == 53));
        s.note_off(53);
        assert!(s.voices.iter().filter(|v| v.gate).all(|v| v.note == 50));
    }

    #[test]
    fn voice_stealing_respects_polyphony() {
        let mut s = Synth::new(48_000.0);
        s.set_param(94, 4.0);
        for n in 60..70 {
            s.note_on(n, 100);
        }
        let _ = render(&mut s, 0.05);
        assert!(s.active_voices() <= 4);
        assert!(s.voices.iter().filter(|v| v.gate).all(|v| v.note >= 66));
    }

    #[test]
    fn arp_plays_notes() {
        let mut s = Synth::new(48_000.0);
        s.set_param(59, 1.0);
        s.note_on(60, 100);
        s.note_on(64, 100);
        let (l, _) = render(&mut s, 1.0);
        assert!(l.iter().any(|v| v.abs() > 0.01));
    }

    #[test]
    fn mod_wheel_moves_target() {
        let mut s = Synth::new(48_000.0);
        // Template: ctl1 = mod wheel -> lfo1 depth (44), sens 74.
        assert_eq!(s.lcfg[0].depth, 0.0);
        s.midi_cc(1, 127);
        assert!(s.lcfg[0].depth > 10.0, "{}", s.lcfg[0].depth);
        s.midi_cc(1, 0);
        assert_eq!(s.lcfg[0].depth, 0.0);
    }

    #[test]
    fn master_gain_curve() {
        assert!((20.0 * master_gain(64.0).log10() + 1.67).abs() < 0.05);
        assert!((20.0 * master_gain(127.0).log10() - 9.2).abs() < 0.1);
    }
}
