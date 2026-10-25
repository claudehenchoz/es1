//! Global LFOs (Synth1 `CLFO`). Both LFOs are shared by every voice, key sync resets them for
//! all voices at each note-on.
//!
//! Unlike Synth1, which holds LFO values over 32..64-sample chunks, values are computed per
//! sample here.

use crate::util::{Rng, TAU};

/// Tempo-sync periods in 24-ppq ticks (96 = one bar), Synth1 table at 0x180073cf0.
pub const SYNC_TICKS: [u32; 30] =
    [1536, 1152, 864, 768, 576, 432, 384, 288, 216, 192, 144, 96, 84, 72, 48, 42, 36, 32, 24, 21, 18, 16, 12, 9, 8, 6, 4, 3, 2, 1];

pub const WAVE_NAMES: [&str; 6] = ["saw", "tri", "square", "s&h", "smooth", "sine"];
pub const DEST_NAMES: [&str; 7] = ["osc 2", "osc 1+2", "filter", "amp", "p/w", "fm", "pan"];

/// Free-running rate in Hz for 0..127: 0.083 .. 125 Hz.
#[inline]
pub fn free_rate(v: f32) -> f32 {
    1500f32.powf((v / 127.0).clamp(0.0, 1.0)) / 12.0
}

/// Synced rate for 0..127 at `bpm`.
#[inline]
pub fn sync_rate(v: f32, bpm: f32) -> f32 {
    let i = (((v.clamp(0.0, 127.0) as u32) * 30) >> 7).min(29) as usize;
    0.4 * bpm / SYNC_TICKS[i] as f32
}

/// Human label for a synced rate value.
pub fn sync_label(v: f32) -> String {
    let i = (((v.clamp(0.0, 127.0) as u32) * 30) >> 7).min(29) as usize;
    let t = SYNC_TICKS[i];
    // 96 ticks = 1 bar (whole note).
    if t >= 96 && t.is_multiple_of(96) {
        format!("{} bar", t / 96)
    } else if t > 96 {
        format!("{:.2} bar", t as f32 / 96.0)
    } else {
        let d = 96.0 / t as f32;
        if (d - d.round()).abs() < 1e-3 {
            format!("1/{}", d.round() as u32)
        } else if (d * 1.5 - (d * 1.5).round()).abs() < 1e-3 {
            format!("1/{}.", (d * 1.5).round() as u32)
        } else if (d / 1.5 - (d / 1.5).round()).abs() < 1e-3 {
            format!("1/{}T", (d / 1.5).round() as u32)
        } else {
            format!("{t}/96")
        }
    }
}

#[derive(Clone, Debug)]
pub struct Lfo {
    pub phase: f32,
    rng: Rng,
    prev: f32,
    next: f32,
    rate: f32,
    target_rate: f32,
}

impl Lfo {
    pub fn new(seed: u32) -> Self {
        let mut rng = Rng::new(seed);
        let next = rng.bipolar();
        Self { phase: 0.0, rng, prev: 0.0, next, rate: 1.0, target_rate: 1.0 }
    }

    pub fn set_rate(&mut self, hz: f32) {
        self.target_rate = hz;
    }

    pub fn jump_rate(&mut self, hz: f32) {
        self.target_rate = hz;
        self.rate = hz;
    }

    /// Key sync: restart phase and draw a new random target.
    pub fn retrigger(&mut self) {
        self.phase = 0.0;
        self.prev = self.next;
        self.next = self.rng.bipolar();
    }

    /// Advance one sample and return a bipolar value in [-1, 1].
    #[inline]
    pub fn tick(&mut self, wave: u8, sr: f32) -> f32 {
        self.tick_n(wave, sr, 1)
    }

    /// Return the current value, then advance by `n` samples.
    #[inline]
    pub fn tick_n(&mut self, wave: u8, sr: f32, n: usize) -> f32 {
        // Synth1 slews the rate toward its target at 1 Hz/ms.
        let step = 1000.0 * n as f32 / sr;
        let d = self.target_rate - self.rate;
        self.rate += d.clamp(-step, step);
        let p = self.phase;
        let out = match wave {
            0 => 1.0 - 2.0 * p,
            1 => {
                if p < 0.5 {
                    -1.0 + 4.0 * p
                } else {
                    3.0 - 4.0 * p
                }
            }
            2 => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            3 => self.next,
            4 => self.prev + (self.next - self.prev) * p,
            _ => (p * TAU).sin(),
        };
        self.phase += self.rate * n as f32 / sr;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.prev = self.next;
            self.next = self.rng.bipolar();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates() {
        assert!((free_rate(0.0) - 1.0 / 12.0).abs() < 1e-5);
        assert!((free_rate(127.0) - 125.0).abs() < 0.01);
        // v=127 -> last index -> 1 tick: 0.4*120/1 = 48 Hz (1/96 note at 120 bpm).
        assert!((sync_rate(127.0, 120.0) - 48.0).abs() < 1e-3);
        // 96 ticks at 120bpm = 0.5 Hz (one bar every 2 s).
        assert!((0.4 * 120.0 / 96.0 - 0.5f32).abs() < 1e-6);
        assert_eq!(sync_label(0.0), "16 bar");
    }

    #[test]
    fn waves_bounded() {
        for w in 0..6 {
            let mut l = Lfo::new(3);
            l.jump_rate(5.0);
            for _ in 0..48_000 {
                let v = l.tick(w, 48_000.0);
                assert!((-1.0..=1.0).contains(&v));
            }
        }
    }
}
