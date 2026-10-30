// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! One synth voice: oscillators (incl. 8-way osc 1 detune and sub), mod envelope, filter with
//! its ADSR, post-filter limiter/saturation and the amp ADSR, with band-limited oscillators and
//! ZDF filters.

use crate::env::{EnvParams, Envelope};
use crate::filter::{FilterMode, VoiceFilter};
use crate::osc::{Osc, Wave};
use crate::util::{fast_exp2, note_to_hz, Rng};

/// Cutoff curve: index 0..1024 → 15 Hz · 1066.67^(i/1024) (15 Hz .. 16 kHz).
#[inline]
pub fn cutoff_hz(idx: f32) -> f32 {
    // log2(1066.6667) = 10.0589
    15.0 * fast_exp2(idx * (10.058_9 / 1024.0))
}

/// Cents → cutoff index units.
#[inline]
pub fn cents_to_cutoff_idx(cents: f32) -> f32 {
    cents / 1200.0 * (1024.0 / 10.058_9)
}

/// Patch-wide voice settings, derived from parameters by the synth.
#[derive(Clone, Debug)]
pub struct VoiceParams {
    pub osc1_wave: Wave,
    pub osc2_wave: Wave,
    pub sub_wave: Wave,
    pub sub_down: bool,
    pub sync: bool,
    pub ring: bool,
    pub track: bool,
    /// osc 2 coarse, semitones (integer -60..60, glided per voice).
    pub osc2_semis: f32,
    pub osc2_cents: f32,
    pub fine_cents: f32,
    pub key_shift: i32,
    pub g1: f32,
    pub g2: f32,
    pub gsub: f32,
    /// Pulse width 0..1 (knob); duty = pw/2 after modulation.
    pub pw: f32,
    /// FM knob 0..127.
    pub fm: f32,
    pub detune_cents: f32,
    pub menv_on: bool,
    pub menv_amount: f32,
    pub menv_dest: u8,
    pub menv: EnvParams,
    /// 0 = free running; >0 = reset at note-on with this phase spread (0..1).
    pub phase: Option<f32>,

    pub flt_mode: FilterMode,
    /// Base cutoff index 0..1023 (glided).
    pub cutoff_idx: f32,
    pub reso: f32,
    pub env_amount: f32,
    pub key_track: f32,
    pub sat: f32,
    pub vel_to_filter: bool,
    pub fenv: EnvParams,

    pub aenv: EnvParams,
    pub amp_gain: f32,
    pub vel_sens: f32,

    pub porta_time: f32,
    pub porta_auto: bool,
}

impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            osc1_wave: Wave::Pulse,
            osc2_wave: Wave::Saw,
            sub_wave: Wave::Triangle,
            sub_down: true,
            sync: false,
            ring: false,
            track: true,
            osc2_semis: 0.0,
            osc2_cents: 0.0,
            fine_cents: 0.0,
            key_shift: 0,
            g1: 0.5,
            g2: 0.5,
            gsub: 0.0,
            pw: 0.5,
            fm: 0.0,
            detune_cents: 0.0,
            menv_on: false,
            menv_amount: 0.0,
            menv_dest: 0,
            menv: EnvParams { one_shot: true, sustain: 0.0, ..Default::default() },
            phase: None,
            flt_mode: FilterMode::Lp24,
            cutoff_idx: 655.0,
            reso: 0.1,
            env_amount: 0.0,
            key_track: 0.5,
            sat: 0.0,
            vel_to_filter: true,
            fenv: EnvParams::default(),
            aenv: EnvParams { short_release: true, ..Default::default() },
            amp_gain: 0.68,
            vel_sens: 0.5,
            porta_time: 0.0,
            porta_auto: false,
        }
    }
}

/// Per-chunk modulation shared by all voices (LFOs, bend).
#[derive(Clone, Copy, Debug, Default)]
pub struct VoiceMods {
    /// Semitones added to osc 1 (and osc 2 when key-tracked): bend + LFO "osc 1+2".
    pub pitch12: f32,
    /// Semitones added to osc 2 only (LFO "osc 2" and "osc 1+2").
    pub pitch2: f32,
    pub bend: f32,
    /// PW modulation, ±1 = full range.
    pub pw: f32,
    /// FM modulation, ±1 = full sweep.
    pub fm: f32,
    /// Filter modulation, ±1 of the remaining headroom.
    pub filter: f32,
    /// Amp attenuation factor (0..1).
    pub amp: f32,
}

const DETUNE_K: [f32; 8] = [-7.0, -5.0, -3.0, -1.0, 1.0, 3.0, 5.0, 7.0];

#[derive(Clone, Debug)]
pub struct Voice {
    pub note: u8,
    pub velocity: u8,
    pub gate: bool,
    /// Allocation stamp (for oldest-voice stealing).
    pub age: u64,
    pub enabled: bool,

    osc1: Osc,
    det: [Osc; 8],
    sub: Osc,
    osc2: Osc,
    filter: VoiceFilter,
    menv: Envelope,
    fenv: Envelope,
    aenv: Envelope,

    // unison
    pub uni_cents: f32,
    pub uni_filter_cents: f32,
    pub uni_pan: f32,
    pub uni_phase: f32,
    pub uni_index: usize,

    // portamento (semitones)
    porta_offset0: f32,
    porta_t: f32,
    prev_note: Option<u8>,

    osc2_semis: f32,
    osc2_glide_from: f32,
    osc2_glide_t: f32,

    // filter state
    key_ofs: f32,
    vel_curve: f32,
    vel_gain: f32,
    limiter: f32,
    last_gain: f32,
    pending_sync: Option<f32>,
    rng: Rng,
}

impl Voice {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed.wrapping_mul(2_654_435_761).max(1));
        let mut det: [Osc; 8] = core::array::from_fn(|i| Osc::new(seed * 31 + i as u32 + 7));
        for o in &mut det {
            o.reset(rng.unipolar());
        }
        let mut osc1 = Osc::new(seed * 17 + 1);
        osc1.reset(rng.unipolar());
        let mut osc2 = Osc::new(seed * 19 + 3);
        osc2.reset(rng.unipolar());
        Self {
            note: 60,
            velocity: 100,
            gate: false,
            age: 0,
            enabled: true,
            osc1,
            det,
            sub: Osc::new(seed * 23 + 5),
            osc2,
            filter: VoiceFilter::default(),
            menv: Envelope::default(),
            fenv: Envelope::default(),
            aenv: Envelope::default(),
            uni_cents: 0.0,
            uni_filter_cents: 0.0,
            uni_pan: 0.0,
            uni_phase: 0.0,
            uni_index: 0,
            porta_offset0: 0.0,
            porta_t: 1.0,
            prev_note: None,
            osc2_semis: 0.0,
            osc2_glide_from: 0.0,
            osc2_glide_t: 1.0,
            key_ofs: 0.0,
            vel_curve: 1.0,
            vel_gain: 1.0,
            limiter: 0.0,
            last_gain: 0.0,
            pending_sync: None,
            rng,
        }
    }

    pub fn is_sounding(&self) -> bool {
        self.aenv.is_active()
    }

    /// Set the note (pitch). `glide` enables portamento from the
    /// previous note of this voice.
    pub fn note_on(&mut self, p: &VoiceParams, note: u8, velocity: u8, glide: bool) {
        if glide && p.porta_time > 0.0 {
            if let Some(prev) = self.prev_note {
                let cur = self.porta_current(p);
                self.porta_offset0 = cur + (prev as f32 - note as f32);
                self.porta_t = 0.0;
            }
        } else {
            self.porta_offset0 = 0.0;
            self.porta_t = 1.0;
        }
        self.prev_note = Some(note);
        self.note = note;
        self.velocity = velocity;
    }

    /// Gate on: start envelopes.
    pub fn trigger(&mut self, p: &VoiceParams) {
        self.gate = true;
        let v = self.velocity as f32;
        self.vel_gain = 10f32.powf(-1.5 * p.vel_sens * (127.0 - v) / 127.0);
        self.vel_curve = ((1.0 - (-1.1 * v / 127.0).exp()) / (1.0 - (-1.3f32).exp())).min(1.0);
        // Key tracking (relative to note 48, computed at note-on).
        let n = (self.note as i32 + p.key_shift) as f32;
        self.key_ofs = if p.key_track > 0.0 {
            let oct = (n - 48.0) / 12.0 * (p.key_track + 1.0).log2();
            oct * (1024.0 / 10.058_9)
        } else {
            0.0
        };
        if let Some(ph) = p.phase {
            let u = self.uni_phase;
            self.osc1.reset(u * self.rng.unipolar());
            self.osc2.reset(ph * 0.5 + u * self.rng.unipolar());
            self.sub.reset(self.osc1.phase);
            for o in &mut self.det {
                o.reset(ph * self.rng.unipolar() + u * self.rng.unipolar());
            }
        }
        self.osc2_semis = p.osc2_semis;
        self.osc2_glide_t = 1.0;
        self.menv.gate_on();
        self.fenv.gate_on();
        self.aenv.gate_on();
    }

    pub fn release(&mut self, p: &VoiceParams) {
        self.gate = false;
        self.aenv.gate_off(&p.aenv);
        self.fenv.gate_off(&p.fenv);
        self.menv.gate_off(&p.menv);
    }

    pub fn kill(&mut self) {
        self.gate = false;
        self.aenv.kill();
        self.fenv.kill();
        self.menv.kill();
        self.filter.reset();
        self.last_gain = 0.0;
    }

    fn porta_current(&self, p: &VoiceParams) -> f32 {
        if self.porta_t >= 1.0 || p.porta_time <= 0.0 {
            0.0
        } else {
            let t = self.porta_t;
            self.porta_offset0 * (-4.0 * t).exp() * (1.0 - t)
        }
    }

    /// Render `n` samples (one control chunk) and add into `l`/`r`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, p: &VoiceParams, m: &VoiceMods, sr: f32, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        if !self.aenv.is_active() {
            return;
        }
        let dt = 1.0 / sr;
        let chunk_s = n as f32 * dt;

        // ---- control-rate values ---------------------------------------------------------
        // Envelopes (advanced sample-accurately inside the loop for the amp; the mod and
        // filter envelopes are sampled at control rate).
        let menv = self.menv.level;
        let fenv = self.fenv.level;
        for _ in 0..n {
            self.menv.tick(&p.menv, dt);
            self.fenv.tick(&p.fenv, dt);
        }
        let menv_v = if p.menv_on { menv * p.menv_amount } else { 0.0 };

        // Portamento.
        let porta = self.porta_current(p);
        if self.porta_t < 1.0 {
            self.porta_t = (self.porta_t + chunk_s / p.porta_time.max(1e-4)).min(1.0);
        }
        // osc2 coarse glides 50 ms on change.
        if p.osc2_semis != self.osc2_semis && self.osc2_glide_t >= 1.0 {
            self.osc2_glide_from = self.osc2_semis;
            self.osc2_glide_t = 0.0;
        }
        let osc2_semis = if self.osc2_glide_t < 1.0 {
            self.osc2_glide_t = (self.osc2_glide_t + chunk_s / 0.05).min(1.0);
            let s = self.osc2_glide_from + (p.osc2_semis - self.osc2_glide_from) * self.osc2_glide_t;
            if self.osc2_glide_t >= 1.0 {
                self.osc2_semis = p.osc2_semis;
            }
            s
        } else {
            self.osc2_semis
        };

        let note = (self.note as i32 + p.key_shift).clamp(0, 127) as f32;
        let base12 = porta + m.bend + m.pitch12;
        let f1 = note_to_hz(note) * fast_exp2((p.fine_cents + self.uni_cents) / 1200.0 + base12 / 12.0);
        let osc2_pitch = osc2_semis + p.osc2_cents / 100.0 + m.pitch2 + if p.menv_dest == 0 { menv_v * 60.0 } else { 0.0 };
        let f2 = if p.track {
            f1 * fast_exp2(osc2_pitch / 12.0)
        } else {
            220.0 * fast_exp2((p.fine_cents + self.uni_cents) / 1200.0 + osc2_pitch / 12.0)
        };
        let nyq = sr * 0.45;
        let dt1 = f1.min(nyq) / sr;
        let dt2 = f2.min(nyq) / sr;

        // PW (pw in 0..2, duty = pw/2).
        let pwm = m.pw + if p.menv_dest == 2 { menv_v } else { 0.0 };
        let pw0 = p.pw;
        let pw = if pwm <= 0.0 { pw0 * (1.0 + pwm.max(-1.0)) } else { pw0 + (2.0 - pw0) * pwm.min(1.0) };
        let duty = (pw.clamp(0.01, 1.99)) * 0.5;

        // FM depth (cycles per sample per unit of osc 2).
        let fmm = m.fm + if p.menv_dest == 1 { menv_v } else { 0.0 };
        let fm = if fmm > 0.0 { p.fm + fmm.min(1.0) * (127.0 - p.fm) } else { p.fm + fmm.max(-1.0) * p.fm };
        let fm_depth = if p.ring || fm <= 0.0 { 0.0 } else { 0.5 * (1000f32.powf(fm / 127.0) - 1.0) / 999.0 };

        // Mix gains with per-waveform level compensation.
        let detune = p.detune_cents > 0.0;
        let mut g1 = p.g1;
        let mut g2 = p.g2;
        let mut gsub = p.gsub;
        if !p.ring && p.osc1_wave == Wave::Pulse {
            g1 *= 0.5;
        }
        if detune && !p.ring {
            g1 *= 0.3;
            gsub *= 0.3 * 3.0; // single sub, matching the level of 9 summed copies
        }
        if fm_depth == 0.0 {
            if p.osc2_wave == Wave::Pulse {
                g2 *= 0.5;
            } else if p.osc2_wave == Wave::Noise {
                g2 *= 0.65;
            }
        }
        if p.sub_wave == Wave::Square {
            gsub *= 0.5;
        }
        let det_dt: [f32; 8] = core::array::from_fn(|i| {
            if detune {
                (dt1 * fast_exp2(DETUNE_K[i] * p.detune_cents / 1200.0)).min(0.45)
            } else {
                0.0
            }
        });

        // Filter cutoff index for this chunk.
        let mut amt = p.env_amount;
        if p.vel_to_filter {
            amt *= self.vel_curve;
        }
        let cents_ofs = cents_to_cutoff_idx(self.uni_filter_cents);
        let base = p.cutoff_idx + self.key_ofs + cents_ofs;
        let room = (1024.0 - base) / 1024.0;
        let amt = amt.clamp(room - 1.0, room);
        let fenv_avg = 0.5 * (fenv + self.fenv.level);
        let mut idx = base + fenv_avg * 1024.0 * amt;
        let fm_ = m.filter;
        idx += if fm_ > 0.0 { (1024.0 - idx) * fm_ } else { idx * fm_ };
        let cutoff = cutoff_hz(idx.clamp(0.0, 1023.0));
        let drive = 1.0;

        // Amp gain: ramp linearly across the chunk.
        let pan = self.uni_pan;
        let (pl, pr) = (if pan > 0.0 { 1.0 - pan } else { 1.0 }, if pan < 0.0 { 1.0 + pan } else { 1.0 });
        let sat100 = p.sat * 100.0;
        let lim_c = (-1.0 / (0.005 * sr)).exp();

        for i in 0..n {
            // osc 2 runs first: it is the FM modulator. Every oscillator outputs one sample
            // late, so osc 1's wrap from the previous tick is applied to osc 2 now, with its
            // sub-sample position, and lines up exactly with osc 1's delayed output.
            let sync = if p.sync { self.pending_sync.take() } else { None };
            let (s2, _) = self.osc2.tick(p.osc2_wave, dt2, duty, sync);
            let (s1, wrap1) = self.osc1.tick(p.osc1_wave, dt1 + fm_depth * s2, duty, None);
            self.pending_sync = wrap1;
            let mut o1 = s1;
            if detune {
                let fmv = fm_depth * s2;
                for k in 0..8 {
                    o1 += self.det[k].tick(p.osc1_wave, det_dt[k] + fmv, duty, None).0;
                }
            }
            let sub = if gsub > 0.0 {
                let ds = (dt1 + fm_depth * s2) * if p.sub_down { 0.5 } else { 1.0 };
                self.sub.tick(p.sub_wave, ds, 0.5, None).0
            } else {
                0.0
            };
            let mix = if p.ring { s1 * (g1 + g2 * s2) } else { g1 * o1 + g2 * s2 } + gsub * sub;

            // Filter.
            let mut y = self.filter.tick(p.flt_mode, mix, cutoff, p.reso, drive, sr);
            // Soft limiter at 1.1 (5 ms), then saturation.
            let e = (y.abs() - 1.1).max(0.0);
            self.limiter = lim_c * self.limiter + (1.0 - lim_c) * e;
            if self.limiter > 1e-6 {
                y *= 1.1 / (self.limiter + 1.1);
            }
            if sat100 > 0.0 {
                y = y * (1.0 + sat100) / (1.0 + sat100 * y.abs());
            }

            // Amp.
            let a = self.aenv.tick(&p.aenv, dt);
            let g = a * p.amp_gain * self.vel_gain * m.amp;
            let y = y * g;
            l[i] += y * pl;
            r[i] += y * pr;
        }
        self.last_gain = self.aenv.level;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_curve_spans_15hz_to_16khz() {
        assert!((cutoff_hz(0.0) - 15.0).abs() < 0.01);
        assert!((cutoff_hz(1024.0) - 16_000.0).abs() < 5.0);
    }

    #[test]
    fn voice_renders_and_releases() {
        let p = VoiceParams::default();
        let mut v = Voice::new(1);
        v.note_on(&p, 60, 100, false);
        v.trigger(&p);
        let sr = 48_000.0;
        let mut l = [0.0f32; 16];
        let mut r = [0.0f32; 16];
        let mut peak = 0.0f32;
        for _ in 0..3000 {
            l.fill(0.0);
            r.fill(0.0);
            v.render(&p, &VoiceMods { amp: 1.0, ..Default::default() }, sr, &mut l, &mut r);
            peak = peak.max(l.iter().fold(0.0f32, |a, b| a.max(b.abs())));
            assert!(l.iter().all(|x| x.is_finite()));
        }
        assert!(peak > 0.05, "peak {peak}");
        v.release(&p);
        for _ in 0..1000 {
            l.fill(0.0);
            r.fill(0.0);
            v.render(&p, &VoiceMods { amp: 1.0, ..Default::default() }, sr, &mut l, &mut r);
        }
        assert!(!v.is_sounding());
    }
}
