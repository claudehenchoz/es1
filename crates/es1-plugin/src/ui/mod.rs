// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! ES 1 editor. "Weniger, aber besser".
//!
//! The panel is a fixed grid of tiles in five rows that follow the signal flow:
//! sources → shaping → modulation → voice → effects. Every row spans the full width, so tile
//! edges line up and the gutters are equal. Inside a tile there is a heading (with the section
//! power key at the right), a *line* of keys/sliders and a *cell row* of knobs, steppers and
//! displays whose labels all share one baseline. Envelopes are faders spanning both.

mod paint;
mod scope;
mod theme;
mod widgets;

use crate::params::{Es1Params, HostParam, StoredBank, NO_PROGRAM};
use crate::Shared;
use es1_dsp::params::{Kind, PARAMS};
use es1_presets::{factory_bank, load_path, parse_sy1, to_latin1, write_sy1, Bank, Sy1Patch};
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, pos2, vec2, Align, Align2, CornerRadius, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;
use paint::*;
use theme::*;
use widgets::*;

const MARGIN: f32 = 16.0;
const GAP: f32 = 8.0;
const ROW_GAP: f32 = 10.0;
/// From the header to the first row; the top plate ends half-way.
const HEADER_GAP: f32 = 22.0;
const HEADER_H: f32 = 64.0;
const TILE_PAD: f32 = 12.0;
const TILE_HEAD: f32 = 30.0;
const TILE_H: f32 = TILE_HEAD + BODY_H + 10.0;
const CONTENT_W: f32 = 1104.0;
const ROWS: usize = 5;

pub const WIDTH: u32 = (CONTENT_W + 2.0 * MARGIN) as u32;
pub const HEIGHT: u32 = (2.0 * MARGIN + HEADER_H + HEADER_GAP + ROWS as f32 * TILE_H + (ROWS - 1) as f32 * ROW_GAP) as u32;

/// Preset change animation length.
const TWEEN_S: f32 = 0.22;

struct Tween {
    start: Instant,
    /// (index into `Es1Params::list`, from, to): normalized values of continuous parameters.
    items: Vec<(usize, f32, f32)>,
}

struct UiState {
    #[cfg(debug_assertions)]
    started: Option<Instant>,
    banks: Vec<Bank>,
    browser: bool,
    bank_sel: usize,
    message: Option<String>,
    tween: Option<Tween>,
    /// A file dialog running on its worker thread.
    dialog: Option<Dialog>,
    /// The about panel (legal notices) is open.
    about: bool,
    scope: scope::ScopeView,
}

fn all_banks(params: &Es1Params) -> Vec<Bank> {
    let mut v = vec![factory_bank()];
    for b in params.banks.read().iter() {
        v.push(Bank { name: b.name.clone(), patches: b.patches.iter().filter_map(|s| parse_sy1(s.as_bytes()).ok()).collect() });
    }
    v
}

pub fn create(params: Arc<Es1Params>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    let state: Arc<EguiState> = params.editor_state.clone();
    let p2 = params.clone();
    create_egui_editor(
        state,
        UiState {
            #[cfg(debug_assertions)]
            started: None,
            banks: Vec::new(),
            browser: false,
            bank_sel: 0,
            message: None,
            tween: None,
            dialog: None,
            about: false,
            scope: scope::ScopeView::default(),
        },
        move |ctx, st| {
            load_settings();
            install_fonts(ctx);
            apply(ctx);
            st.banks = all_banks(&p2);
            st.bank_sel = p2.patch.read().bank.min(st.banks.len().saturating_sub(1));
        },
        move |ctx, setter, st| {
            #[cfg(debug_assertions)]
            selftest(&params, &shared, setter, st, ctx);
            run_tween(&params, setter, st, ctx);
            poll_dialog(&params, &shared, setter, st, ctx);
            egui::CentralPanel::default().frame(egui::Frame::new().fill(pal().body)).show(ctx, |ui| {
                let cx = Cx { p: &params, s: setter };
                let full = ui.max_rect();
                gradient_rrect(ui.painter(), full, 0.0, pal().body_hi, pal().body);
                let origin = full.min + vec2(MARGIN, MARGIN);
                let header = Rect::from_min_size(origin, vec2(CONTENT_W, HEADER_H));
                brushed_plate(ui.painter(), Rect::from_min_max(full.min, pos2(full.right(), header.bottom() + HEADER_GAP * 0.5)));
                ui.scope_builder(UiBuilder::new().max_rect(header), |ui| header_bar(ui, &cx, &shared, st, ctx));
                let body_top = header.bottom() + HEADER_GAP;
                if st.browser {
                    let r = Rect::from_min_max(pos2(origin.x, body_top), pos2(origin.x + CONTENT_W, full.bottom() - MARGIN));
                    browser(ui, &cx, &shared, st, r);
                } else {
                    panel(ui, &cx, pos2(origin.x, body_top));
                }
            });
            if st.about {
                about(ctx, st);
            }
        },
    )
}

/// Debug builds only: `ES1_UI_SELFTEST=step` steps to the next program 1.5 s after opening, so
/// the preset tween can be captured; `=popup` opens the LFO 1 destination list; `=browser`
/// opens the program browser; `=about` the about panel; `=import` presses the import key.
#[cfg(debug_assertions)]
fn selftest(params: &Es1Params, shared: &Shared, setter: &ParamSetter, st: &mut UiState, ctx: &egui::Context) {
    let Ok(mode) = std::env::var("ES1_UI_SELFTEST") else { return };
    let t0 = *st.started.get_or_insert_with(Instant::now);
    let el = t0.elapsed().as_secs_f32();
    if el < 1.5 {
        ctx.request_repaint();
        return;
    }
    if st.started.is_some() && el < 10.0 {
        st.started = Some(t0 - std::time::Duration::from_secs(100));
        match mode.as_str() {
            "step" => {
                let cx = Cx { p: params, s: setter };
                step_program(&cx, shared, st, 82);
            }
            "popup" => ctx.memory_mut(|m| m.open_popup(egui::Id::new(("es1-dropdown", "LFO 1 Dest")))),
            "browser" => st.browser = true,
            "about" => st.about = true,
            // `import` opens the file dialog; with ES1_UI_IMPORT=<path> it imports that file directly.
            "import" => match std::env::var_os("ES1_UI_IMPORT") {
                Some(path) => import(params, shared, setter, st, std::path::Path::new(&path)),
                None => start_import(st),
            },
            // `note[:program]`: optionally select a factory program, then hold A3 for the scope.
            m if m.starts_with("note") => {
                if let Some(n) = m.strip_prefix("note:").and_then(|n| n.parse::<i32>().ok()) {
                    let cx = Cx { p: params, s: setter };
                    step_program(&cx, shared, st, n);
                }
                shared.test_note.store(57, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Parameter access helpers
// ---------------------------------------------------------------------------------------------

struct Cx<'a> {
    p: &'a Es1Params,
    s: &'a ParamSetter<'a>,
}

impl Cx<'_> {
    fn f(&self, sy1: usize) -> &FloatParam {
        match self.p.get(sy1).0 {
            HostParam::F(p) => p,
            _ => panic!("sy1 {sy1} is not continuous"),
        }
    }
    fn i(&self, sy1: usize) -> &IntParam {
        match self.p.get(sy1).0 {
            HostParam::I(p) => p,
            _ => panic!("sy1 {sy1} is not an int/choice"),
        }
    }
    fn b(&self, sy1: usize) -> &BoolParam {
        match self.p.get(sy1).0 {
            HostParam::B(p) => p,
            _ => panic!("sy1 {sy1} is not a switch"),
        }
    }
    fn knob(&self, ui: &mut Ui, sy1: usize, label: &str) {
        knob(ui, self.f(sy1), self.s, label, false, false, None);
    }
    fn bip(&self, ui: &mut Ui, sy1: usize, label: &str) {
        knob(ui, self.f(sy1), self.s, label, false, true, None);
    }
    fn fader(&self, ui: &mut Ui, sy1: usize, label: &str) {
        fader(ui, self.f(sy1), self.s, label);
    }
    fn toggle(&self, ui: &mut Ui, sy1: usize, text: &str) {
        toggle(ui, self.b(sy1), self.s, text);
    }
    fn stepper(&self, ui: &mut Ui, sy1: usize, label: &str) {
        stepper(ui, self.i(sy1), self.s, label, 64.0);
    }
    fn hslider(&self, ui: &mut Ui, sy1: usize, w: f32, l: &str, r: &str) {
        hslider(ui, self.f(sy1), self.s, w, l, r, true);
    }
    fn base(&self, sy1: usize) -> i32 {
        let def = self.p.get(sy1).1;
        if matches!(def.kind, Kind::Choice { .. }) {
            0
        } else {
            def.min
        }
    }
    fn icons(&self, ui: &mut Ui, sy1: usize) {
        let Kind::Choice { labels, .. } = self.p.get(sy1).1.kind else { return };
        let segs: Vec<Seg> = labels.iter().map(|l| Seg::Icon(l)).collect();
        segmented(ui, self.i(sy1), self.s, &segs, 0, None);
    }
    fn seg(&self, ui: &mut Ui, sy1: usize, labels: &[&str], caption: Option<&str>) {
        let segs: Vec<Seg> = labels.iter().map(|l| Seg::Text(l)).collect();
        segmented(ui, self.i(sy1), self.s, &segs, self.base(sy1), caption);
    }
    fn choice_items(&self, sy1: usize) -> Vec<(i32, String)> {
        match self.p.get(sy1).1.kind {
            Kind::Choice { labels, .. } => labels.iter().enumerate().map(|(i, l)| (i as i32, l.to_string())).collect(),
            _ => Vec::new(),
        }
    }
    fn drop_cell(&self, ui: &mut Ui, sy1: usize, label: &str, w: f32) {
        dropdown(ui, self.i(sy1), self.s, Some(label), &self.choice_items(sy1), w);
    }
    fn drop_line(&self, ui: &mut Ui, sy1: usize, w: f32) {
        dropdown(ui, self.i(sy1), self.s, None, &self.choice_items(sy1), w);
    }
    fn int(&self, sy1: usize) -> i32 {
        self.i(sy1).value()
    }
}

/// Run `f` in a left-to-right layout confined to `r`, vertically centred.
fn put(ui: &mut Ui, r: Rect, spacing: f32, f: impl FnOnce(&mut Ui)) {
    ui.scope_builder(UiBuilder::new().max_rect(r).layout(Layout::left_to_right(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        f(ui)
    });
}

/// Like [`put`] but top-aligned (for controls spanning the whole tile body).
fn put_top(ui: &mut Ui, r: Rect, spacing: f32, f: impl FnOnce(&mut Ui)) {
    ui.scope_builder(UiBuilder::new().max_rect(r).layout(Layout::left_to_right(Align::Min)), |ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        f(ui)
    });
}

// ---------------------------------------------------------------------------------------------
// Panel grid
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum T {
    Osc1,
    Sub,
    Osc2,
    Mix,
    ModEnv,
    Filter,
    FilterEnv,
    Amp,
    Lfo1,
    Lfo2,
    Arp,
    Voice,
    Unison,
    Control,
    Effect,
    Eq,
    Delay,
    Chorus,
}

/// (tile, title, power switch sy1, minimum width)
type TileSpec = (T, &'static str, Option<usize>, f32);

const GRID: [&[TileSpec]; ROWS] = [
    &[(T::Osc1, "oscillator 1", None, 150.0), (T::Sub, "sub oscillator", None, 200.0), (T::Osc2, "oscillator 2", None, 330.0), (T::Mix, "mix", None, 270.0)],
    &[(T::ModEnv, "mod envelope", Some(10), 200.0), (T::Filter, "filter", None, 330.0), (T::FilterEnv, "filter envelope", None, 180.0), (T::Amp, "amplifier", None, 290.0)],
    &[(T::Lfo1, "lfo 1", Some(57), 310.0), (T::Lfo2, "lfo 2", Some(58), 310.0), (T::Arp, "arpeggiator", Some(59), 320.0)],
    &[(T::Voice, "voice", None, 240.0), (T::Unison, "unison", Some(73), 280.0), (T::Control, "controllers", None, 400.0)],
    &[(T::Effect, "effect", Some(77), 192.0), (T::Eq, "equalizer", None, 254.0), (T::Delay, "delay", Some(65), 322.0), (T::Chorus, "chorus / flanger", Some(66), 310.0)],
];

/// A section plate: lifted off the body by a soft shadow, lit from above.
fn tile_face(painter: &egui::Painter, r: Rect) {
    let pl = pal();
    shadow(painter, r, 5.0, 2, 8);
    gradient_rrect(painter, r, 5.0, pl.tile_hi, pl.tile);
    painter.rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0, pl.tile_edge), StrokeKind::Inside);
    sheen(painter, r, 5.0);
}

fn panel(ui: &mut Ui, cx: &Cx, origin: egui::Pos2) {
    let pl = pal();
    let mut y = origin.y;
    for row in GRID {
        let n = row.len() as f32;
        let min: f32 = row.iter().map(|t| t.3).sum();
        let extra = ((CONTENT_W - GAP * (n - 1.0) - min) / n).max(0.0);
        let mut x = origin.x;
        for &(t, title, power_sy1, w) in row {
            let w = w + extra;
            let r = Rect::from_min_size(pos2(x, y), vec2(w, TILE_H));
            tile_face(ui.painter(), r);
            ui.painter().text(pos2(r.left() + TILE_PAD, r.top() + 16.0), Align2::LEFT_CENTER, title, heading_font(), pl.ink);
            if let Some(sy1) = power_sy1 {
                let pr = Rect::from_min_size(pos2(r.right() - TILE_PAD - 44.0, r.top() + 7.0), vec2(44.0, 18.0));
                power(ui, cx.b(sy1), cx.s, pr);
            }
            let l1 = Rect::from_min_size(pos2(r.left() + TILE_PAD, r.top() + TILE_HEAD), vec2(w - 2.0 * TILE_PAD, LINE_H));
            let l2 = Rect::from_min_size(pos2(l1.left(), l1.bottom() + LINE_GAP), vec2(l1.width(), CELL_H));
            tile(ui, cx, t, l1, l2);
            x += w + GAP;
        }
        y += TILE_H + ROW_GAP;
    }
}

/// Effect block knob legends follow the selected effect type.
fn effect_labels(kind: i32) -> [&'static str; 3] {
    match kind {
        0..=2 => ["drive", "tone", "level"],
        3 => ["rate", "bits", "mix"],
        4 => ["freq", "-", "mix"],
        5 => ["thresh", "time", "gain"],
        _ => ["range", "rate", "feedback"],
    }
}

fn tile(ui: &mut Ui, cx: &Cx, t: T, l1: Rect, l2: Rect) {
    let body = l1.union(l2);
    match t {
        T::Osc1 => {
            put(ui, l1, 8.0, |ui| cx.icons(ui, 0));
            put(ui, l2, 0.0, |ui| {
                cx.knob(ui, 76, "detune");
                cx.knob(ui, 45, "fm");
            });
        }
        T::Sub => {
            put(ui, l1, 10.0, |ui| {
                cx.icons(ui, 96);
                cx.seg(ui, 97, &["0", "-1"], Some("oct"));
            });
            put(ui, l2, 0.0, |ui| cx.knob(ui, 95, "level"));
        }
        T::Osc2 => {
            put(ui, l1, 6.0, |ui| {
                cx.icons(ui, 1);
                ui.add_space(6.0);
                cx.toggle(ui, 6, "sync");
                cx.toggle(ui, 7, "ring");
                cx.toggle(ui, 4, "key track");
            });
            put(ui, l2, 0.0, |ui| {
                cx.bip(ui, 2, "pitch");
                cx.bip(ui, 3, "fine");
            });
        }
        T::Mix => {
            put(ui, l1, 0.0, |ui| cx.hslider(ui, 5, 200.0, "osc 1", "osc 2"));
            put(ui, l2, 0.0, |ui| {
                cx.knob(ui, 8, "p/w");
                cx.knob(ui, 91, "phase");
                cx.bip(ui, 72, "tune");
                cx.stepper(ui, 9, "transpose");
            });
        }
        T::ModEnv => {
            put(ui, l1, 0.0, |ui| cx.seg(ui, 71, &["osc 2", "fm", "p/w"], Some("to")));
            put(ui, l2, 0.0, |ui| {
                cx.knob(ui, 12, "attack");
                cx.knob(ui, 13, "decay");
                cx.bip(ui, 11, "amount");
            });
        }
        T::Filter => {
            put(ui, l1, 10.0, |ui| {
                cx.seg(ui, 14, &["lp 12", "lp 24", "hp 12", "bp 12", "lp dl"], None);
                cx.toggle(ui, 24, "velocity");
            });
            put(ui, l2, 0.0, |ui| {
                knob(ui, cx.f(19), cx.s, "cutoff", true, false, None);
                cx.knob(ui, 20, "resonance");
                cx.bip(ui, 21, "envelope");
                cx.knob(ui, 22, "key track");
                cx.knob(ui, 23, "drive");
            });
        }
        T::FilterEnv => {
            put_top(ui, body, 4.0, |ui| {
                cx.fader(ui, 15, "a");
                cx.fader(ui, 16, "d");
                cx.fader(ui, 17, "s");
                cx.fader(ui, 18, "r");
            });
        }
        T::Amp => {
            put_top(ui, body, 4.0, |ui| {
                cx.fader(ui, 25, "a");
                cx.fader(ui, 26, "d");
                cx.fader(ui, 27, "s");
                cx.fader(ui, 28, "r");
                ui.add_space(12.0);
                cx.fader(ui, 29, "gain");
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.add_space(LINE_H + LINE_GAP);
                    cx.knob(ui, 30, "velocity");
                });
            });
        }
        T::Lfo1 | T::Lfo2 => {
            let (wave, dest, speed, depth, sync, key) = if matches!(t, T::Lfo1) { (42, 41, 43, 44, 67, 68) } else { (47, 46, 48, 49, 69, 70) };
            put(ui, l1, 6.0, |ui| {
                cx.icons(ui, wave);
                ui.add_space(6.0);
                cx.toggle(ui, sync, "tempo");
                cx.toggle(ui, key, "key sync");
            });
            put(ui, l2, 0.0, |ui| {
                cx.drop_cell(ui, dest, "to", 92.0);
                let synced = cx.b(sync).value();
                let shown = synced.then(|| es1_dsp::lfo::sync_label(cx.f(speed).value()));
                knob(ui, cx.f(speed), cx.s, "speed", false, false, shown);
                cx.knob(ui, depth, "depth");
            });
        }
        T::Arp => {
            put(ui, l1, 10.0, |ui| {
                cx.seg(ui, 31, &["up/down", "up", "down", "random"], None);
                cx.seg(ui, 32, &["1", "2", "3", "4"], Some("oct"));
            });
            put(ui, l2, 0.0, |ui| {
                cx.drop_cell(ui, 33, "beat", 80.0);
                cx.knob(ui, 34, "gate");
            });
        }
        T::Voice => {
            put(ui, l1, 10.0, |ui| {
                cx.seg(ui, 38, &["poly", "mono", "legato"], None);
                cx.toggle(ui, 74, "auto glide");
            });
            put(ui, l2, 0.0, |ui| {
                cx.stepper(ui, 94, "polyphony");
                cx.knob(ui, 39, "glide");
                cx.stepper(ui, 40, "bend");
            });
        }
        T::Unison => {
            put(ui, l1, 0.0, |ui| cx.seg(ui, 93, &["2", "3", "4", "5", "6", "7", "8"], Some("voices")));
            put(ui, l2, 0.0, |ui| {
                cx.knob(ui, 75, "detune");
                cx.knob(ui, 92, "phase");
                cx.bip(ui, 84, "spread");
                cx.stepper(ui, 85, "pitch");
            });
        }
        T::Control => {
            let targets: Vec<(i32, String)> = std::iter::once((-1, "none".to_string()))
                .chain(PARAMS.iter().filter(|d| d.modulatable).map(|d| (d.sy1 as i32, d.name.to_lowercase())))
                .collect();
            let slot2 = Rect::from_min_size(pos2(l2.left(), l2.top() + 4.0), vec2(l2.width(), LINE_H));
            for (r, src, dst, amt, n) in [(l1, 86, 87, 50, "1"), (slot2, 88, 89, 51, "2")] {
                put(ui, r, 6.0, |ui| {
                    ui.label(RichText::new(n).font(small_font()).color(pal().ink_soft));
                    cx.drop_line(ui, src, 100.0);
                    ui.label(RichText::new("→").font(label_font()).color(pal().ink_soft));
                    dropdown(ui, cx.i(dst), cx.s, None, &targets, 132.0);
                    hslider(ui, cx.f(amt), cx.s, 96.0, "-", "+", true);
                });
            }
            ui.painter().text(
                pos2(l2.left(), l2.bottom() - 7.0),
                Align2::LEFT_CENTER,
                "midi source → parameter, with amount",
                small_font(),
                pal().ink_soft,
            );
        }
        T::Effect => {
            put(ui, l1, 0.0, |ui| cx.drop_line(ui, 78, 120.0));
            let [a, b, c] = effect_labels(cx.int(78));
            put(ui, l2, 0.0, |ui| {
                cx.knob(ui, 79, a);
                cx.knob(ui, 80, b);
                cx.knob(ui, 81, c);
            });
        }
        T::Eq => {
            put(ui, l1, 14.0, |ui| {
                cx.hslider(ui, 60, 112.0, "lp", "hp");
                cx.hslider(ui, 90, 104.0, "L", "R");
            });
            put(ui, l2, 0.0, |ui| {
                cx.knob(ui, 61, "freq");
                cx.bip(ui, 62, "gain");
                cx.knob(ui, 63, "q");
            });
        }
        T::Delay => {
            put(ui, l1, 0.0, |ui| cx.seg(ui, 82, &["stereo", "cross", "ping-pong"], None));
            put(ui, l2, 0.0, |ui| {
                cx.drop_cell(ui, 35, "time", 72.0);
                cx.bip(ui, 83, "spread");
                cx.knob(ui, 36, "feedback");
                cx.bip(ui, 98, "tone");
                cx.knob(ui, 37, "mix");
            });
        }
        T::Chorus => {
            put(ui, l1, 0.0, |ui| cx.seg(ui, 64, &["1", "2", "3", "4"], Some("mode")));
            put(ui, l2, 0.0, |ui| {
                cx.knob(ui, 52, "time");
                cx.knob(ui, 53, "depth");
                cx.knob(ui, 54, "rate");
                cx.bip(ui, 55, "feedback");
                cx.knob(ui, 56, "level");
            });
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Patch handling and the preset tween
// ---------------------------------------------------------------------------------------------

fn finish_tween(params: &Es1Params, setter: &ParamSetter, st: &mut UiState) {
    if let Some(tw) = st.tween.take() {
        for (i, _, to) in tw.items {
            if let HostParam::F(p) = &params.list[i] {
                setter.set_parameter_normalized(p, to);
                setter.end_set_parameter(p);
            }
        }
    }
}

fn run_tween(params: &Es1Params, setter: &ParamSetter, st: &mut UiState, ctx: &egui::Context) {
    let Some(tw) = &st.tween else { return };
    let t = (tw.start.elapsed().as_secs_f32() / TWEEN_S).min(1.0);
    if t >= 1.0 {
        finish_tween(params, setter, st);
        return;
    }
    // Ease-out cubic: quick departure, soft landing.
    let e = 1.0 - (1.0 - t).powi(3);
    for &(i, from, to) in &tw.items {
        if let HostParam::F(p) = &params.list[i] {
            setter.set_parameter_normalized(p, from + (to - from) * e);
        }
    }
    ctx.request_repaint();
}

/// Load a patch: discrete values switch at once, continuous ones glide over `TWEEN_S`.
fn load_patch(params: &Es1Params, shared: &Shared, setter: &ParamSetter, st: &mut UiState, patch: &Sy1Patch, bank: usize, program: usize) {
    finish_tween(params, setter, st);
    let v = patch.to_values();
    let mut items = Vec::new();
    for (i, (hp, def)) in params.list.iter().zip(PARAMS).enumerate() {
        let norm = hp.normalized_for(def, v[def.sy1 as usize]);
        match hp {
            HostParam::F(p) => {
                let from = p.unmodulated_normalized_value();
                if (from - norm).abs() > 1e-6 {
                    setter.begin_set_parameter(p);
                    items.push((i, from, norm));
                }
            }
            HostParam::I(p) => {
                setter.begin_set_parameter(p);
                setter.set_parameter_normalized(p, norm);
                setter.end_set_parameter(p);
            }
            HostParam::B(p) => {
                setter.begin_set_parameter(p);
                setter.set_parameter_normalized(p, norm);
                setter.end_set_parameter(p);
            }
        }
    }
    st.tween = Some(Tween { start: Instant::now(), items });
    {
        let mut s = params.patch.write();
        s.name = patch.name.clone();
        s.bank = bank;
        s.program = program;
    }
    shared.patch_loaded.store(true, Ordering::Release);
}

fn current_values(params: &Es1Params) -> es1_dsp::Values {
    let f = params.values();
    let mut v = [0i32; es1_dsp::params::NUM_SY1];
    for (i, x) in f.iter().enumerate() {
        v[i] = x.round() as i32;
    }
    v
}

fn step_program(cx: &Cx, shared: &Shared, st: &mut UiState, delta: i32) {
    let (b, p) = {
        let s = cx.p.patch.read();
        (s.bank.min(st.banks.len() - 1), s.program)
    };
    let n = st.banks[b].patches.len();
    if n == 0 {
        return;
    }
    let p = if p == NO_PROGRAM {
        if delta > 0 {
            0
        } else {
            n - 1
        }
    } else {
        ((p as i32 + delta).rem_euclid(n as i32)) as usize
    };
    let patch = st.banks[b].patches[p].clone();
    load_patch(cx.p, shared, cx.s, st, &patch, b, p);
}

// File dialogs run on a worker thread. A modal dialog pumps the window's messages while it is
// open; called from inside the editor's frame it re-enters baseview's window procedure and
// panics ("RefCell already borrowed"), which aborts the host.

enum DialogKind {
    Import,
    /// Save the patch text (already serialized when the dialog opened).
    Save(Vec<u8>),
}

struct Dialog {
    kind: DialogKind,
    /// In a mutex only because the editor state must be `Sync`.
    rx: parking_lot::Mutex<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
}

fn spawn_dialog(st: &mut UiState, kind: DialogKind, run: impl FnOnce() -> Option<std::path::PathBuf> + Send + 'static) {
    if st.dialog.is_some() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run());
    });
    st.dialog = Some(Dialog { kind, rx: parking_lot::Mutex::new(rx) });
}

fn start_import(st: &mut UiState) {
    spawn_dialog(st, DialogKind::Import, || rfd::FileDialog::new().add_filter("Patches", &["sy1", "zip"]).pick_file());
}

fn start_save(params: &Es1Params, st: &mut UiState) {
    let name = params.patch.read().name.clone();
    let text = to_latin1(&write_sy1(&Sy1Patch::from_values(&name, &current_values(params))));
    spawn_dialog(st, DialogKind::Save(text), move || {
        rfd::FileDialog::new().add_filter("Patch", &["sy1"]).set_file_name(format!("{name}.sy1")).save_file()
    });
}

/// Pick up a finished dialog; while one is open, keep polling.
fn poll_dialog(params: &Es1Params, shared: &Shared, setter: &ParamSetter, st: &mut UiState, ctx: &egui::Context) {
    let Some(d) = &st.dialog else { return };
    let path = match d.rx.lock().try_recv() {
        Ok(path) => path,
        Err(std::sync::mpsc::TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
    };
    let kind = st.dialog.take().map(|d| d.kind);
    let Some(path) = path else { return };
    match kind {
        Some(DialogKind::Import) => import(params, shared, setter, st, &path),
        Some(DialogKind::Save(text)) => {
            st.message = Some(match std::fs::write(&path, text) {
                Ok(()) => "saved".into(),
                Err(e) => format!("save failed: {e}"),
            });
        }
        None => {}
    }
}

/// Add the banks in `path`; a single patch is loaded straight away.
fn import(params: &Es1Params, shared: &Shared, setter: &ParamSetter, st: &mut UiState, path: &std::path::Path) {
    match load_path(path) {
        Ok(banks) => {
            let mut count = 0;
            {
                let mut stored = params.banks.write();
                for b in banks {
                    count += b.patches.len();
                    stored.push(StoredBank { name: b.name.clone(), patches: b.patches.iter().map(write_sy1).collect() });
                    st.banks.push(b);
                }
            }
            st.bank_sel = st.banks.len() - 1;
            if count == 1 {
                let b = st.bank_sel;
                let patch = st.banks[b].patches[0].clone();
                load_patch(params, shared, setter, st, &patch, b, 0);
                st.message = None;
            } else {
                st.message = Some(format!("imported {count} patches"));
            }
        }
        Err(e) => st.message = Some(format!("import failed: {e}")),
    }
}

// ---------------------------------------------------------------------------------------------
// Header: wordmark, display, transport keys, theme, master volume
// ---------------------------------------------------------------------------------------------

fn key(ui: &mut Ui, text: &str, w: f32) -> egui::Response {
    let pl = pal();
    let (r, resp) = ui.allocate_exact_size(vec2(w, LINE_H), Sense::click());
    let edge = if resp.hovered() { pl.ink_soft } else { pl.key_edge };
    let painter = ui.painter_at(r.expand(4.0));
    if resp.is_pointer_button_down_on() {
        painter.rect(r, CornerRadius::same(3), pl.key_lo, Stroke::new(1.0, edge), StrokeKind::Inside);
    } else {
        raised(&painter, r, 3.0, edge);
    }
    painter.text(r.center(), Align2::CENTER_CENTER, text, label_font(), pl.ink);
    resp
}

/// Header display: program on the left, the output scope on the right, behind one glass.
const DISPLAY_X: f32 = 156.0;
const DISPLAY_W: f32 = 520.0;
const PROGRAM_W: f32 = 300.0;

fn header_bar(ui: &mut Ui, cx: &Cx, shared: &Shared, st: &mut UiState, ctx: &egui::Context) {
    let pl = pal();
    let h = ui.max_rect();
    let mid = h.center().y;

    // Wordmark; a click opens the about panel.
    let mark = Rect::from_min_size(pos2(h.left(), mid - 22.0), vec2(DISPLAY_X - 16.0, 44.0));
    let resp = ui.interact(mark, ui.id().with("wordmark"), Sense::click()).on_hover_text("about ES 1");
    if resp.clicked() {
        st.about = !st.about;
    }
    let p = ui.painter();
    p.text(pos2(h.left() - 1.0, mid - 7.0), Align2::LEFT_CENTER, "ES 1", wordmark_font(), pl.ink);
    p.text(pos2(h.left() + 1.0, mid + 19.0), Align2::LEFT_CENTER, "elektronischer synthesizer", small_font(), pl.ink_soft);

    // Display.
    let d = Rect::from_min_size(pos2(h.left() + DISPLAY_X, mid - 26.0), vec2(DISPLAY_W, 52.0));
    glass(ui.painter(), d, 5.0);
    let prog = Rect::from_min_size(d.min, vec2(PROGRAM_W, d.height()));
    let p = ui.painter_at(prog.shrink(2.0));
    let (name, bank, num) = {
        let s = cx.p.patch.read();
        (s.name.clone(), s.bank, s.program)
    };
    let num = if num == NO_PROGRAM { "---".to_string() } else { format!("{:03}", num + 1) };
    p.text(pos2(prog.left() + 14.0, prog.top() + 20.0), Align2::LEFT_CENTER, num, value_font(), pl.orange);
    p.text(pos2(prog.left() + 48.0, prog.top() + 20.0), Align2::LEFT_CENTER, &name, egui::FontId::proportional(17.0), pl.display_fg);
    let bank_name = st.banks.get(bank).map(|b| b.name.to_lowercase()).unwrap_or_default();
    let info = st.message.clone().unwrap_or(bank_name);
    p.text(pos2(prog.left() + 48.0, prog.bottom() - 12.0), Align2::LEFT_CENTER, info, small_font(), pl.display_dim);
    let voices = shared.active_voices.load(Ordering::Relaxed);
    let vtext = if voices == 1 { "1 voice".to_string() } else { format!("{voices} voices") };
    p.text(pos2(prog.right() - 14.0, prog.bottom() - 12.0), Align2::RIGHT_CENTER, vtext, small_font(), pl.display_dim);
    let div = prog.right();
    ui.painter().line_segment([pos2(div, d.top() + 9.0), pos2(div, d.bottom() - 9.0)], Stroke::new(1.0, pl.display_dim.gamma_multiply(0.35)));
    let scope_r = Rect::from_min_max(pos2(div + 12.0, d.top() + 7.0), pos2(d.right() - 12.0, d.bottom() - 7.0));
    st.scope.draw(ui, &ui.painter_at(scope_r.expand(2.0)), scope_r, shared);

    // Program keys.
    let keys = Rect::from_min_size(pos2(d.right() + 16.0, mid - LINE_H * 0.5), vec2(280.0, LINE_H));
    put(ui, keys, 4.0, |ui| {
        if key(ui, "‹", 28.0).on_hover_text("previous program").clicked() {
            step_program(cx, shared, st, -1);
        }
        if key(ui, "›", 28.0).on_hover_text("next program").clicked() {
            step_program(cx, shared, st, 1);
        }
        ui.add_space(6.0);
        if key(ui, if st.browser { "panel" } else { "programs" }, 76.0).clicked() {
            st.browser = !st.browser;
        }
        if key(ui, "import", 56.0).on_hover_text("Load a .sy1 patch or a .zip bank").clicked() {
            start_import(st);
        }
        if key(ui, "save", 48.0).on_hover_text("Save the current sound as a .sy1 file").clicked() {
            start_save(cx.p, st);
        }
    });

    // Theme and master volume, right-aligned.
    let right = Rect::from_min_max(pos2(h.right() - 180.0, h.top()), h.max);
    ui.scope_builder(UiBuilder::new().max_rect(right).layout(Layout::right_to_left(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 14.0;
        knob(ui, &cx.p.master, cx.s, "volume", true, false, None);
        if let Some(dark) = choice_pair(ui, "light", "dark", is_dark()) {
            if dark != is_dark() {
                set_dark(ctx, dark);
            }
        }
    });
}

/// About panel: the "Appropriate Legal Notices" the GPL asks an interactive program to show.
fn about(ctx: &egui::Context, st: &mut UiState) {
    let pl = pal();
    let screen = ctx.screen_rect();
    // Dim the panel; a click outside the card (or Escape) closes it.
    let dim = egui::Area::new(egui::Id::new("es1-about-dim")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(r, 0.0, egui::Color32::from_black_alpha(if is_dark() { 120 } else { 70 }));
        resp.clicked()
    });
    let w = 460.0;
    let card = Rect::from_center_size(screen.center(), vec2(w, 250.0));
    egui::Area::new(egui::Id::new("es1-about")).order(egui::Order::Tooltip).fixed_pos(card.min).show(ctx, |ui| {
        tile_face(ui.painter(), card);
        let inner = card.shrink(20.0);
        ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(RichText::new("ES 1").font(title_font()).color(pl.ink));
            ui.label(RichText::new(format!("elektronischer synthesizer, version {}", env!("CARGO_PKG_VERSION"))).font(small_font()).color(pl.ink_soft));
            ui.add_space(6.0);
            ui.label(RichText::new("Copyright (C) 2026 Claude Henchoz").font(label_font()).color(pl.ink));
            ui.add_space(2.0);
            for t in [
                "ES 1 is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.",
                "ES 1 is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.",
                "The license (LICENSE) and the licenses of the libraries ES 1 uses (THIRD_PARTY_LICENSES.md) are in the plugin bundle, under Contents/Resources, and at https://www.gnu.org/licenses/.",
            ] {
                ui.label(RichText::new(t).font(small_font()).color(pl.ink_soft));
            }
        });
    });
    if dim.inner || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.about = false;
    }
}

// ---------------------------------------------------------------------------------------------
// Program browser
// ---------------------------------------------------------------------------------------------

const COLS: usize = 5;

fn browser(ui: &mut Ui, cx: &Cx, shared: &Shared, st: &mut UiState, area: Rect) {
    let pl = pal();
    let banks_r = Rect::from_min_size(area.min, vec2(232.0, area.height()));
    let progs_r = Rect::from_min_max(pos2(banks_r.right() + GAP, area.top()), area.max);
    for r in [banks_r, progs_r] {
        tile_face(ui.painter(), r);
    }
    ui.painter().text(pos2(banks_r.left() + TILE_PAD, banks_r.top() + 16.0), Align2::LEFT_CENTER, "banks", heading_font(), pl.ink);
    ui.painter().text(pos2(progs_r.left() + TILE_PAD, progs_r.top() + 16.0), Align2::LEFT_CENTER, "programs", heading_font(), pl.ink);

    let inner = |r: Rect| Rect::from_min_max(pos2(r.left() + TILE_PAD, r.top() + TILE_HEAD + 4.0), pos2(r.right() - TILE_PAD, r.bottom() - TILE_PAD));
    let mut remove = None;
    ui.scope_builder(UiBuilder::new().max_rect(inner(banks_r)).layout(Layout::top_down(Align::Min)), |ui| {
        for (i, b) in st.banks.iter().enumerate() {
            ui.horizontal(|ui| {
                let sel = i == st.bank_sel;
                let (cell, resp) = ui.allocate_exact_size(vec2(180.0, 22.0), Sense::click());
                if sel {
                    ui.painter().rect_filled(cell, CornerRadius::same(3), pl.key_sel);
                } else if resp.hovered() {
                    ui.painter().rect_filled(cell, CornerRadius::same(3), pl.key);
                }
                let col = if sel { pl.key_sel_fg } else { pl.ink };
                ui.painter().text(pos2(cell.left() + 8.0, cell.center().y), Align2::LEFT_CENTER, b.name.to_lowercase(), label_font(), col);
                ui.painter().text(pos2(cell.right() - 8.0, cell.center().y), Align2::RIGHT_CENTER, b.patches.len().to_string(), small_font(), col);
                if resp.clicked() {
                    st.bank_sel = i;
                }
                if i > 0 && key(ui, "×", 22.0).on_hover_text("remove bank").clicked() {
                    remove = Some(i);
                }
            });
        }
    });
    if let Some(i) = remove {
        st.banks.remove(i);
        cx.p.banks.write().remove(i - 1);
        st.bank_sel = st.bank_sel.min(st.banks.len() - 1);
    }

    let b = st.bank_sel.min(st.banks.len() - 1);
    let cur = {
        let s = cx.p.patch.read();
        (s.bank == b).then_some(s.program)
    };
    let mut clicked = None;
    ui.scope_builder(UiBuilder::new().max_rect(inner(progs_r)).layout(Layout::top_down(Align::Min)), |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let n = st.banks[b].patches.len();
            let rows = n.div_ceil(COLS);
            let col_w = (ui.available_width() / COLS as f32).floor();
            for r in 0..rows {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    for c in 0..COLS {
                        let i = c * rows + r;
                        let (cell, resp) = ui.allocate_exact_size(vec2(col_w, 20.0), Sense::click());
                        if let Some(p) = st.banks[b].patches.get(i) {
                            let sel = cur == Some(i);
                            if resp.hovered() {
                                ui.painter().rect_filled(cell.shrink2(vec2(2.0, 1.0)), CornerRadius::same(3), pl.key);
                            }
                            let num_col = if sel { pl.orange } else { pl.ink_soft };
                            ui.painter().text(pos2(cell.left() + 6.0, cell.center().y), Align2::LEFT_CENTER, format!("{:03}", i + 1), value_font(), num_col);
                            ui.painter().text(pos2(cell.left() + 36.0, cell.center().y), Align2::LEFT_CENTER, &p.name, label_font(), if sel { pl.orange } else { pl.ink });
                            if resp.clicked() {
                                clicked = Some(i);
                            }
                        }
                    }
                });
            }
        });
    });
    if let Some(i) = clicked {
        let patch = st.banks[b].patches[i].clone();
        load_patch(cx.p, shared, cx.s, st, &patch, b, i);
        st.message = None;
    }
}
