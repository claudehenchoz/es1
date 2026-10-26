//! Output scope in the header display, in the manner of the minilogue's OLED: one bright trace
//! on black, steady on a held note.
//!
//! The view is pitch-tracked: a YIN-style difference function on the decimated signal finds
//! the period and the trace spans two periods. On a new pitch the trace starts at the steepest
//! rising zero crossing; after that each frame starts where one period best matches the previous
//! frame, so a slowly evolving wave (detuning, PWM) changes shape in place instead of jumping.
//! Signals without a clear period (noise, dense chords) get a fixed 20 ms window.

use super::theme::*;
use crate::{Shared, SCOPE_LEN};
use nih_plug_egui::egui::{self, pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke};
use std::time::Duration;

/// Decimation for the period search.
const DEC: usize = 2;
/// Integration window and longest lag of the search, in decimated samples.
const WIN: usize = 512;
const MAX_LAG: usize = (SCOPE_LEN / DEC) - WIN;
const MIN_LAG: usize = 6;

pub struct ScopeView {
    buf: Vec<f32>,
    dec: Vec<f32>,
    diff: Vec<f32>,
    /// Smoothed peak, so the trace follows the envelope without pumping.
    peak: f32,
    /// Smoothed period, so a pitch-tracked view does not flicker between candidates.
    period: f32,
    /// One period of the previous frame, for shape tracking; empty after a pitch change.
    prev: Vec<f32>,
}

/// Points per period used for shape tracking.
const MATCH_N: usize = 64;

fn sample(buf: &[f32], t: f32) -> f32 {
    let i = (t.max(0.0).floor() as usize).min(buf.len() - 2);
    let f = (t - i as f32).clamp(0.0, 1.0);
    buf[i] * (1.0 - f) + buf[i + 1] * f
}

impl Default for ScopeView {
    fn default() -> Self {
        Self { buf: vec![0.0; SCOPE_LEN], dec: vec![0.0; SCOPE_LEN / DEC], diff: vec![0.0; MAX_LAG + 1], peak: 0.0, period: 0.0, prev: Vec::with_capacity(MATCH_N) }
    }
}

impl ScopeView {
    /// Period in samples, if the signal has a clear one.
    fn find_period(&mut self) -> Option<f32> {
        for (d, c) in self.dec.iter_mut().zip(self.buf.chunks_exact(DEC)) {
            *d = c.iter().sum::<f32>() / DEC as f32;
        }
        // Compare the newest WIN samples against the same span `lag` samples earlier.
        let x = &self.dec;
        let n = x.len();
        let head = &x[n - WIN..];
        let mut run = 0.0;
        self.diff[0] = 1.0;
        let mut best = None;
        for lag in 1..=MAX_LAG {
            let past = &x[n - WIN - lag..n - lag];
            let d: f32 = head.iter().zip(past).map(|(a, b)| (a - b) * (a - b)).sum();
            run += d;
            // Cumulative-mean normalised difference (YIN step 3).
            let nd = if run > 0.0 { d * lag as f32 / run } else { 1.0 };
            self.diff[lag] = nd;
            if lag > MIN_LAG && best.is_none() && self.diff[lag - 1] < 0.2 && nd > self.diff[lag - 1] {
                best = Some(lag - 1);
                break;
            }
        }
        let lag = best?;
        // Parabolic refinement around the dip.
        let (a, b, c) = (self.diff[lag - 1], self.diff[lag], self.diff[lag + 1]);
        let den = a - 2.0 * b + c;
        let off = if den.abs() > 1e-9 { 0.5 * (a - c) / den } else { 0.0 };
        Some((lag as f32 + off.clamp(-0.5, 0.5)) * DEC as f32)
    }

    pub fn draw(&mut self, ui: &egui::Ui, painter: &Painter, rect: Rect, shared: &Shared) {
        let pl = pal();
        shared.scope.snapshot(&mut self.buf);
        let n = self.buf.len();
        let recent = &self.buf[n - 2048..];
        let peak = recent.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        // Instant attack, ~0.4 s release at 60 fps.
        self.peak = if peak > self.peak { peak } else { self.peak * 0.93 + peak * 0.07 };
        let live = self.peak > 1e-4;

        // Graticule: centre line and quarter ticks, very faint.
        let mid = rect.center().y;
        let faint = pl.display_dim.gamma_multiply(0.35);
        dotted(painter, pos2(rect.left(), mid), rect.right(), faint);
        for i in 1..4 {
            let x = rect.left() + rect.width() * i as f32 / 4.0;
            painter.line_segment([pos2(x, rect.top() + 3.0), pos2(x, rect.top() + 6.0)], Stroke::new(1.0, faint));
            painter.line_segment([pos2(x, rect.bottom() - 6.0), pos2(x, rect.bottom() - 3.0)], Stroke::new(1.0, faint));
        }

        if !live {
            self.period = 0.0;
            painter.line_segment([pos2(rect.left(), mid), pos2(rect.right(), mid)], Stroke::new(1.2, pl.display_fg.gamma_multiply(0.55)));
            ui.ctx().request_repaint_after(Duration::from_millis(60));
            return;
        }

        let period = self.find_period();
        if let Some(p) = period {
            // Follow small drift (vibrato, glide) smoothly; jump on a new note.
            if self.period > 0.0 && (p / self.period - 1.0).abs() < 0.06 {
                self.period += (p - self.period) * 0.35;
            } else {
                self.period = p;
                self.prev.clear();
            }
        } else {
            self.period = 0.0;
            self.prev.clear();
        }
        let span = if self.period > 0.0 {
            // At least ~3 ms on screen so very high notes still read as a waveform.
            let k = (144.0 / self.period).ceil().max(2.0);
            (self.period * k).min((n / 2) as f32)
        } else {
            960.0
        };

        // Trigger: steepest rising zero crossing within one period before the window.
        let latest_start = n as f32 - span - 2.0;
        let search = if self.period > 0.0 { self.period } else { 480.0 };
        let lo = (latest_start - search).max(1.0) as usize;
        let hi = latest_start as usize;
        let mut start = latest_start;
        if self.prev.len() == MATCH_N {
            let step = self.period / MATCH_N as f32;
            let mut best = f32::MAX;
            for i in lo..=hi {
                let err: f32 = self.prev.iter().enumerate().map(|(j, &q)| {
                    let d = sample(&self.buf, i as f32 + step * j as f32) - q;
                    d * d
                }).sum();
                if err < best {
                    best = err;
                    start = i as f32;
                }
            }
        } else {
            let mut best = 0.0;
            for i in lo..hi {
                let (a, b) = (self.buf[i - 1], self.buf[i]);
                if a < 0.0 && b >= 0.0 && b - a > best {
                    best = b - a;
                    start = (i - 1) as f32 + -a / (b - a);
                }
            }
        }
        if self.period > 0.0 {
            let step = self.period / MATCH_N as f32;
            self.prev.clear();
            self.prev.extend((0..MATCH_N).map(|j| sample(&self.buf, start + step * j as f32)));
        }

        let gain = 0.86 / self.peak.max(0.02);
        let h = rect.height() * 0.5;
        let cols = rect.width().round() as usize;
        let pts: Vec<Pos2> = (0..=cols)
            .map(|c| {
                let v = sample(&self.buf, start + span * c as f32 / cols as f32);
                pos2(rect.left() + c as f32, mid - (v * gain).clamp(-1.0, 1.0) * h)
            })
            .collect();
        // Phosphor: a soft halo under a crisp line.
        painter.add(Shape::line(pts.clone(), Stroke::new(4.0, pl.display_fg.gamma_multiply(0.10))));
        painter.add(Shape::line(pts, Stroke::new(1.3, pl.display_fg)));
        ui.ctx().request_repaint();
    }
}

fn dotted(painter: &Painter, from: Pos2, to_x: f32, color: Color32) {
    let mut x = from.x;
    while x < to_x {
        painter.rect_filled(Rect::from_min_size(pos2(x, from.y - 0.5), vec2(1.0, 1.0)), 0.0, color);
        x += 4.0;
    }
}
