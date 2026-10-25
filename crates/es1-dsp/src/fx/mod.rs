//! Master effect chain, after Synth1's `CSynthesizer1` device graph:
//!
//! voice sum → effect block (dist / lo-fi / ring / comp / phaser) → peak EQ → tone → pan
//! → tempo delay (tone filter in the loop) → chorus/flanger.
//!
//! As in Synth1, when the chorus is on the delay's wet signal is fed *through* the chorus line;
//! otherwise it is added to the output directly.

pub mod prim;

use prim::{Allpass1, Biquad};

#[inline]
fn x01(v: f32) -> f32 {
    (v / 127.0).clamp(0.0, 1.0)
}

#[inline]
fn db(g: f32) -> f32 {
    10f32.powf(g / 20.0)
}

// ------------------------------------------------------------------------------------------
// Effect block (Synth1 "effect" section, types 0..9)
// ------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectType {
    /// 3-stage tube-style drive.
    Drive1,
    /// Soft clip.
    Drive2,
    /// Wave folder.
    Fold,
    Decimator,
    RingMod,
    Compressor,
    Phaser(usize),
}

impl EffectType {
    pub fn from_index(i: i32) -> Self {
        match i {
            0 => Self::Drive1,
            1 => Self::Drive2,
            2 => Self::Fold,
            3 => Self::Decimator,
            4 => Self::RingMod,
            5 => Self::Compressor,
            6 => Self::Phaser(1),
            7 => Self::Phaser(2),
            8 => Self::Phaser(4),
            _ => Self::Phaser(6),
        }
    }
}

pub const EFFECT_TYPE_NAMES: [&str; 10] = ["AD1", "AD2", "DIG", "LoFi", "Ring", "Comp", "Ph 1", "Ph 2", "Ph 4", "Ph 6"];

#[derive(Clone, Debug, Default)]
struct TubeStage {
    bias: f32,
    out: f32,
    hp: f32,
    lp: f32,
}

#[derive(Clone, Debug, Default)]
struct DistChannel {
    st: [TubeStage; 3],
    lpf: Biquad,
    hpf: Biquad,
}

#[inline]
fn tube_t(v: f32) -> f32 {
    if v >= 0.0 {
        1.0 - 1.0 / (2.0 * v * v * (v + 0.4) + 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Debug)]
struct PhaserChannel {
    ap: [Allpass1; 12],
    yprev: f32,
}

impl Default for PhaserChannel {
    fn default() -> Self {
        Self { ap: [Allpass1::default(); 12], yprev: 0.0 }
    }
}

#[derive(Clone, Debug)]
pub struct Effector {
    pub on: bool,
    pub kind: EffectType,
    sr: f32,
    ctl1: f32,
    ctl2: f32,
    level: f32,
    // distortion
    drive: f32,
    dist_norm: [f32; 3],
    out_gain: f32,
    dist: [DistChannel; 2],
    kb: [f32; 3],
    kh: f32,
    kl: f32,
    // decimator
    dec_counter: u32,
    dec_held: [f32; 2],
    // ring
    ring_s: f32,
    ring_c: f32,
    ring_w: f32,
    // compressor
    comp_t: f32,
    comp_makeup: f32,
    comp_c: f32,
    comp_env: [f32; 2],
    comp_out: f32,
    // phaser
    ph: [PhaserChannel; 2],
    ph_f: f32,
    ph_step: f32,
    ph_fmin: f32,
    ph_fmax: f32,
    ph_up: bool,
}

impl Effector {
    pub fn new(sr: f32) -> Self {
        let mut e = Self {
            on: false,
            kind: EffectType::Drive1,
            sr,
            ctl1: 64.0,
            ctl2: 64.0,
            level: 64.0,
            drive: 1.0,
            dist_norm: [1.0; 3],
            out_gain: 0.5,
            dist: Default::default(),
            kb: [0.0; 3],
            kh: 0.0,
            kl: 0.0,
            dec_counter: 0,
            dec_held: [0.0; 2],
            ring_s: 0.0,
            ring_c: 1.0,
            ring_w: 0.0,
            comp_t: 0.1,
            comp_makeup: 1.0,
            comp_c: 0.0,
            comp_env: [0.0; 2],
            comp_out: 1.0,
            ph: Default::default(),
            ph_f: 1000.0,
            ph_step: 1.0,
            ph_fmin: 100.0,
            ph_fmax: 1000.0,
            ph_up: true,
        };
        e.update();
        e
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.update();
    }

    pub fn set(&mut self, ctl1: f32, ctl2: f32, level: f32) {
        if ctl1 != self.ctl1 || ctl2 != self.ctl2 || level != self.level {
            self.ctl1 = ctl1;
            self.ctl2 = ctl2;
            self.level = level;
            self.update();
        }
    }

    fn update(&mut self) {
        let sr = self.sr;
        let (x1, x2, xl) = (x01(self.ctl1), x01(self.ctl2), x01(self.level));
        // Distortion: drive -10..+60 dB, out level (v+2)/129, tone LPF 400*40^x2 Hz.
        self.drive = db(70.0 * x1 - 10.0);
        self.out_gain = (self.level + 2.0) / 129.0;
        let g = self.drive;
        self.dist_norm[0] = 2.0 / tube_t(g).max(1e-4);
        self.dist_norm[1] = 1.0 / (1.0 - 1.0 / ((g + 1.0) * (g + 1.0) * g + 1.0)).max(1e-4);
        self.dist_norm[2] = 1.0 / 0.7;
        for ch in &mut self.dist {
            ch.lpf.set_lowpass(sr, (400.0 * 40f32.powf(x2)).min(sr * 0.45), 3.0f32.sqrt() * 0.5 + 0.2);
            ch.hpf.set_highpass(sr, 100.0, 0.707);
        }
        self.kb = [540.354 / sr, 829.380 / sr, 1218.938 / sr];
        self.kh = 1.0 / (194.779 / sr + 1.0);
        self.kl = (41_035.48 / sr).min(1.0);
        // Ring carrier 1..8000 Hz.
        let fc = 8000f32.powf(x1);
        self.ring_w = 2.0 * (core::f32::consts::PI * fc / sr).sin();
        // Compressor.
        let tdb = -20.0 - 40.0 * x1;
        self.comp_t = db(tdb);
        self.comp_makeup = db(-20.0 - tdb);
        let tau = 0.1 * 2000f32.powf(x2.sqrt()) * 0.001;
        self.comp_c = (-1.0 / (tau * sr)).exp();
        self.comp_out = db(30.0 * xl - 20.0);
        // Phaser sweep.
        self.ph_fmin = 100.0 * 10f32.powf(1.0 - x1);
        self.ph_fmax = (1000.0 * 10f32.powf(x1)).min(sr * 0.2);
        let rate = if self.ctl2 <= 0.0 { 0.01 } else { 0.02 * 1000f32.powf(x2) };
        self.ph_step = ((self.ph_fmax / self.ph_fmin).ln() / (0.5 * sr / rate)).exp();
        self.ph_f = self.ph_f.clamp(self.ph_fmin, self.ph_fmax);
    }

    #[inline]
    fn dist_tick(&mut self, ch: usize, x: f32) -> f32 {
        let g = self.drive;
        let mode = self.kind;
        let n = match mode {
            EffectType::Drive1 => 0,
            EffectType::Drive2 => 1,
            _ => 2,
        };
        let y = match mode {
            EffectType::Drive1 => {
                let (kb, kh, kl) = (self.kb, self.kh, self.kl);
                let c = &mut self.dist[ch];
                let mut inp = g * x;
                for (i, s) in c.st.iter_mut().enumerate() {
                    let target = if i == 0 { (1.0 - s.out) * 5.0 } else { 1.0 - s.out };
                    s.bias += (target - s.bias) * kb[i];
                    let o = tube_t(inp + s.bias);
                    s.hp = (o - s.out + s.hp) * kh;
                    s.out = o;
                    if i < 2 {
                        s.lp += (s.hp - s.lp) * kl;
                        inp = s.lp;
                    } else {
                        inp = s.hp;
                    }
                }
                inp
            }
            EffectType::Drive2 => {
                let u = (x + 1e-4) * g;
                let a = u.abs();
                u.signum() * (1.0 - 1.0 / ((a + 1.0) * (a + 1.0) * a + 1.0))
            }
            _ => {
                let t = 0.7;
                let mut u = g * x;
                if u.abs() > t {
                    u = ((u - t).rem_euclid(4.0 * t) - 2.0 * t).abs() - t;
                }
                u
            }
        };
        let c = &mut self.dist[ch];
        let y = y * self.dist_norm[n] * self.out_gain;
        c.hpf.tick(c.lpf.tick(y))
    }

    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        if !self.on {
            return (l, r);
        }
        match self.kind {
            EffectType::Drive1 | EffectType::Drive2 | EffectType::Fold => (self.dist_tick(0, l), self.dist_tick(1, r)),
            EffectType::Decimator => {
                let hold = self.ctl1.max(0.0) as u32 + 1;
                if self.dec_counter.is_multiple_of(hold) {
                    let bits = ((self.ctl2.max(0.0) as i32) >> 3) + 1;
                    let mask = -1i32 << bits;
                    let q = |x: f32| ((x.clamp(-1.0, 1.0) * 65536.0) as i32 & mask) as f32 / 65536.0;
                    self.dec_held = [q(l), q(r)];
                }
                self.dec_counter = self.dec_counter.wrapping_add(1);
                let m = x01(self.level);
                (m * self.dec_held[0] + (1.0 - m) * l, m * self.dec_held[1] + (1.0 - m) * r)
            }
            EffectType::RingMod => {
                self.ring_s -= self.ring_w * self.ring_c;
                self.ring_c += self.ring_w * self.ring_s;
                let m = x01(self.level);
                let g = m * self.ring_s + 1.0 - m;
                (l * g, r * g)
            }
            EffectType::Compressor => {
                let mut out = [l, r];
                for (i, x) in out.iter_mut().enumerate() {
                    let e = (x.abs() - self.comp_t).max(0.0);
                    self.comp_env[i] = self.comp_c * self.comp_env[i] + (1.0 - self.comp_c) * e;
                    *x = *x * self.comp_t / (self.comp_t + self.comp_env[i]) * self.comp_makeup * self.comp_out;
                }
                (out[0], out[1])
            }
            EffectType::Phaser(stages) => {
                if self.ph_up {
                    self.ph_f *= self.ph_step;
                    if self.ph_f >= self.ph_fmax {
                        self.ph_up = false;
                    }
                } else {
                    self.ph_f /= self.ph_step;
                    if self.ph_f <= self.ph_fmin {
                        self.ph_up = true;
                    }
                }
                let k = 4.0 * self.ph_f / self.sr;
                let a = (1.0 - k) / (1.0 + k);
                let fb = (self.level - 64.0) / 64.0 * 0.99;
                let mut out = [l, r];
                for (c, x) in out.iter_mut().enumerate() {
                    let ch = &mut self.ph[c];
                    let mut v = *x + fb * ch.yprev;
                    for ap in ch.ap.iter_mut().take(stages * 2) {
                        v = ap.tick(v, a);
                    }
                    ch.yprev = v;
                    *x = 0.5 * *x + 0.5 * v;
                }
                (out[0], out[1])
            }
        }
    }
}

// ------------------------------------------------------------------------------------------
// EQ, tone, pan
// ------------------------------------------------------------------------------------------

/// First-order bilinear filter used for the tone control (and the delay's loop filter).
#[derive(Clone, Copy, Debug, Default)]
pub struct Tone {
    b0: f32,
    b1: f32,
    a: f32,
    x1: [f32; 2],
    y1: [f32; 2],
    active: bool,
}

impl Tone {
    pub fn set_lp(&mut self, sr: f32, fc: f32) {
        if fc >= sr * 0.5 {
            self.active = false;
            return;
        }
        let wc = 2.0 * core::f32::consts::PI * fc;
        let k = 2.0 * sr;
        self.b0 = wc / (wc + k);
        self.b1 = self.b0;
        self.a = (k - wc) / (wc + k);
        self.active = true;
    }

    pub fn set_hp(&mut self, sr: f32, fc: f32) {
        let wc = 2.0 * core::f32::consts::PI * fc.min(sr * 0.45);
        let k = 2.0 * sr;
        self.b0 = k / (wc + k);
        self.b1 = -self.b0;
        self.a = (k - wc) / (wc + k);
        self.active = true;
    }

    pub fn set_off(&mut self) {
        self.active = false;
    }

    #[inline]
    pub fn tick(&mut self, ch: usize, x: f32) -> f32 {
        if !self.active {
            return x;
        }
        let y = self.b0 * x + self.b1 * self.x1[ch] + self.a * self.y1[ch];
        self.x1[ch] = x;
        self.y1[ch] = y;
        y
    }
}

// ------------------------------------------------------------------------------------------
// Tempo delay
// ------------------------------------------------------------------------------------------

/// Delay time divisions in ticks of 1/24 quarter note (Synth1 table at 0x180093a90).
pub const DELAY_TICKS: [u32; 20] = [0, 1, 2, 3, 4, 6, 8, 9, 12, 16, 18, 21, 24, 32, 36, 42, 48, 72, 84, 96];
pub const DELAY_TIME_NAMES: [&str; 20] = [
    "0", "1/96", "1/32T", "1/32", "1/16T", "1/16", "1/8T", "1/16.", "1/8", "1/4T", "1/8.", "1/8..", "1/4", "1/2T", "1/4.",
    "1/4..", "1/2", "1/2.", "1/2..", "1/1",
];
pub const DELAY_TYPE_NAMES: [&str; 3] = ["stereo", "cross", "ping-pong"];

const DELAY_FRAMES: usize = 1 << 18;

#[derive(Clone, Debug)]
pub struct Delay {
    pub on: bool,
    pub kind: u8,
    sr: f32,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    w: usize,
    d_l: f32,
    d_r: f32,
    t_l: f32,
    t_r: f32,
    glide: f32,
    fb: f32,
    dry: f32,
    wet: f32,
    tone: Tone,
    time_idx: usize,
    spread: f32,
    bpm: f32,
}

impl Delay {
    pub fn new(sr: f32) -> Self {
        let mut d = Self {
            on: false,
            kind: 0,
            sr,
            buf_l: vec![0.0; DELAY_FRAMES],
            buf_r: vec![0.0; DELAY_FRAMES],
            w: 0,
            d_l: 1.0,
            d_r: 1.0,
            t_l: 1.0,
            t_r: 1.0,
            glide: 0.0,
            fb: 0.0,
            dry: 1.0,
            wet: 0.0,
            tone: Tone::default(),
            time_idx: 12,
            spread: 64.0,
            bpm: 120.0,
        };
        d.set_sample_rate(sr);
        d
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.glide = 1.0 - 0.5f32.powf(1.0 / (0.15 * sr));
        self.retime();
        self.d_l = self.t_l;
        self.d_r = self.t_r;
    }

    pub fn clear(&mut self) {
        self.buf_l.iter_mut().for_each(|v| *v = 0.0);
        self.buf_r.iter_mut().for_each(|v| *v = 0.0);
    }

    pub fn set_time(&mut self, idx: i32, spread: f32, bpm: f32) {
        let idx = idx.clamp(0, 19) as usize;
        if idx != self.time_idx || spread != self.spread || bpm != self.bpm {
            self.time_idx = idx;
            self.spread = spread;
            self.bpm = bpm.clamp(20.0, 999.0);
            self.retime();
        }
    }

    fn retime(&mut self) {
        let sec = DELAY_TICKS[self.time_idx] as f32 * 60.0 / (self.bpm * 24.0);
        let d = (self.sr * sec / 4.0).floor() * 4.0;
        let v = self.spread - 64.0;
        let s = if v == 0.0 { 0.0 } else { v.signum() * 1000f32.powf((v.abs() / 63.0).min(1.0)) * 1e-4 };
        let (mut dl, mut dr) = (d, d);
        if s >= 0.0 {
            dl += s * self.sr;
        } else {
            dr += -s * self.sr;
        }
        let hi = (DELAY_FRAMES / 2 - 256) as f32;
        let lo = (self.sr * 1e-4).max(1.0);
        self.t_l = dl.clamp(lo, hi);
        self.t_r = dr.clamp(lo, hi);
    }

    pub fn set_feedback(&mut self, v: f32) {
        self.fb = x01(v).min(0.999);
    }

    pub fn set_dry_wet(&mut self, v: f32) {
        if v <= 64.0 {
            self.dry = 1.0;
            self.wet = (v.max(1.0) - 1.0) / 63.0;
        } else {
            self.wet = 1.0;
            self.dry = (127.0 - v) / 63.0;
        }
    }

    pub fn set_tone(&mut self, v: f32) {
        let sr = self.sr;
        if (v - 64.0).abs() < 0.5 {
            self.tone.set_off();
        } else if v < 64.0 {
            self.tone.set_lp(sr, 200.0 * 110f32.powf(v / 63.0));
        } else {
            self.tone.set_hp(sr, 10.0 * 800f32.powf(((v - 65.0) / 62.0).max(0.0)));
        }
    }

    #[inline]
    fn read(buf: &[f32], w: usize, d: f32) -> f32 {
        let mask = buf.len() - 1;
        let di = d as usize;
        let f = d - di as f32;
        let a = buf[w.wrapping_sub(di) & mask];
        let b = buf[w.wrapping_sub(di + 1) & mask];
        a + (b - a) * f
    }

    /// Returns `(dry_l, dry_r, wet_l, wet_r)`.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32, f32, f32) {
        if !self.on {
            return (l, r, 0.0, 0.0);
        }
        self.d_l += (self.t_l - self.d_l) * self.glide;
        self.d_r += (self.t_r - self.d_r) * self.glide;
        let w = self.w;
        let mask = DELAY_FRAMES - 1;
        let (wl, wr) = if self.kind == 2 {
            // Ping-pong: two mono lines in series (A in buf_l, B in buf_r).
            let a = Self::read(&self.buf_l, w, self.d_l);
            let b = Self::read(&self.buf_r, w, self.d_r);
            let (a, b) = (self.tone.tick(0, a), self.tone.tick(1, b));
            self.buf_l[w & mask] = 0.5 * (l + r) + self.fb * b;
            self.buf_r[w & mask] = a;
            (a, b)
        } else {
            let a = Self::read(&self.buf_l, w, self.d_l);
            let b = Self::read(&self.buf_r, w, self.d_r);
            let (a, b) = (self.tone.tick(0, a), self.tone.tick(1, b));
            if self.kind == 0 {
                self.buf_l[w & mask] = l + self.fb * a;
                self.buf_r[w & mask] = r + self.fb * b;
            } else {
                self.buf_l[w & mask] = r + self.fb * b;
                self.buf_r[w & mask] = l + self.fb * a;
            }
            (a, b)
        };
        self.w = (w + 1) & mask;
        (l * self.dry, r * self.dry, wl * self.wet, wr * self.wet)
    }
}

// ------------------------------------------------------------------------------------------
// Chorus / flanger
// ------------------------------------------------------------------------------------------

pub const CHORUS_TYPE_NAMES: [&str; 4] = ["mono", "stereo", "stereo", "quad"];
const CHORUS_FRAMES: usize = 8192;

#[derive(Clone, Debug)]
pub struct Chorus {
    pub on: bool,
    pub kind: u8,
    sr: f32,
    buf: [Vec<f32>; 2],
    w: usize,
    d: f32,
    m: f32,
    s: f32,
    c: f32,
    lw: f32,
    fb: f32,
    level: f32,
    params: [f32; 3],
}

impl Chorus {
    pub fn new(sr: f32) -> Self {
        let mut c = Self {
            on: false,
            kind: 2,
            sr,
            buf: [vec![0.0; CHORUS_FRAMES], vec![0.0; CHORUS_FRAMES]],
            w: 0,
            d: 100.0,
            m: 0.0,
            s: 0.0,
            c: 1.0,
            lw: 0.0,
            fb: 0.0,
            level: 0.5,
            params: [64.0; 3],
        };
        c.update();
        c
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.update();
    }

    pub fn clear(&mut self) {
        for b in &mut self.buf {
            b.iter_mut().for_each(|v| *v = 0.0);
        }
    }

    pub fn set_kind(&mut self, v: i32) {
        self.kind = match v {
            1 => 1,
            4 => 4,
            _ => 2,
        };
    }

    pub fn set(&mut self, time: f32, depth: f32, rate: f32, fb: f32, level: f32) {
        self.fb = (fb - 64.0) / 64.0 * 0.99;
        self.level = x01(level);
        if [time, depth, rate] != self.params {
            self.params = [time, depth, rate];
            self.update();
        }
    }

    fn update(&mut self) {
        let [time, depth, rate] = self.params;
        let x = x01(time);
        let ms = if x < 0.5 { 0.05 * 300f32.powf((2.0 * x).powf(0.6)) } else { 30.0 * x };
        self.d = (self.sr * ms * 0.001).clamp(2.0, (CHORUS_FRAMES / 2) as f32);
        let dep = (100f32.powf(x01(depth)) - 1.0) / 99.0;
        let hz = if rate <= 0.0 { 0.01 } else { 0.02 * 20000f32.powf(x01(rate)) }.min(self.sr * 0.25);
        let lim = ((self.sr / hz) / (core::f32::consts::TAU * self.d * dep).max(1e-6)).min(1.0);
        self.m = (self.d * dep * lim).min(self.d - 1.5);
        self.lw = 2.0 * (core::f32::consts::PI * hz / self.sr).sin();
    }

    #[inline]
    fn tap(&self, ch: usize, d: f32) -> f32 {
        let mask = CHORUS_FRAMES - 1;
        let d = d.clamp(1.0, (CHORUS_FRAMES - 2) as f32);
        let di = d as usize;
        let f = d - di as f32;
        let b = &self.buf[ch];
        let a = b[self.w.wrapping_sub(di) & mask];
        let c = b[self.w.wrapping_sub(di + 1) & mask];
        a + (c - a) * f
    }

    /// `wet_*` is the delay's wet bus, which the chorus line carries when on.
    #[inline]
    pub fn tick(&mut self, l: f32, r: f32, wet_l: f32, wet_r: f32) -> (f32, f32) {
        if !self.on {
            return (l + wet_l, r + wet_r);
        }
        self.s -= self.lw * self.c;
        self.c += self.lw * self.s;
        let mask = CHORUS_FRAMES - 1;
        let (d, m, s, c) = (self.d, self.m, self.s, self.c);
        let (yl, yr, fl, fr) = match self.kind {
            1 => {
                let a = self.tap(0, d + m * s);
                let b = self.tap(1, d + m * s);
                (a, b, a, b)
            }
            4 => {
                let p1 = self.tap(0, d + m * s);
                let p2 = self.tap(0, d - m * s);
                let q1 = self.tap(1, d + m * c);
                let q2 = self.tap(1, d - m * c);
                (0.9 * p1 + 0.1 * p2 + 0.7 * q1 + 0.3 * q2, 0.1 * p1 + 0.9 * p2 + 0.3 * q1 + 0.7 * q2, p1, q1)
            }
            _ => {
                let a = self.tap(0, d + m * s);
                let b = self.tap(1, d + m * c);
                (a, b, a, b)
            }
        };
        let w = self.w & mask;
        self.buf[0][w] = wet_l + self.level * l + self.fb * fl;
        self.buf[1][w] = wet_r + self.level * r + self.fb * fr;
        self.w = (self.w + 1) & mask;
        (l + yl, r + yr)
    }
}

// ------------------------------------------------------------------------------------------
// Whole chain
// ------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct MasterFx {
    pub effector: Effector,
    eq: [Biquad; 2],
    eq_on: bool,
    tone: Tone,
    gain_l: f32,
    gain_r: f32,
    pub delay: Delay,
    pub chorus: Chorus,
    sr: f32,
    eq_params: [f32; 3],
}

impl MasterFx {
    pub fn new(sr: f32) -> Self {
        Self {
            effector: Effector::new(sr),
            eq: [Biquad::default(); 2],
            eq_on: false,
            tone: Tone::default(),
            gain_l: 1.0,
            gain_r: 1.0,
            delay: Delay::new(sr),
            chorus: Chorus::new(sr),
            sr,
            eq_params: [64.0, 64.0, 64.0],
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
        self.effector.set_sample_rate(sr);
        self.delay.set_sample_rate(sr);
        self.chorus.set_sample_rate(sr);
        let p = self.eq_params;
        self.eq_params = [-1.0; 3];
        self.set_eq(p[0], p[1], p[2]);
    }

    pub fn reset(&mut self) {
        self.delay.clear();
        self.chorus.clear();
        for e in &mut self.eq {
            e.reset();
        }
    }

    /// sy1 61 freq, 62 level, 63 Q.
    pub fn set_eq(&mut self, freq: f32, level: f32, q: f32) {
        if [freq, level, q] == self.eq_params {
            return;
        }
        self.eq_params = [freq, level, q];
        let gain_db = (level - 64.0) / 127.0 * 50.0;
        self.eq_on = gain_db.abs() > 0.01;
        let f = 50.0 * 320f32.powf(x01(freq));
        let q = 10f32.powf(x01(q));
        for e in &mut self.eq {
            e.set_peaking(self.sr, f.min(self.sr * 0.45), q, gain_db);
        }
    }

    /// sy1 60 tone: <=64 low-pass 200 Hz..22 kHz, >64 high-pass 10 Hz..8 kHz.
    pub fn set_tone(&mut self, v: f32) {
        if v <= 64.0 {
            self.tone.set_lp(self.sr, 200.0 * 110f32.powf(v / 64.0));
        } else {
            self.tone.set_hp(self.sr, 10.0 * 800f32.powf((v - 64.0) / 64.0));
        }
    }

    /// sy1 90 pan, linear law without centre boost.
    pub fn set_pan(&mut self, v: f32) {
        let p = if v <= 64.0 { (v - 64.0) / 64.0 } else { (v - 64.0) / 63.0 };
        self.gain_l = if p > 0.0 { 1.0 - p } else { 1.0 };
        self.gain_r = if p < 0.0 { 1.0 + p } else { 1.0 };
    }

    #[inline]
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let (mut l, mut r) = self.effector.tick(l, r);
        if self.eq_on {
            l = self.eq[0].tick(l);
            r = self.eq[1].tick(r);
        }
        l = self.tone.tick(0, l) * self.gain_l;
        r = self.tone.tick(1, r) * self.gain_r;
        let (dl, dr, wl, wr) = self.delay.tick(l, r);
        self.chorus.tick(dl, dr, wl, wr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_finite_for_all_effects() {
        let sr = 48_000.0;
        for t in 0..10 {
            let mut fx = MasterFx::new(sr);
            fx.effector.on = true;
            fx.effector.kind = EffectType::from_index(t);
            fx.effector.set(100.0, 90.0, 100.0);
            fx.set_eq(64.0, 120.0, 90.0);
            fx.set_tone(30.0);
            fx.delay.on = true;
            fx.delay.kind = (t % 3) as u8;
            fx.delay.set_time(8, 80.0, 120.0);
            fx.delay.set_feedback(120.0);
            fx.delay.set_dry_wet(64.0);
            fx.delay.set_tone(40.0);
            fx.chorus.on = true;
            fx.chorus.set_kind(1 + (t % 4));
            fx.chorus.set(60.0, 64.0, 40.0, 100.0, 64.0);
            for i in 0..48_000 {
                let x = if i < 4800 { ((i as f32) * 0.05).sin() * 0.8 } else { 0.0 };
                let (l, r) = fx.tick(x, x);
                assert!(l.is_finite() && r.is_finite() && l.abs() < 500.0 && r.abs() < 500.0, "type {t} i {i} {l} {r}");
            }
        }
    }

    #[test]
    fn delay_time_matches_tempo() {
        let sr = 48_000.0;
        let mut d = Delay::new(sr);
        d.on = true;
        d.set_time(12, 64.0, 120.0); // 1/4 at 120 bpm = 0.5 s
        d.set_feedback(0.0);
        d.set_dry_wet(127.0);
        d.set_sample_rate(sr);
        let mut hit = None;
        for i in 0..48_000 {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let (_, _, wl, _) = d.tick(x, x);
            if wl.abs() > 0.5 && hit.is_none() {
                hit = Some(i);
            }
        }
        assert_eq!(hit, Some(24_000));
    }
}
