// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Rams-style controls drawn directly with egui's painter and bound to nih-plug parameters.
//!
//! Every control fits the panel grid. A *line* is 22 px tall and holds keys, sliders and
//! displays; a *cell* is 56 × 56 px and holds a knob, stepper or display with its label. Labels
//! sit on one baseline at the bottom of their cell. Faders span both lines.

use super::paint::*;
use super::theme::*;
use nih_plug::prelude::{BoolParam, IntParam, Param, ParamSetter};
use nih_plug_egui::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Id, Painter, PopupCloseBehavior, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};

pub const CELL_W: f32 = 56.0;
pub const CELL_H: f32 = 56.0;
pub const LINE_H: f32 = 22.0;
pub const LINE_GAP: f32 = 10.0;
pub const FADER_W: f32 = 36.0;
/// Height of a control spanning both lines (faders).
pub const BODY_H: f32 = LINE_H + LINE_GAP + CELL_H;

/// Rotary range: 300 degrees, with the gap at the bottom.
const ARC: f32 = 300.0_f32 * core::f32::consts::PI / 180.0;

fn set_norm<P: Param>(setter: &ParamSetter, p: &P, v: f32) {
    setter.begin_set_parameter(p);
    setter.set_parameter_normalized(p, v);
    setter.end_set_parameter(p);
}

/// Vertical (or horizontal) drag, shift = fine, double-click = default, wheel = step.
fn drag_normalized<P: Param>(ui: &Ui, resp: &Response, p: &P, setter: &ParamSetter, px_per_full: f32, horizontal: bool) {
    if resp.drag_started() {
        setter.begin_set_parameter(p);
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = if horizontal { resp.drag_delta().x } else { -resp.drag_delta().y };
        let delta = d / if fine { px_per_full * 8.0 } else { px_per_full };
        let v = (p.unmodulated_normalized_value() + delta).clamp(0.0, 1.0);
        setter.set_parameter_normalized(p, v);
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(p);
    }
    if resp.double_clicked() {
        set_norm(setter, p, p.default_normalized_value());
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let step = p.step_count().map(|s| 1.0 / s as f32).unwrap_or(1.0 / 127.0);
            set_norm(setter, p, (p.unmodulated_normalized_value() + step * scroll.signum()).clamp(0.0, 1.0));
        }
    }
}

fn cell_label(ui: &Ui, rect: Rect, text: &str, active: bool) {
    let pl = pal();
    let (font, color) = if active { (value_font(), pl.orange) } else { (label_font(), pl.ink) };
    put_text(ui.painter(), pos2(rect.center().x, rect.bottom() - 7.0), Align2::CENTER_CENTER, text, font, color);
}

/// Round knob in a cell. `main` = inverted cap for primary controls; `bipolar` fills the scale
/// from the centre; `shown` overrides the value text (e.g. note values for a tempo-synced rate).
///
/// Drawn like a Braun rotary: a lit skirt with a dished cap, a dot scale whose dots light up to
/// the current value, and a single index line.
pub fn knob<P: Param>(ui: &mut Ui, p: &P, setter: &ParamSetter, label: &str, main: bool, bipolar: bool, shown: Option<String>) -> Response {
    let pl = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(CELL_W, CELL_H), Sense::click_and_drag());
    drag_normalized(ui, &resp, p, setter, 150.0, false);
    let painter = ui.painter_at(rect.expand(6.0));
    let r = if main { 16.0 } else { 13.0 };
    let c = pos2(rect.center().x, rect.top() + 20.0);
    let v = p.modulated_normalized_value();
    let active = resp.dragged() || resp.hovered();

    // Dot scale: 11 positions, lit from the origin (left end, or centre if bipolar) to the value.
    let (lo, hi) = if bipolar { (v.min(0.5), v.max(0.5)) } else { (0.0, v) };
    for k in 0..=10 {
        let t = k as f32 / 10.0;
        let a = -ARC * 0.5 + ARC * t;
        let lit = t >= lo - 0.05 && t <= hi + 0.05;
        let major = k == 0 || k == 10 || (bipolar && k == 5);
        let col = if lit {
            if resp.dragged() {
                pl.orange
            } else {
                pl.ink
            }
        } else {
            pl.rule
        };
        painter.circle_filled(c + vec2(a.sin(), -a.cos()) * (r + 5.5), if major { 1.35 } else { 1.0 }, col);
    }

    let (hi_c, lo_c, line) = if main { (pl.main_hi, pl.main_lo, pl.knob_main_line) } else { (pl.knob_hi, pl.knob_lo, pl.knob_line) };
    shadow(&painter, Rect::from_center_size(c, vec2(2.0 * r, 2.0 * r)), r, 2, 6);
    gradient_circle(&painter, c, r, hi_c, lo_c);
    gradient_circle(&painter, c, r - 2.5, mix(lo_c, hi_c, 0.35), mix(hi_c, lo_c, 0.08));
    painter.circle_stroke(c, r - 2.5, Stroke::new(0.8, mix(lo_c, hi_c, 0.2).gamma_multiply(0.8)));
    painter.circle_stroke(c, r, Stroke::new(1.0, if active { pl.ink_soft } else { pl.knob_edge }));

    let a = -ARC * 0.5 + ARC * v;
    let dir = vec2(a.sin(), -a.cos());
    let line = if resp.dragged() || main { pl.orange } else { line };
    painter.line_segment([c + dir * (r * 0.28), c + dir * (r - 2.0)], Stroke::new(2.0, line));
    let value = shown.unwrap_or_else(|| p.to_string());
    cell_label(ui, rect, &if active { value } else { label.to_string() }, active);
    resp.on_hover_text(p.name().to_lowercase())
}

/// Vertical fader spanning both lines (envelopes and levels).
pub fn fader<P: Param>(ui: &mut Ui, p: &P, setter: &ParamSetter, label: &str) -> Response {
    let pl = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(FADER_W, BODY_H), Sense::click_and_drag());
    let top = rect.top() + 7.0;
    let bot = rect.bottom() - 22.0;
    drag_normalized(ui, &resp, p, setter, bot - top, false);
    let painter = ui.painter_at(rect.expand(4.0));
    let x = rect.center().x;
    for k in 0..=10 {
        let y = bot - (bot - top) * k as f32 / 10.0;
        let len = if k % 5 == 0 { 5.0 } else { 3.0 };
        painter.line_segment([pos2(x - 8.0 - len, y), pos2(x - 8.0, y)], Stroke::new(1.0, pl.rule));
    }
    groove(&painter, Rect::from_min_max(pos2(x - 2.0, top - 2.0), pos2(x + 2.0, bot + 2.0)));
    let y = bot - (bot - top) * p.modulated_normalized_value();
    let cap = Rect::from_center_size(pos2(x, y), vec2(22.0, 13.0));
    let active = resp.dragged() || resp.hovered();
    shadow(&painter, cap, 2.5, 2, 5);
    gradient_rrect(&painter, cap, 2.5, pl.knob_hi, pl.knob_lo);
    painter.rect_stroke(cap, CornerRadius::same(2), Stroke::new(1.0, if active { pl.ink_soft } else { pl.knob_edge }), StrokeKind::Inside);
    sheen(&painter, cap, 2.5);
    painter.line_segment(
        [pos2(cap.left() + 3.5, y), pos2(cap.right() - 3.5, y)],
        Stroke::new(1.5, if resp.dragged() { pl.orange } else { pl.knob_line }),
    );
    cell_label(ui, rect, &if active { p.to_string() } else { label.to_string() }, active);
    resp.on_hover_text(p.name().to_lowercase())
}

/// Horizontal slider on a line, with end legends; `center` draws a centre detent mark.
pub fn hslider<P: Param>(ui: &mut Ui, p: &P, setter: &ParamSetter, w: f32, left: &str, right: &str, center: bool) -> Response {
    let pl = pal();
    let lw = text_width(ui.ctx(), left, small_font());
    let rw = text_width(ui.ctx(), right, small_font());
    let (rect, resp) = ui.allocate_exact_size(vec2(w, LINE_H), Sense::click_and_drag());
    let x0 = rect.left() + lw + 10.0;
    let x1 = rect.right() - rw - 10.0;
    drag_normalized(ui, &resp, p, setter, x1 - x0, true);
    let painter = ui.painter_at(rect.expand(2.0));
    let y = rect.center().y;
    put_text(&painter, pos2(rect.left(), y), Align2::LEFT_CENTER, left, small_font(), pl.ink_soft);
    put_text(&painter, pos2(rect.right(), y), Align2::RIGHT_CENTER, right, small_font(), pl.ink_soft);
    groove(&painter, Rect::from_min_max(pos2(x0 - 2.0, y - 2.0), pos2(x1 + 2.0, y + 2.0)));
    if center {
        let xc = 0.5 * (x0 + x1);
        painter.line_segment([pos2(xc, y - 8.0), pos2(xc, y - 4.0)], Stroke::new(1.0, pl.ink_soft));
        painter.line_segment([pos2(xc, y + 4.0), pos2(xc, y + 8.0)], Stroke::new(1.0, pl.ink_soft));
    }
    let x = x0 + (x1 - x0) * p.modulated_normalized_value();
    let cap = Rect::from_center_size(pos2(x, y), vec2(11.0, 18.0));
    shadow(&painter, cap, 2.5, 2, 4);
    gradient_rrect(&painter, cap, 2.5, pl.knob_hi, pl.knob_lo);
    let edge = if resp.dragged() || resp.hovered() { pl.ink_soft } else { pl.knob_edge };
    painter.rect_stroke(cap, CornerRadius::same(2), Stroke::new(1.0, edge), StrokeKind::Inside);
    sheen(&painter, cap, 2.5);
    painter.line_segment(
        [pos2(x, cap.top() + 4.0), pos2(x, cap.bottom() - 4.0)],
        Stroke::new(1.5, if resp.dragged() { pl.orange } else { pl.knob_line }),
    );
    let text = format!("{}  {}", p.name().to_lowercase(), p);
    if resp.dragged() {
        egui::show_tooltip_at_pointer(ui.ctx(), ui.layer_id(), resp.id.with("tip"), |ui| ui.label(&text));
        resp
    } else {
        resp.on_hover_text(text)
    }
}

// ---------------------------------------------------------------------------------------------
// Keys: segmented selectors, latching switches, section power
// ---------------------------------------------------------------------------------------------

pub enum Seg<'a> {
    Text(&'a str),
    Icon(&'a str),
}

/// Waveform / shape glyph inside `r`.
pub fn glyph(painter: &Painter, r: Rect, name: &str, color: Color32) {
    let pts: Vec<(f32, f32)> = match name {
        "sine" => (0..=32).map(|i| {
            let x = i as f32 / 32.0;
            (x, 0.5 - 0.5 * (x * core::f32::consts::TAU).sin())
        })
        .collect(),
        "tri" => vec![(0.0, 0.5), (0.25, 0.0), (0.75, 1.0), (1.0, 0.5)],
        "saw" => vec![(0.0, 1.0), (0.0, 0.0), (0.5, 1.0), (0.5, 0.0), (1.0, 1.0)],
        "pulse" => vec![(0.0, 1.0), (0.0, 0.0), (0.28, 0.0), (0.28, 1.0), (1.0, 1.0), (1.0, 0.0)],
        "square" => vec![(0.0, 1.0), (0.0, 0.0), (0.5, 0.0), (0.5, 1.0), (1.0, 1.0), (1.0, 0.0)],
        "noise" => [0.5, 0.1, 0.8, 0.3, 0.95, 0.2, 0.65, 0.0, 0.75, 0.4, 1.0, 0.25, 0.55]
            .iter()
            .enumerate()
            .map(|(i, &y)| (i as f32 / 12.0, y))
            .collect(),
        "s&h" => {
            let h = [0.3, 0.85, 0.1, 0.6, 0.95];
            let mut v = vec![];
            for (i, &y) in h.iter().enumerate() {
                v.push((i as f32 / 5.0, y));
                v.push(((i + 1) as f32 / 5.0, y));
            }
            v
        }
        _ => (0..=32)
            .map(|i| {
                let x = i as f32 / 32.0;
                let t = x * core::f32::consts::TAU;
                (x, 0.5 - 0.3 * (t * 1.3).sin() - 0.18 * (t * 2.7 + 1.0).sin())
            })
            .collect(),
    };
    let inner = Rect::from_center_size(r.center(), vec2(16.0, 9.0));
    let pts = pts.into_iter().map(|(x, y)| pos2(inner.left() + x * inner.width(), inner.top() + y * inner.height())).collect();
    painter.add(Shape::line(pts, Stroke::new(1.3, color)));
}

fn seg_width(ui: &Ui, s: &Seg) -> f32 {
    match s {
        Seg::Icon(_) => 26.0,
        Seg::Text(t) => text_width(ui.ctx(), t, label_font()) + 14.0,
    }
}

/// Adjoining keys; the selected key is filled. `base` is the plain value of the first item.
pub fn segmented(ui: &mut Ui, p: &IntParam, setter: &ParamSetter, items: &[Seg], base: i32, caption: Option<&str>) -> Response {
    let pl = pal();
    let cap_w = caption.map(|c| text_width(ui.ctx(), c, small_font()) + 8.0).unwrap_or(0.0);
    let widths: Vec<f32> = items.iter().map(|s| seg_width(ui, s)).collect();
    let total: f32 = widths.iter().sum::<f32>() + cap_w;
    let (rect, resp) = ui.allocate_exact_size(vec2(total, LINE_H), Sense::click());
    let painter = ui.painter_at(rect.expand(4.0));
    if let Some(c) = caption {
        put_text(&painter, pos2(rect.left(), rect.center().y), Align2::LEFT_CENTER, c, small_font(), pl.ink_soft);
    }
    let keys = Rect::from_min_max(pos2(rect.left() + cap_w, rect.top()), rect.max);
    let cur = p.modulated_plain_value() - base;
    let mut x = keys.left();
    let mut cells = Vec::with_capacity(items.len());
    for w in &widths {
        cells.push(Rect::from_min_size(pos2(x, keys.top()), vec2(*w, LINE_H)));
        x += w;
    }
    shadow(&painter, keys, 3.0, 1, 3);
    gradient_rrect(&painter, keys, 3.0, pl.key_hi, pl.key_lo);
    for (i, (cell, item)) in cells.iter().zip(items).enumerate() {
        let sel = i as i32 == cur;
        if sel {
            let cr = CornerRadius {
                nw: if i == 0 { 3 } else { 0 },
                sw: if i == 0 { 3 } else { 0 },
                ne: if i + 1 == items.len() { 3 } else { 0 },
                se: if i + 1 == items.len() { 3 } else { 0 },
            };
            painter.rect_filled(*cell, cr, pl.key_sel);
            // Pressed in: a shade along the top inner edge.
            painter.line_segment(
                [pos2(cell.left() + 1.0, cell.top() + 1.5), pos2(cell.right() - 1.0, cell.top() + 1.5)],
                Stroke::new(1.0, Color32::from_black_alpha(if is_dark() { 40 } else { 90 })),
            );
        } else if i > 0 && i as i32 != cur + 1 {
            painter.line_segment([pos2(cell.left(), cell.top() + 5.0), pos2(cell.left(), cell.bottom() - 5.0)], Stroke::new(1.0, pl.key_edge));
        }
        let col = if sel { pl.key_sel_fg } else { pl.ink };
        match item {
            Seg::Icon(name) => glyph(&painter, *cell, name, col),
            Seg::Text(t) => {
                put_text(&painter, cell.center(), Align2::CENTER_CENTER, *t, label_font(), col);
            }
        }
    }
    painter.rect_stroke(keys, CornerRadius::same(3), Stroke::new(1.0, pl.key_edge), StrokeKind::Inside);
    let mut choose = None;
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            choose = cells.iter().position(|c| c.contains(pos));
        }
    }
    if resp.hovered() {
        let s = ui.input(|i| i.smooth_scroll_delta.y);
        if s != 0.0 {
            choose = Some((cur - s.signum() as i32).clamp(0, items.len() as i32 - 1) as usize);
        }
    }
    if let Some(i) = choose {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, base + i as i32);
        setter.end_set_parameter(p);
    }
    let tip = if let Some(pos) = resp.hover_pos() {
        cells.iter().zip(items).find(|(c, _)| c.contains(pos)).map(|(_, s)| match s {
            Seg::Icon(n) => n.to_string(),
            Seg::Text(t) => t.to_string(),
        })
    } else {
        None
    };
    resp.on_hover_text(format!("{}{}", p.name(), tip.map(|t| format!(": {t}")).unwrap_or_default()))
}

/// Indicator: a solid ink dot when on, an open ring when off. The key face never changes, so
/// "off" never reads as "disabled"; the accent colour stays reserved for the primary controls.
fn lamp(painter: &Painter, c: Pos2, on: bool) {
    let pl = pal();
    if on {
        painter.circle_filled(c, 3.2, pl.ink);
    } else {
        painter.circle_filled(c, 3.2, pl.groove);
        painter.circle_stroke(c, 3.2, Stroke::new(1.0, pl.ink_soft.gamma_multiply(0.7)));
    }
}

/// Latching key with a lamp.
pub fn toggle(ui: &mut Ui, p: &BoolParam, setter: &ParamSetter, text: &str) -> Response {
    let pl = pal();
    let w = text_width(ui.ctx(), text, label_font()) + 30.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, LINE_H), Sense::click());
    let on = p.modulated_plain_value();
    if resp.clicked() {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, !on);
        setter.end_set_parameter(p);
    }
    let painter = ui.painter_at(rect.expand(4.0));
    let edge = if resp.hovered() { pl.ink_soft } else { pl.key_edge };
    raised(&painter, rect, 3.0, edge);
    lamp(&painter, pos2(rect.left() + 11.0, rect.center().y), on);
    put_text(&painter, pos2(rect.left() + 21.0, rect.center().y), Align2::LEFT_CENTER, text, label_font(), pl.ink);
    resp.on_hover_text(p.name().to_lowercase())
}

/// Section power key drawn at `rect` (in a tile heading).
pub fn power(ui: &mut Ui, p: &BoolParam, setter: &ParamSetter, rect: Rect) -> Response {
    let pl = pal();
    let resp = ui.interact(rect, ui.id().with(p.name()), Sense::click());
    let on = p.modulated_plain_value();
    if resp.clicked() {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, !on);
        setter.end_set_parameter(p);
    }
    let painter = ui.painter_at(rect.expand(4.0));
    let edge = if resp.hovered() { pl.ink_soft } else { pl.key_edge };
    raised(&painter, rect, 3.0, edge);
    lamp(&painter, pos2(rect.left() + 10.0, rect.center().y), on);
    put_text(&painter, pos2(rect.left() + 19.0, rect.center().y), Align2::LEFT_CENTER, if on { "on" } else { "off" }, small_font(), pl.ink);
    resp.on_hover_text(p.name().to_lowercase())
}

// ---------------------------------------------------------------------------------------------
// Stepper and display dropdown
// ---------------------------------------------------------------------------------------------

/// Integer stepper in a cell: ‹ value ›. Click an arrow, drag (8 px per step) or scroll.
pub fn stepper(ui: &mut Ui, p: &IntParam, setter: &ParamSetter, label: &str, w: f32) -> Response {
    let pl = pal();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, CELL_H), Sense::click_and_drag());
    let bx = Rect::from_center_size(pos2(rect.center().x, rect.top() + 20.0), vec2(w - 8.0, LINE_H));
    let cur = p.modulated_plain_value();
    let mut target = None;
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if pos.x < bx.left() + 18.0 {
                target = Some(cur - 1);
            } else if pos.x > bx.right() - 18.0 {
                target = Some(cur + 1);
            }
        }
    }
    let acc_id = resp.id.with("acc");
    if resp.drag_started() {
        ui.data_mut(|d| d.insert_temp(acc_id, 0.0f32));
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        let mut acc = ui.data(|m| m.get_temp::<f32>(acc_id).unwrap_or(0.0)) + d.x - d.y;
        let steps = (acc / 8.0).trunc() as i32;
        if steps != 0 {
            acc -= steps as f32 * 8.0;
            target = Some(cur + steps);
        }
        ui.data_mut(|m| m.insert_temp(acc_id, acc));
    }
    if resp.hovered() {
        let s = ui.input(|i| i.smooth_scroll_delta.y);
        if s != 0.0 {
            target = Some(cur + s.signum() as i32);
        }
    }
    if resp.double_clicked() {
        set_norm(setter, p, p.default_normalized_value());
    } else if let Some(t) = target {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, t);
        setter.end_set_parameter(p);
    }
    let painter = ui.painter_at(rect.expand(1.0));
    glass(&painter, bx, 3.0);
    put_text(&painter, pos2(bx.left() + 8.0, bx.center().y), Align2::CENTER_CENTER, "‹", label_font(), pl.display_dim);
    put_text(&painter, pos2(bx.right() - 8.0, bx.center().y), Align2::CENTER_CENTER, "›", label_font(), pl.display_dim);
    let active = resp.dragged();
    put_text(&painter, bx.center(), Align2::CENTER_CENTER, p.to_string(), value_font(), if active { pl.orange } else { pl.display_fg });
    cell_label(ui, rect, label, false);
    resp.on_hover_text(p.name().to_lowercase())
}

/// Small dark display showing the current choice; click opens a list. Wheel steps.
/// `items` are (plain value, label). With `label` it occupies a cell, otherwise a line.
pub fn dropdown(ui: &mut Ui, p: &IntParam, setter: &ParamSetter, label: Option<&str>, items: &[(i32, String)], w: f32) -> Response {
    let size = if label.is_some() { vec2(w, CELL_H) } else { vec2(w, LINE_H) };
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let bx = if label.is_some() { Rect::from_center_size(pos2(rect.center().x, rect.top() + 20.0), vec2(w - 8.0, LINE_H)) } else { rect };
    let cur = p.modulated_plain_value();
    let pos = items.iter().position(|(v, _)| *v == cur).unwrap_or(0);
    let popup_id = Id::new(("es1-dropdown", p.name()));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    if resp.hovered() {
        let s = ui.input(|i| i.smooth_scroll_delta.y);
        if s != 0.0 {
            let n = (pos as i32 - s.signum() as i32).clamp(0, items.len() as i32 - 1) as usize;
            setter.begin_set_parameter(p);
            setter.set_parameter(p, items[n].0);
            setter.end_set_parameter(p);
        }
    }
    let open = ui.memory(|m| m.is_popup_open(popup_id));
    display_box(&ui.painter_at(rect.expand(1.0)), bx, items.get(pos).map(|(_, s)| s.as_str()).unwrap_or(""), label_font(), open);
    if let Some(l) = label {
        cell_label(ui, rect, l, false);
    }
    let resp = resp.on_hover_text(p.name().to_lowercase());
    if let Some(i) = popup_list(ui, popup_id, &resp, w.max(120.0), items.iter().map(|(v, n)| (*v == cur, n.as_str()))) {
        setter.begin_set_parameter(p);
        setter.set_parameter(p, items[i].0);
        setter.end_set_parameter(p);
    }
    resp
}

/// A small display that is not bound to a parameter (the editor size): shows `items[cur]`, a
/// click opens the list. Returns the index the user picks.
pub fn menu(ui: &mut Ui, id: &str, items: &[String], cur: usize, size: Vec2, tip: &str) -> Option<usize> {
    let (bx, resp) = ui.allocate_exact_size(size, Sense::click());
    let popup_id = Id::new(("es1-menu", id));
    if resp.clicked() {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }
    let open = ui.memory(|m| m.is_popup_open(popup_id));
    display_box(&ui.painter_at(bx.expand(1.0)), bx, items.get(cur).map(String::as_str).unwrap_or(""), small_font(), open);
    let resp = resp.on_hover_text(tip);
    popup_list(ui, popup_id, &resp, bx.width().max(72.0), items.iter().enumerate().map(|(i, n)| (i == cur, n.as_str())))
}

/// Dark display glass with the current choice and a down arrow.
fn display_box(painter: &Painter, bx: Rect, text: &str, font: egui::FontId, open: bool) {
    let pl = pal();
    glass(painter, bx, 3.0);
    if open {
        painter.rect_stroke(bx, CornerRadius::same(3), Stroke::new(1.0, pl.orange), StrokeKind::Inside);
    }
    put_text(painter, pos2(bx.left() + 7.0, bx.center().y), Align2::LEFT_CENTER, text, font, pl.display_fg);
    let a = pos2(bx.right() - 9.0, bx.center().y + 2.0);
    painter.add(Shape::convex_polygon(vec![a + vec2(-3.5, -4.0), a + vec2(3.5, -4.0), a], pl.display_dim, Stroke::NONE));
}

/// The list under a display; returns the index of the clicked item.
fn popup_list<'a>(ui: &Ui, popup_id: Id, resp: &Response, w: f32, items: impl Iterator<Item = (bool, &'a str)>) -> Option<usize> {
    let pl = pal();
    let mut chosen = None;
    egui::popup::popup_below_widget(ui, popup_id, resp, PopupCloseBehavior::CloseOnClickOutside, |ui| {
        ui.set_min_width(w);
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let row_w = ui.available_width().max(w - 12.0);
            for (i, (sel, name)) in items.enumerate() {
                let (r, item) = ui.allocate_exact_size(vec2(row_w, 22.0), Sense::click());
                if sel {
                    ui.painter().rect_filled(r, CornerRadius::same(3), pl.key_sel);
                } else if item.hovered() {
                    ui.painter().rect_filled(r, CornerRadius::same(3), pl.key);
                }
                let col = if sel { pl.key_sel_fg } else { pl.ink };
                put_text(ui.painter(), pos2(r.left() + 8.0, r.center().y), Align2::LEFT_CENTER, name, label_font(), col);
                if item.clicked() {
                    chosen = Some(i);
                    ui.memory_mut(|m| m.close_popup());
                }
            }
        });
    });
    chosen
}

/// Two-way key pair that is not a parameter (e.g. the light/dark switch).
pub fn choice_pair(ui: &mut Ui, a: &str, b: &str, second: bool) -> Option<bool> {
    let pl = pal();
    let wa = text_width(ui.ctx(), a, small_font()) + 14.0;
    let wb = text_width(ui.ctx(), b, small_font()) + 14.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(wa + wb, 18.0), Sense::click());
    let painter = ui.painter_at(rect.expand(4.0));
    let ra = Rect::from_min_size(rect.min, vec2(wa, 18.0));
    let rb = Rect::from_min_size(pos2(rect.left() + wa, rect.top()), vec2(wb, 18.0));
    shadow(&painter, rect, 3.0, 1, 3);
    gradient_rrect(&painter, rect, 3.0, pl.key_hi, pl.key_lo);
    let (sel, other) = if second { (rb, ra) } else { (ra, rb) };
    let cr = if second { CornerRadius { ne: 3, se: 3, nw: 0, sw: 0 } } else { CornerRadius { nw: 3, sw: 3, ne: 0, se: 0 } };
    painter.rect_filled(sel, cr, pl.key_sel);
    painter.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0, pl.key_edge), StrokeKind::Inside);
    let _ = other;
    put_text(&painter, ra.center(), Align2::CENTER_CENTER, a, small_font(), if second { pl.ink } else { pl.key_sel_fg });
    put_text(&painter, rb.center(), Align2::CENTER_CENTER, b, small_font(), if second { pl.key_sel_fg } else { pl.ink });
    if resp.clicked() {
        let pos = resp.interact_pointer_pos()?;
        return Some(rb.contains(pos));
    }
    None
}
