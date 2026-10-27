//! Envelope generators:
//!
//! * attack is linear, retriggering from the current level;
//! * decay and release follow `target + range · r · e^(8(r−1))` with `r = 1 − t/len`, reaching the
//!   target exactly at `len` (or a linear ramp when `linear` is set, used by the FM mod env);
//! * release length is fixed regardless of the level at note-off; the amp envelope finishes at
//!   90 % of the release time (≈ −80 dB).
//!
//! The curve is evaluated per sample from a lookup table, so it is smooth at any block size.

/// Attack time in seconds for a 0..127 value: 0.5 ms .. 28 s.
#[inline]
pub fn attack_time(v: f32) -> f32 {
    0.0005 * 56_000f32.powf((v / 127.0).clamp(0.0, 1.0))
}

/// Decay / release time in seconds for a 0..127 value: 10 ms .. 60 s.
#[inline]
pub fn decay_time(v: f32) -> f32 {
    0.01 * 6_000f32.powf((v / 127.0).clamp(0.0, 1.0))
}

const CURVE_N: usize = 512;

/// `g(r) = r·e^(8(r−1))` sampled on r ∈ [0, 1].
fn curve_table() -> &'static [f32; CURVE_N + 2] {
    use std::sync::OnceLock;
    static T: OnceLock<[f32; CURVE_N + 2]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0.0f32; CURVE_N + 2];
        for (i, v) in t.iter_mut().enumerate() {
            let r = (i as f64 / CURVE_N as f64).min(1.0);
            *v = (r * (8.0 * (r - 1.0)).exp()) as f32;
        }
        t
    })
}

#[inline]
fn curve(table: &[f32; CURVE_N + 2], r: f32) -> f32 {
    let x = r.clamp(0.0, 1.0) * CURVE_N as f32;
    let i = x as usize;
    let f = x - i as f32;
    table[i] + (table[i + 1] - table[i]) * f
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Off,
    Attack,
    Decay,
    Sustain,
    Release,
}

#[derive(Clone, Copy, Debug)]
pub struct EnvParams {
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
    /// Linear decay/release instead of the exponential curve.
    pub linear: bool,
    /// Ignore note-off and never sustain (AD envelope).
    pub one_shot: bool,
    /// Finish release at 90 % of its length (amp envelope).
    pub short_release: bool,
}

impl Default for EnvParams {
    fn default() -> Self {
        Self { attack: 0.001, decay: 0.1, sustain: 1.0, release: 0.1, linear: false, one_shot: false, short_release: false }
    }
}

#[derive(Clone, Debug)]
pub struct Envelope {
    pub stage: Stage,
    pub level: f32,
    t: f32,
    start: f32,
    table: &'static [f32; CURVE_N + 2],
}

impl Default for Envelope {
    fn default() -> Self {
        Self { stage: Stage::Off, level: 0.0, t: 0.0, start: 0.0, table: curve_table() }
    }
}

impl Envelope {
    pub fn gate_on(&mut self) {
        // Retrigger from the current level: attack position derived from level.
        self.stage = Stage::Attack;
        self.start = self.level;
        self.t = 0.0;
    }

    pub fn gate_off(&mut self, p: &EnvParams) {
        if p.one_shot || matches!(self.stage, Stage::Off | Stage::Release) {
            return;
        }
        self.stage = Stage::Release;
        self.start = self.level;
        self.t = 0.0;
    }

    pub fn kill(&mut self) {
        self.stage = Stage::Off;
        self.level = 0.0;
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Off
    }

    /// Advance by `dt` seconds and return the new level (0..1).
    #[inline]
    pub fn tick(&mut self, p: &EnvParams, dt: f32) -> f32 {
        match self.stage {
            Stage::Off => {
                self.level = 0.0;
            }
            Stage::Attack => {
                // Linear from the retrigger level up to 1, at the full-scale slope.
                let a = p.attack.max(1e-5);
                self.level += dt / a;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                    self.t = 0.0;
                }
            }
            Stage::Decay => {
                let sus = if p.one_shot { 0.0 } else { p.sustain };
                self.t += dt;
                let r = 1.0 - self.t / p.decay.max(1e-5);
                if r <= 0.0 {
                    self.level = sus;
                    self.stage = if p.one_shot { Stage::Off } else { Stage::Sustain };
                } else {
                    let g = if p.linear { r } else { curve(self.table, r) };
                    self.level = sus + (1.0 - sus) * g;
                }
            }
            Stage::Sustain => {
                self.level = p.sustain;
            }
            Stage::Release => {
                self.t += dt;
                let len = p.release.max(1e-5);
                let r = 1.0 - self.t / len;
                let end = if p.short_release { 0.1 } else { 0.0 };
                if r <= end {
                    self.level = 0.0;
                    self.stage = Stage::Off;
                } else {
                    let g = if p.linear { r } else { curve(self.table, r) };
                    self.level = self.start * g;
                }
            }
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_ranges() {
        assert!((attack_time(0.0) - 0.0005).abs() < 1e-6);
        assert!((attack_time(127.0) - 28.0).abs() < 0.01);
        assert!((decay_time(127.0) - 60.0).abs() < 0.01);
    }

    #[test]
    fn adsr_shape_and_timing() {
        let sr = 48_000.0;
        let dt = 1.0 / sr;
        let p = EnvParams { attack: 0.01, decay: 0.1, sustain: 0.5, release: 0.2, ..Default::default() };
        let mut e = Envelope::default();
        e.gate_on();
        let mut n = 0;
        while e.stage == Stage::Attack {
            e.tick(&p, dt);
            n += 1;
        }
        assert!((n as f32 - 480.0).abs() <= 2.0, "attack samples {n}");
        for _ in 0..(0.1 * sr) as usize + 2 {
            e.tick(&p, dt);
        }
        assert_eq!(e.stage, Stage::Sustain);
        assert!((e.level - 0.5).abs() < 1e-6);
        e.gate_off(&p);
        let mut n = 0;
        while e.is_active() {
            e.tick(&p, dt);
            n += 1;
        }
        assert!((n as f32 - 9600.0).abs() <= 2.0, "release samples {n}");
    }

    #[test]
    fn one_shot_ignores_note_off() {
        let p = EnvParams { attack: 0.001, decay: 0.01, one_shot: true, ..Default::default() };
        let mut e = Envelope::default();
        e.gate_on();
        e.tick(&p, 0.0005);
        e.gate_off(&p);
        assert_eq!(e.stage, Stage::Attack);
        for _ in 0..1000 {
            e.tick(&p, 1.0 / 48_000.0);
        }
        assert_eq!(e.stage, Stage::Off);
    }
}
