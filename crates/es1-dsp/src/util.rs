//! Small numeric helpers shared by every DSP module.

use core::f32::consts::PI;

pub const TAU: f32 = 2.0 * PI;

/// Semitones to frequency ratio.
#[inline]
pub fn semi_to_ratio(semi: f32) -> f32 {
    fast_exp2(semi * (1.0 / 12.0))
}

/// MIDI note (fractional) to Hz, A4 = 440 Hz.
#[inline]
pub fn note_to_hz(note: f32) -> f32 {
    440.0 * fast_exp2((note - 69.0) * (1.0 / 12.0))
}

/// 2^x, accurate to ~1e-6 relative over the audio range. Uses the integer/fraction split
/// plus a 5th-order least-squares polynomial; much faster than `powf` on the voice hot path.
#[inline]
pub fn fast_exp2(x: f32) -> f32 {
    let x = x.clamp(-126.0, 126.0);
    let xi = x.floor();
    let f = x - xi;
    let p = 1.0
        + f * (0.693_151_6
            + f * (0.240_164_4 + f * (0.055_793_73 + f * (0.009_031_18 + f * 0.001_858_75))));
    f32::from_bits(((xi as i32 + 127) as u32) << 23) * p
}

/// Rational tanh approximation (Padé 7/6, clamped), smooth and monotone for saturation.
#[inline]
pub fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-4.97, 4.97);
    let x2 = x * x;
    let n = x * (135_135.0 + x2 * (17_325.0 + x2 * (378.0 + x2)));
    let d = 135_135.0 + x2 * (62_370.0 + x2 * (3_150.0 + x2 * 28.0));
    n / d
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// One-pole parameter smoother (exponential approach), used for zipper-free control changes.
#[derive(Clone, Copy, Debug)]
pub struct Smoother {
    pub value: f32,
    target: f32,
    coef: f32,
}

impl Smoother {
    pub fn new(value: f32) -> Self {
        Self { value, target: value, coef: 1.0 }
    }

    /// `time_s` is the ~63% settle time.
    pub fn set_time(&mut self, sample_rate: f32, time_s: f32) {
        self.coef = 1.0 - (-1.0 / (time_s * sample_rate).max(1.0)).exp();
    }

    #[inline]
    pub fn set(&mut self, target: f32) {
        self.target = target;
    }

    #[inline]
    pub fn reset(&mut self, value: f32) {
        self.value = value;
        self.target = value;
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        self.value += (self.target - self.value) * self.coef;
        self.value
    }
}

/// xorshift32 white-noise / random source. Deterministic and allocation-free.
#[derive(Clone, Copy, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [-1, 1).
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
    }

    /// Uniform in [0, 1).
    #[inline]
    pub fn unipolar(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }
}

/// Enable flush-to-zero / denormals-are-zero for the current thread (x86/x86_64 only).
/// Returns the previous MXCSR so callers can restore it.
#[inline]
#[allow(deprecated)]
pub fn enable_ftz() -> u32 {
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    {
        #[cfg(target_arch = "x86")]
        use core::arch::x86::{_mm_getcsr, _mm_setcsr};
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::{_mm_getcsr, _mm_setcsr};
        #[allow(deprecated)]
        unsafe {
            let old = _mm_getcsr();
            _mm_setcsr(old | 0x8040);
            old
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
    {
        0
    }
}

#[inline]
#[allow(deprecated)]
pub fn restore_ftz(old: u32) {
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    {
        #[cfg(target_arch = "x86")]
        use core::arch::x86::_mm_setcsr;
        #[cfg(target_arch = "x86_64")]
        use core::arch::x86_64::_mm_setcsr;
        #[allow(deprecated)]
        unsafe {
            _mm_setcsr(old)
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "x86")))]
    let _ = old;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp2_accuracy() {
        let mut x = -20.0f32;
        while x < 20.0 {
            let e = 2f32.powf(x);
            let a = fast_exp2(x);
            assert!(((a - e) / e).abs() < 2e-6, "x={x} a={a} e={e}");
            x += 0.0137;
        }
    }

    #[test]
    fn tanh_accuracy() {
        let mut x = -6.0f32;
        while x < 6.0 {
            assert!((fast_tanh(x) - x.tanh()).abs() < 2e-3, "x={x}");
            x += 0.01;
        }
    }
}
