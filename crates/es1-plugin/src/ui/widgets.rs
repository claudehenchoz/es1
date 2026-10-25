//! Rams-style controls drawn directly with egui's painter, bound to nih-plug parameters.

use super::theme::*;
use nih_plug::prelude::{Param, ParamSetter};
use nih_plug_egui::egui::{self, pos2, vec2, Align2, CornerRadius, Response, Sense, Stroke, StrokeKind, Ui};

/// Rotary range: 300 degrees, with the gap at the bottom.
const ARC: f32 = 300.0_f32 * core::f32::consts::PI / 180.0;

fn drag_normalized<P: Param>(ui: &Ui, resp: &Response, param: &P, setter: &ParamSetter, px_per_full: f32) {
    if resp.drag_started() {
        setter.begin_set_parameter(param);
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let delta = -resp.drag_delta().y / if fine { px_per_full * 8.0 } else { px_per_full };
        let v = (param.modulated_normalized_value() + delta).clamp(0.0, 1.0);
        setter.set_parameter_normalized(param, v);
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(param);
    }
    if resp.double_clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, param.default_normalized_value());
        setter.end_set_parameter(param);
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            setter.begin_set_parameter(param);
            let steps = param.step_count().map(|s| 1.0 / s as f32).unwrap_or(1.0 / 254.0);
            let v = (param.modulated_normalized_value() + steps * scroll.signum()).clamp(0.0, 1.0);
            setter.set_parameter_normalized(param, v);
            setter.end_set_parameter(param);
        }
    }
}

/// A round knob with a flat cap and an index line, label underneath. `big` uses the
/// dark cap reserved for primary controls (cutoff, master level), the T3 / TP1 idiom.
pub fn knob<P: Param>(ui: &mut Ui, param: &P, setter: &ParamSetter, label: &str, big: bool) -> Response {
    let d = if big { 5.0 * U } else { 4.0 * U };
    let w = 6.5 * U;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, d + 3.0 * U), Sense::click_and_drag());
    drag_normalized(ui, &resp, param, setter, 160.0);
    let p = ui.painter_at(rect.expand(2.0));
    let c = pos2(rect.center().x, rect.top() + d * 0.5 + 2.0);
    let r = d * 0.5;
    let v = param.modulated_normalized_value();

    // Scale ticks: 11 short marks, the Rams "information, not decoration" ring.
    for i in 0..=10 {
        let a = -ARC * 0.5 + ARC * (i as f32 / 10.0);
        let dir = vec2(a.sin(), -a.cos());
        let (r0, r1) = if i == 0 || i == 10 || i == 5 { (r + 2.0, r + 5.0) } else { (r + 2.5, r + 4.0) };
        p.line_segment([c + dir * r0, c + dir * r1], Stroke::new(1.0, INK_SOFT));
    }
    let (cap, line) = if big { (KNOB_DARK, KNOB) } else { (KNOB, INK) };
    p.circle_filled(c + vec2(0.0, 1.0), r, PANEL_DARK);
    p.circle(c, r, cap, Stroke::new(1.0, KNOB_EDGE));
    let a = -ARC * 0.5 + ARC * v;
    let dir = vec2(a.sin(), -a.cos());
    let active = resp.dragged() || resp.hovered();
    p.line_segment([c + dir * (r * 0.25), c + dir * (r - 2.0)], Stroke::new(2.0, if resp.dragged() { ORANGE } else { line }));

    let text = if active { param.to_string() } else { label.to_string() };
    let font = if active { value_font() } else { label_font() };
    p.text(pos2(rect.center().x, rect.bottom() - U), Align2::CENTER_CENTER, text, font, if active { ORANGE } else { INK });
    resp.on_hover_text(param.name())
}

/// A two-state switch: a small rectangular key with a green indicator when on.
pub fn switch<P: Param>(ui: &mut Ui, param: &P, setter: &ParamSetter, label: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(6.0 * U, 4.0 * U), Sense::click());
    let on = param.modulated_normalized_value() > 0.5;
    if resp.clicked() {
        setter.begin_set_parameter(param);
        setter.set_parameter_normalized(param, if on { 0.0 } else { 1.0 });
        setter.end_set_parameter(param);
    }
    let p = ui.painter_at(rect);
    let key = egui::Rect::from_center_size(pos2(rect.center().x, rect.top() + 1.25 * U), vec2(3.5 * U, 1.75 * U));
    p.rect(key, CornerRadius::same(2), if on { KNOB } else { PANEL_DARK }, Stroke::new(1.0, KNOB_EDGE), StrokeKind::Inside);
    p.circle_filled(pos2(key.right() - 0.6 * U, key.center().y), 2.5, if on { GREEN } else { RULE });
    p.text(pos2(rect.center().x, rect.bottom() - U * 0.9), Align2::CENTER_CENTER, label, label_font(), INK);
    resp.on_hover_text(param.name())
}

/// A selector for enumerated parameters: legends in a column, the active one marked with an
/// orange bar. Click to choose, or drag/scroll like a knob.
pub fn selector<P: Param>(ui: &mut Ui, param: &P, setter: &ParamSetter, label: &str, options: &[&str]) -> Response {
    let n = options.len().max(1);
    let row = 1.5 * U;
    let w = 8.0 * U;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, row * n as f32 + 2.0 * U), Sense::click_and_drag());
    let p = ui.painter_at(rect);
    let steps = param.step_count().unwrap_or(n - 1).max(1);
    let cur = (param.modulated_normalized_value() * steps as f32).round() as usize;
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let i = (((pos.y - rect.top()) / row).floor() as usize).min(n - 1);
            setter.begin_set_parameter(param);
            setter.set_parameter_normalized(param, i as f32 / steps as f32);
            setter.end_set_parameter(param);
        }
    } else {
        drag_normalized(ui, &resp, param, setter, 120.0);
    }
    for (i, o) in options.iter().enumerate() {
        let y = rect.top() + row * (i as f32 + 0.5);
        let sel = i == cur;
        if sel {
            p.rect_filled(egui::Rect::from_min_size(pos2(rect.left(), y - 4.0), vec2(3.0, 8.0)), CornerRadius::ZERO, ORANGE);
        }
        p.text(pos2(rect.left() + U, y), Align2::LEFT_CENTER, *o, label_font(), if sel { INK } else { INK_SOFT });
    }
    p.text(pos2(rect.left() + U, rect.bottom() - U * 0.9), Align2::LEFT_CENTER, label, label_font(), INK);
    resp.on_hover_text(param.name())
}

/// Section header: small caps-style label with a hairline rule, as on Braun front panels.
pub fn section(ui: &mut Ui, title: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0 * U), Sense::hover());
    let p = ui.painter_at(rect);
    let g = p.layout_no_wrap(title.to_lowercase(), section_font(), INK);
    let tw = g.size().x;
    p.galley(pos2(rect.left(), rect.center().y - g.size().y * 0.5), g, INK);
    p.line_segment(
        [pos2(rect.left() + tw + U, rect.center().y), pos2(rect.right(), rect.center().y)],
        Stroke::new(1.0, RULE),
    );
}
