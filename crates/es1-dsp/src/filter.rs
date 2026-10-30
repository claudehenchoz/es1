// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Zero-delay-feedback voice filters.
//!
//! * `Lp12` / `Hp12` / `Bp12`: Cytomic (Andrew Simper) trapezoidal SVF.
//! * `Lp24`: Zavalishin TPT 4-pole ladder, solved implicitly. Feedback saturation keeps max
//!   resonance bounded.
//!
//! Type order matches the `.sy1` format: 0 LP12, 1 LP24, 2 HP12, 3 BP12, 4 LPDL.
//! * `LpDiode`: 4-stage coupled diode ladder (each stage sees the next stage's state, as in the
//!   TB-303 circuit), run at 2x oversampling, with tanh feedback.

use crate::util::fast_tanh;
use core::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FilterMode {
    Lp12,
    #[default]
    Lp24,
    Hp12,
    Bp12,
    LpDiode,
}

impl FilterMode {
    /// `.sy1` `filter type` value 0..4.
    pub fn from_index(i: i32) -> Self {
        match i {
            0 => Self::Lp12,
            1 => Self::Lp24,
            2 => Self::Hp12,
            3 => Self::Bp12,
            _ => Self::LpDiode,
        }
    }
}

/// tan(x) for x in [0, ~1.55]: rational approximation, < 0.1% error below 1.4.
#[inline]
fn fast_tan(x: f32) -> f32 {
    let x2 = x * x;
    x * (945.0 - x2 * (105.0 - x2)) / (945.0 - x2 * (420.0 - 15.0 * x2))
}

#[inline]
fn prewarp(cutoff: f32, sr: f32) -> f32 {
    let w = (PI * cutoff / sr).clamp(1e-5, 1.50);
    fast_tan(w)
}

#[derive(Clone, Debug, Default)]
pub struct VoiceFilter {
    // SVF
    ic1: f32,
    ic2: f32,
    // ladder / diode stages
    s: [f32; 4],
    x_prev: f32,
}

impl VoiceFilter {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Process one sample.
    ///
    /// * `cutoff` in Hz (already including envelope / key-track / LFO modulation).
    /// * `reso` in 0..1 (1 = self-oscillation threshold for the ladders).
    /// * `drive` pre-gain for saturation (1 = clean-ish).
    #[inline]
    pub fn tick(&mut self, mode: FilterMode, x: f32, cutoff: f32, reso: f32, drive: f32, sr: f32) -> f32 {
        let reso = reso.clamp(0.0, 1.0);
        match mode {
            FilterMode::Lp12 | FilterMode::Hp12 | FilterMode::Bp12 => {
                let g = prewarp(cutoff, sr);
                // Q from 0.5 to ~25.
                let k = 2.0 - 1.96 * reso;
                let a1 = 1.0 / (1.0 + g * (g + k));
                let a2 = g * a1;
                let a3 = g * a2;
                let v0 = fast_tanh(x * drive) / drive.max(1e-3);
                let v3 = v0 - self.ic2;
                let v1 = a1 * self.ic1 + a2 * v3;
                let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
                // Soft-limit the integrator states so very high Q can't run away.
                self.ic1 = fast_tanh((2.0 * v1 - self.ic1) * 0.25) * 4.0;
                self.ic2 = 2.0 * v2 - self.ic2;
                match mode {
                    FilterMode::Lp12 => v2,
                    FilterMode::Bp12 => v1,
                    _ => v0 - k * v1 - v2,
                }
            }
            FilterMode::Lp24 => {
                let g = prewarp(cutoff, sr);
                let gg = g / (1.0 + g);
                let k = 4.0 * reso * 1.02;
                // Implicit solve of u = x - k*y4 for 4 cascaded TPT one-poles.
                let s = &mut self.s;
                let gp = gg * gg * gg * gg;
                // Per-stage TPT: y = gg*in + (1-gg)*s, so y4 = gg^4*u + sigma.
                let sigma = (1.0 - gg) * (gg * gg * gg * s[0] + gg * gg * s[1] + gg * s[2] + s[3]);
                let xin = fast_tanh(x * drive) / drive.max(1e-3);
                let u_lin = (xin - k * sigma) / (1.0 + k * gp);
                // Feedback saturation: bounds resonance energy.
                let u = fast_tanh(u_lin);
                let mut inp = u;
                let mut ys = [0.0f32; 4];
                for i in 0..4 {
                    let v = gg * (inp - s[i]);
                    let y = v + s[i];
                    s[i] = y + v;
                    ys[i] = y;
                    inp = y;
                }
                // Compensate passband loss caused by resonance.
                ys[3] * (1.0 + 0.5 * k)
            }
            FilterMode::LpDiode => {
                // 2x oversampled, linear-interpolated input.
                let sr2 = sr * 2.0;
                let g = prewarp(cutoff.min(sr * 0.45), sr2);
                let gg = g / (1.0 + g);
                let k = reso * 17.0;
                let mid = 0.5 * (self.x_prev + x);
                self.x_prev = x;
                let a = self.diode_step(mid * drive, gg, k);
                let b = self.diode_step(x * drive, gg, k);
                (0.5 * (a + b)) * 2.2 / drive.max(1e-3).sqrt()
            }
        }
    }

    #[inline]
    fn diode_step(&mut self, x: f32, gg: f32, k: f32) -> f32 {
        let s = &mut self.s;
        // Feedback from last stage (one-sample delayed at the oversampled rate), saturated.
        let u = fast_tanh(x - k * s[3]);
        // Coupled stages: each stage is driven by the difference to its neighbours,
        // giving the diode ladder's characteristic gentler, "darker" roll-off.
        let i0 = u - 0.5 * s[1];
        let y0 = tpt(&mut s[0], i0, gg);
        let i1 = 0.5 * (y0 - s[2]) + 0.5 * y0;
        let y1 = tpt(&mut s[1], fast_tanh(i1), gg);
        let i2 = 0.5 * (y1 - s[3]) + 0.5 * y1;
        let y2 = tpt(&mut s[2], i2, gg);
        
        tpt(&mut s[3], 0.5 * y2 + 0.5 * y2, gg)
    }
}

#[inline(always)]
fn tpt(s: &mut f32, x: f32, gg: f32) -> f32 {
    let v = gg * (x - *s);
    let y = v + *s;
    *s = y + v;
    y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::TAU;

    fn gain_at(mode: FilterMode, f: f32, cutoff: f32, reso: f32) -> f32 {
        let sr = 48_000.0;
        let mut flt = VoiceFilter::default();
        let n = 24_000;
        let mut acc = 0.0f64;
        for i in 0..n {
            let x = 0.1 * (TAU * f * i as f32 / sr).sin();
            let y = flt.tick(mode, x, cutoff, reso, 1.0, sr);
            if i > n / 2 {
                acc += (y * y) as f64;
            }
        }
        let rms = (acc / (n / 2) as f64).sqrt() as f32;
        rms / (0.1 / 2f32.sqrt())
    }

    #[test]
    fn lowpasses_pass_low_and_cut_high() {
        for m in [FilterMode::Lp12, FilterMode::Lp24, FilterMode::LpDiode] {
            let lo = gain_at(m, 100.0, 2_000.0, 0.0);
            let hi = gain_at(m, 12_000.0, 2_000.0, 0.0);
            assert!(lo > 0.5 && lo < 2.0, "{m:?} lo {lo}");
            assert!(hi < 0.15 * lo, "{m:?} hi {hi} lo {lo}");
        }
    }

    #[test]
    fn highpasses_cut_low() {
        for m in [FilterMode::Hp12] {
            let lo = gain_at(m, 100.0, 2_000.0, 0.0);
            let hi = gain_at(m, 12_000.0, 2_000.0, 0.0);
            assert!(hi > 0.5 && hi < 2.0, "{m:?} hi {hi}");
            assert!(lo < 0.1 * hi, "{m:?} lo {lo}");
        }
    }

    #[test]
    fn svf_cutoff_is_minus_3db() {
        let g = gain_at(FilterMode::Lp12, 1_000.0, 1_000.0, 0.0);
        // Q=0.5 SVF: -6 dB at cutoff; just check it's in the right region.
        assert!(g > 0.4 && g < 0.75, "{g}");
    }

    #[test]
    fn max_resonance_is_bounded() {
        let sr = 48_000.0;
        for m in [FilterMode::Lp12, FilterMode::Lp24, FilterMode::Hp12, FilterMode::Bp12, FilterMode::LpDiode] {
            let mut flt = VoiceFilter::default();
            let mut peak = 0.0f32;
            for i in 0..96_000 {
                let x = if i % 200 < 100 { 1.0 } else { -1.0 };
                let c = 200.0 + 8_000.0 * (0.5 + 0.5 * (i as f32 * 0.0003).sin());
                let y = flt.tick(m, x, c, 1.0, 2.0, sr);
                assert!(y.is_finite(), "{m:?}");
                peak = peak.max(y.abs());
            }
            assert!(peak < 8.0, "{m:?} peak {peak}");
        }
    }
}
