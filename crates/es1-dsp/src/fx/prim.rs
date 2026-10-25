//! Effect building blocks: biquads, one-poles, fractional delay lines.

use core::f32::consts::PI;

/// Transposed direct form II biquad (RBJ cookbook coefficients).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }
}

impl Biquad {
    fn set(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        let ia = 1.0 / a0;
        self.b0 = b0 * ia;
        self.b1 = b1 * ia;
        self.b2 = b2 * ia;
        self.a1 = a1 * ia;
        self.a2 = a2 * ia;
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    pub fn set_peaking(&mut self, sr: f32, f: f32, q: f32, gain_db: f32) {
        let a = 10f32.powf(gain_db / 40.0);
        let w = 2.0 * PI * (f / sr).clamp(1e-5, 0.49);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q.max(0.05));
        self.set(1.0 + al * a, -2.0 * c, 1.0 - al * a, 1.0 + al / a, -2.0 * c, 1.0 - al / a);
    }

    pub fn set_lowpass(&mut self, sr: f32, f: f32, q: f32) {
        let w = 2.0 * PI * (f / sr).clamp(1e-5, 0.49);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q);
        let b = (1.0 - c) * 0.5;
        self.set(b, 2.0 * b, b, 1.0 + al, -2.0 * c, 1.0 - al);
    }

    pub fn set_highpass(&mut self, sr: f32, f: f32, q: f32) {
        let w = 2.0 * PI * (f / sr).clamp(1e-5, 0.49);
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q);
        let b = (1.0 + c) * 0.5;
        self.set(b, -2.0 * b, b, 1.0 + al, -2.0 * c, 1.0 - al);
    }

    pub fn set_bypass(&mut self) {
        self.set(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// One-pole low-pass (TPT form); `hp()` returns the complementary high-pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    s: f32,
    g: f32,
}

impl OnePole {
    pub fn set(&mut self, sr: f32, f: f32) {
        let w = (PI * f / sr).clamp(1e-6, 1.5);
        let g = w.tan();
        self.g = g / (1.0 + g);
    }

    pub fn reset(&mut self) {
        self.s = 0.0;
    }

    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        let v = self.g * (x - self.s);
        let y = v + self.s;
        self.s = y + v;
        y
    }

    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        x - self.lp(x)
    }
}

/// Power-of-two ring buffer with fractional (cubic Hermite) reads.
#[derive(Clone, Debug)]
pub struct DelayLine {
    buf: Vec<f32>,
    mask: usize,
    w: usize,
}

impl DelayLine {
    pub fn new(max_samples: usize) -> Self {
        let n = (max_samples + 4).next_power_of_two();
        Self { buf: vec![0.0; n], mask: n - 1, w: 0 }
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn clear(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
    }

    #[inline]
    pub fn push(&mut self, x: f32) {
        self.w = (self.w + 1) & self.mask;
        self.buf[self.w] = x;
    }

    /// Read with a delay of `d` samples, called *before* pushing the current input:
    /// `read(1.0)` returns the most recently pushed sample.
    #[inline]
    pub fn read(&self, d: f32) -> f32 {
        let d = d.clamp(1.0, (self.buf.len() - 4) as f32);
        let di = d as usize;
        let f = d - di as f32;
        let i = self.w.wrapping_sub(di - 1) & self.mask;
        if di < 2 {
            let x1 = self.buf[i.wrapping_sub(1) & self.mask];
            return self.buf[i] + (x1 - self.buf[i]) * f;
        }
        let xm1 = self.buf[(i + 1) & self.mask];
        let x0 = self.buf[i];
        let x1 = self.buf[i.wrapping_sub(1) & self.mask];
        let x2 = self.buf[i.wrapping_sub(2) & self.mask];
        // Hermite interpolation between x0 (delay di) and x1 (delay di+1).
        let c1 = 0.5 * (x1 - xm1);
        let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
        let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
        ((c3 * f + c2) * f + c1) * f + x0
    }
}

/// First-order allpass (for the phaser).
#[derive(Clone, Copy, Debug, Default)]
pub struct Allpass1 {
    z: f32,
}

impl Allpass1 {
    #[inline]
    pub fn tick(&mut self, x: f32, a: f32) -> f32 {
        let y = -a * x + self.z;
        self.z = x + a * y;
        y
    }

    pub fn reset(&mut self) {
        self.z = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delay_integer_read_is_exact() {
        let mut d = DelayLine::new(64);
        for i in 0..100 {
            d.push(i as f32);
        }
        assert_eq!(d.read(1.0), 99.0);
        assert_eq!(d.read(10.0), 90.0);
        assert!((d.read(10.5) - 89.5).abs() < 1e-4);
        assert!((d.read(1.5) - 98.5).abs() < 1e-4);
    }

    #[test]
    fn biquad_bypass_identity() {
        let mut b = Biquad::default();
        b.set_peaking(48_000.0, 1_000.0, 1.0, 0.0);
        let mut e = 0.0f32;
        for i in 0..1000 {
            let x = (i as f32 * 0.1).sin();
            e = e.max((b.tick(x) - x).abs());
        }
        assert!(e < 1e-4);
    }
}
