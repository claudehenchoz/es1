// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Band-limited oscillators.
//!
//! Every waveform is generated naively from a phase accumulator and then corrected with
//! two-sided polynomial BLEP (steps) / BLAMP (slope changes) residuals. To apply the half of a
//! residual that falls *before* a discontinuity, each oscillator runs one sample behind: the
//! value returned by [`Osc::tick`] is the previous sample, already fully corrected. All
//! oscillators share this one-sample latency, so relative phase between them is preserved.
//!
//! Frequency modulation is linear FM applied directly to the phase increment (through-zero
//! capable), so discontinuity timing stays exact under modulation. Hard sync resets the phase
//! at a fractional sample position supplied by the master oscillator.

use crate::util::{Rng, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Wave {
    #[default]
    Saw,
    Square,
    Triangle,
    Sine,
    /// Pulse with variable width.
    Pulse,
    Noise,
}

/// Fraction of a sample at which a master oscillator wrapped during the last tick:
/// `Some(tau)` means the wrap happened `tau` samples before the current sample.
pub type SyncEvent = Option<f32>;

#[derive(Clone, Debug)]
pub struct Osc {
    pub phase: f32,
    /// Corrected value of the sample being held back (returned on the next tick).
    held: f32,
    rng: Rng,
}

impl Default for Osc {
    fn default() -> Self {
        Self::new(0x1234_5678)
    }
}

/// Residual of a unit step for the sample *before* the step, `tau` = time from step to the
/// following sample in samples (0..1).
#[inline(always)]
fn blep_before(tau: f32) -> f32 {
    0.5 * tau * tau
}

/// Residual of a unit step for the sample *after* the step.
#[inline(always)]
fn blep_after(tau: f32) -> f32 {
    let u = 1.0 - tau;
    -0.5 * u * u
}

/// Residual of a unit slope change (per sample) for the sample before the corner.
#[inline(always)]
fn blamp_before(tau: f32) -> f32 {
    tau * tau * tau * (1.0 / 6.0)
}

#[inline(always)]
fn blamp_after(tau: f32) -> f32 {
    let u = 1.0 - tau;
    u * u * u * (1.0 / 6.0)
}

#[inline(always)]
fn naive(wave: Wave, p: f32, pw: f32) -> f32 {
    match wave {
        // Falling ramp, +1 at phase 0.
        Wave::Saw => 1.0 - 2.0 * p,
        Wave::Square => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Pulse => {
            if p < pw {
                1.0
            } else {
                -1.0
            }
        }
        Wave::Triangle => {
            if p < 0.5 {
                -1.0 + 4.0 * p
            } else {
                3.0 - 4.0 * p
            }
        }
        Wave::Sine => (p * TAU).sin(),
        Wave::Noise => 0.0,
    }
}

impl Osc {
    pub fn new(seed: u32) -> Self {
        Self { phase: 0.0, held: 0.0, rng: Rng::new(seed) }
    }

    pub fn reset(&mut self, phase: f32) {
        self.phase = phase.rem_euclid(1.0);
        self.held = 0.0;
    }

    /// Advance by `dt` cycles/sample (may be negative for through-zero FM).
    ///
    /// `pw` is the pulse width (0..1) for [`Wave::Pulse`]. `sync` resets the phase at the given
    /// fractional position. Returns `(sample, wrapped)` where `wrapped` reports this
    /// oscillator's own forward wrap so it can act as a sync master.
    #[inline]
    pub fn tick(&mut self, wave: Wave, dt: f32, pw: f32, sync: SyncEvent) -> (f32, SyncEvent) {
        if wave == Wave::Noise {
            let out = self.held;
            // Binary ±1 noise (the level of a 1-bit LFSR).
            self.held = if self.rng.next_u32() & 0x8000_0000 != 0 { 1.0 } else { -1.0 };
            return (out, None);
        }
        if wave == Wave::Sine {
            let out = self.held;
            let p0 = self.phase;
            let mut p = p0 + dt;
            let mut wrapped = None;
            if p >= 1.0 {
                p -= 1.0;
                wrapped = Some((p / dt).clamp(0.0, 1.0));
            } else if p < 0.0 {
                p += 1.0;
            }
            if let Some(tau) = sync {
                p = (tau * dt.abs()).rem_euclid(1.0);
            }
            self.phase = p;
            self.held = (p * TAU).sin();
            return (out, wrapped);
        }

        let pw = pw.clamp(0.02, 0.98);
        let adt = dt.abs().max(1e-9);
        let p0 = self.phase;
        let mut p = p0 + dt;
        let mut prev_corr = 0.0f32;
        let mut cur_corr = 0.0f32;
        let mut wrapped = None;

        // Edge tables per waveform for forward traversal: (phase, step height, slope change in
        // units/cycle). Phases lie in (0, 1].
        let (edges, n): ([(f32, f32, f32); 2], usize) = match wave {
            Wave::Saw => ([(1.0, 2.0, 0.0), (0.0, 0.0, 0.0)], 1),
            Wave::Square => ([(0.5, -2.0, 0.0), (1.0, 2.0, 0.0)], 2),
            Wave::Pulse => ([(pw, -2.0, 0.0), (1.0, 2.0, 0.0)], 2),
            Wave::Triangle => ([(0.5, 0.0, -8.0), (1.0, 0.0, 8.0)], 2),
            _ => ([(0.0, 0.0, 0.0); 2], 0),
        };

        // Visit every edge crossed between p0 and the unwrapped p, in either direction. Each
        // edge also exists one cycle later (forward) or earlier (backward).
        let (sign, shift) = if dt >= 0.0 { (1.0f32, 1.0f32) } else { (-1.0, -1.0) };
        for &(at, h, s) in &edges[..n] {
            for pos in [at, at + shift] {
                let crossed = if dt >= 0.0 { p0 < pos && pos <= p } else { p < pos && pos <= p0 };
                if crossed {
                    let tau = ((p - pos) * sign / adt).clamp(0.0, 1.0);
                    let (h, s) = (h * sign, s * sign);
                    prev_corr += h * blep_before(tau) + s * adt * blamp_before(tau);
                    cur_corr += h * blep_after(tau) + s * adt * blamp_after(tau);
                }
            }
        }
        if p >= 1.0 {
            p -= 1.0;
            wrapped = Some((p / adt).clamp(0.0, 1.0));
        } else if p < 0.0 {
            p += 1.0;
        }

        // Hard sync: jump phase to where it would be `tau` samples after a reset.
        if let Some(tau) = sync {
            let before = naive(wave, p, pw);
            let np = (tau * adt).min(0.999_999);
            let after = naive(wave, np, pw);
            let h = after - before;
            prev_corr += h * blep_before(tau);
            cur_corr += h * blep_after(tau);
            p = np;
        }

        self.phase = p;
        let out = self.held + prev_corr;
        self.held = naive(wave, p, pw) + cur_corr;
        (out, wrapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goertzel magnitude at a given frequency bin.
    fn tone_mag(x: &[f32], freq: f32, sr: f32) -> f32 {
        let w = TAU * freq / sr;
        let (mut s1, mut s2) = (0.0f64, 0.0f64);
        let c = 2.0 * (w as f64).cos();
        for &v in x {
            let s0 = v as f64 + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        ((s1 * s1 + s2 * s2 - c * s1 * s2).sqrt() / x.len() as f64) as f32
    }

    fn render(wave: Wave, f: f32, sr: f32, n: usize) -> Vec<f32> {
        let mut o = Osc::default();
        (0..n).map(|_| o.tick(wave, f / sr, 0.5, None).0).collect()
    }

    #[test]
    fn saw_fundamental_and_bounds() {
        let sr = 48_000.0;
        let x = render(Wave::Saw, 440.0, sr, 48_000);
        assert!(x.iter().all(|v| v.abs() < 1.3));
        // Saw harmonic 1 amplitude ≈ 2/π, Goertzel magnitude is A/2.
        let m = tone_mag(&x, 440.0, sr);
        assert!((m - (1.0 / core::f32::consts::PI)).abs() < 0.01, "{m}");
    }

    #[test]
    fn saw_aliasing_reduced() {
        // A high note: aliased partials of a naive saw fall at non-harmonic frequencies.
        let sr = 44_100.0;
        let f = 3_520.0;
        let x = render(Wave::Saw, f, sr, 44_100);
        // 13th harmonic (45.76k) aliases to 44.1k - 45.76k -> |.|=1660 Hz.
        let alias = tone_mag(&x, (13.0 * f - sr).abs(), sr);
        let fund = tone_mag(&x, f, sr);
        assert!(alias / fund < 0.02, "alias ratio {}", alias / fund);
    }

    #[test]
    fn triangle_and_pulse_are_finite() {
        for w in [Wave::Triangle, Wave::Pulse, Wave::Square, Wave::Sine, Wave::Noise] {
            let x = render(w, 1_000.0, 48_000.0, 4_800);
            assert!(x.iter().all(|v| v.is_finite() && v.abs() < 1.5), "{w:?}");
        }
    }

    #[test]
    fn negative_dt_is_finite() {
        let mut o = Osc::default();
        for i in 0..10_000 {
            let dt = ((i as f32) * 0.01).sin() * 0.05;
            let (v, _) = o.tick(Wave::Saw, dt, 0.5, None);
            assert!(v.is_finite() && v.abs() < 2.0);
        }
    }

    #[test]
    fn sync_reports_master_wraps() {
        let mut m = Osc::default();
        let mut s = Osc::default();
        let mut wraps = 0;
        for _ in 0..48_000 {
            let (_, w) = m.tick(Wave::Saw, 100.0 / 48_000.0, 0.5, None);
            if w.is_some() {
                wraps += 1;
            }
            let (v, _) = s.tick(Wave::Saw, 270.0 / 48_000.0, 0.5, w);
            assert!(v.is_finite() && v.abs() < 2.5);
        }
        assert!((99..=101).contains(&wraps));
    }
}
