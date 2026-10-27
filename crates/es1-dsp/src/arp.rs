//! Arpeggiator: up&down / up / down / random over the held notes
//! (sorted by pitch) and 1..4 octaves, tempo-synced step length, gate as a fraction of a step.
//! It free-runs from the first key press (no host song-position alignment).

use crate::params::ARP_TICKS;
use crate::util::Rng;

#[derive(Clone, Debug)]
pub struct Arp {
    pub kind: i32,
    pub range: i32,
    pub beat: usize,
    pub gate: f32,
    pos: f32,
    oct: i32,
    idx: i32,
    down: bool,
    started: bool,
    sounding: Option<u8>,
    off_at: f32,
    pub velocity: u8,
    rng: Rng,
}

impl Default for Arp {
    fn default() -> Self {
        Self {
            kind: 1,
            range: 0,
            beat: 11,
            gate: 0.5,
            pos: 0.0,
            oct: 0,
            idx: -1,
            down: false,
            started: false,
            sounding: None,
            off_at: 0.0,
            velocity: 100,
            rng: Rng::new(0xA5A5),
        }
    }
}

/// What the arpeggiator wants the voice allocator to do this chunk.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ArpOut {
    pub off: Option<u8>,
    pub on: Option<u8>,
}

impl Arp {
    pub fn step_samples(&self, sr: f32, bpm: f32) -> f32 {
        ARP_TICKS[self.beat.min(18)] as f32 * sr * 60.0 / (bpm.max(1.0) * 24.0)
    }

    /// First key pressed: restart the pattern.
    pub fn reset(&mut self, velocity: u8) {
        self.pos = 0.0;
        self.oct = 0;
        self.idx = -1;
        self.down = self.kind == 3;
        self.started = false;
        self.velocity = velocity;
    }

    /// Stop the sounding note (all keys released / arp switched off).
    pub fn stop(&mut self) -> Option<u8> {
        self.started = false;
        self.sounding.take()
    }

    fn next_note(&mut self, held: &[u8]) -> Option<u8> {
        let len = held.len() as i32;
        if len == 0 {
            return None;
        }
        let range = self.range.clamp(0, 3);
        if self.idx >= len {
            self.idx = len - 1;
        }
        match self.kind {
            4 => {
                self.oct = (self.rng.next_u32() % (range as u32 + 1)) as i32;
                self.idx = (self.rng.next_u32() % len as u32) as i32;
            }
            3 => {
                if self.idx < 0 {
                    self.oct = range;
                    self.idx = len - 1;
                } else if self.idx > 0 {
                    self.idx -= 1;
                } else {
                    self.idx = len - 1;
                    self.oct = if self.oct > 0 { self.oct - 1 } else { range };
                }
            }
            2 => {
                if self.idx < 0 {
                    self.oct = 0;
                    self.idx = 0;
                } else if self.idx + 1 < len {
                    self.idx += 1;
                } else {
                    self.idx = 0;
                    self.oct = if self.oct < range { self.oct + 1 } else { 0 };
                }
            }
            _ => {
                if self.idx < 0 {
                    self.oct = 0;
                    self.idx = 0;
                    self.down = false;
                } else {
                    let at_top = self.oct == range && self.idx == len - 1;
                    let at_bottom = self.oct == 0 && self.idx == 0;
                    if !self.down && at_top && !(at_bottom) {
                        self.down = true;
                    } else if self.down && at_bottom && !(at_top) {
                        self.down = false;
                    }
                    if at_top && at_bottom {
                        // single position: repeat
                    } else if !self.down {
                        if self.idx + 1 < len {
                            self.idx += 1;
                        } else {
                            self.idx = 0;
                            self.oct += 1;
                        }
                    } else if self.idx > 0 {
                        self.idx -= 1;
                    } else {
                        self.idx = len - 1;
                        self.oct -= 1;
                    }
                }
            }
        }
        let n = held[self.idx as usize] as i32 + 12 * self.oct;
        (n <= 127).then_some(n as u8)
    }

    /// Advance by `n` samples. `held` must be sorted ascending.
    pub fn advance(&mut self, n: usize, sr: f32, bpm: f32, held: &[u8]) -> ArpOut {
        let mut out = ArpOut::default();
        if held.is_empty() {
            out.off = self.stop();
            return out;
        }
        let step = self.step_samples(sr, bpm).max(1.0);
        // Gate release inside the step.
        if self.sounding.is_some() && self.gate < 1.0 && self.pos >= self.off_at {
            out.off = self.sounding.take();
        }
        if !self.started || self.pos >= step {
            if self.started {
                self.pos -= step;
            }
            self.started = true;
            if let Some(s) = self.sounding.take() {
                out.off = Some(s);
            }
            if self.gate > 0.0 {
                if let Some(note) = self.next_note(held) {
                    out.on = Some(note);
                    self.sounding = Some(note);
                    self.off_at = step * self.gate;
                }
            }
        }
        self.pos += n as f32;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(kind: i32, range: i32, held: &[u8], steps: usize) -> Vec<u8> {
        let mut a = Arp { kind, range, gate: 0.5, ..Default::default() };
        a.reset(100);
        let sr = 48_000.0;
        let step = a.step_samples(sr, 120.0) as usize;
        let mut notes = vec![];
        let mut t = 0;
        while notes.len() < steps {
            let o = a.advance(16, sr, 120.0, held);
            if let Some(n) = o.on {
                notes.push(n);
            }
            t += 16;
            assert!(t < step * (steps + 2) + 64);
        }
        notes
    }

    #[test]
    fn up_down_patterns() {
        assert_eq!(run(2, 1, &[60, 64], 6), vec![60, 64, 72, 76, 60, 64]);
        assert_eq!(run(3, 1, &[60, 64], 5), vec![76, 72, 64, 60, 76]);
        assert_eq!(run(1, 1, &[60, 64], 8), vec![60, 64, 72, 76, 72, 64, 60, 64]);
        assert_eq!(run(1, 0, &[60], 3), vec![60, 60, 60]);
    }
}
