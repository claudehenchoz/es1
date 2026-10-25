//! Braun / Dieter Rams palette and type scale.
//!
//! Reference points: the warm light-grey bodies of the T 1000 / RT 20, the dark grey legends,
//! a single signal orange (as on the ET 66 "=" key and SK 4 controls) and the green "on"
//! indicator. Nothing decorative: hierarchy comes from spacing and weight, not ornament.

use nih_plug_egui::egui::{self, Color32, FontFamily, FontId, Stroke, Visuals};

pub const PANEL: Color32 = Color32::from_rgb(0xE9, 0xE6, 0xDF);
pub const PANEL_DARK: Color32 = Color32::from_rgb(0xD9, 0xD5, 0xCC);
pub const RULE: Color32 = Color32::from_rgb(0xB9, 0xB4, 0xAA);
pub const INK: Color32 = Color32::from_rgb(0x2B, 0x2A, 0x28);
pub const INK_SOFT: Color32 = Color32::from_rgb(0x6E, 0x6A, 0x63);
pub const KNOB: Color32 = Color32::from_rgb(0xF7, 0xF5, 0xF0);
pub const KNOB_EDGE: Color32 = Color32::from_rgb(0xA8, 0xA3, 0x99);
pub const KNOB_DARK: Color32 = Color32::from_rgb(0x34, 0x33, 0x31);
pub const ORANGE: Color32 = Color32::from_rgb(0xE8, 0x5D, 0x1A);
pub const GREEN: Color32 = Color32::from_rgb(0x3F, 0xA3, 0x4D);
pub const DISPLAY_BG: Color32 = Color32::from_rgb(0x1E, 0x1F, 0x1D);
pub const DISPLAY_FG: Color32 = Color32::from_rgb(0xF1, 0xEE, 0xE6);

/// Grid unit: every dimension in the layout is a multiple of this.
pub const U: f32 = 8.0;

pub fn label_font() -> FontId {
    FontId::new(10.0, FontFamily::Proportional)
}

pub fn value_font() -> FontId {
    FontId::new(10.0, FontFamily::Monospace)
}

pub fn section_font() -> FontId {
    FontId::new(11.0, FontFamily::Proportional)
}

pub fn title_font() -> FontId {
    FontId::new(18.0, FontFamily::Proportional)
}

pub fn apply(ctx: &egui::Context) {
    let mut v = Visuals::light();
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = DISPLAY_BG;
    v.override_text_color = Some(INK);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, RULE);
    v.widgets.inactive.bg_fill = KNOB;
    v.widgets.inactive.weak_bg_fill = KNOB;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, INK);
    v.widgets.hovered.bg_fill = KNOB;
    v.widgets.hovered.weak_bg_fill = PANEL_DARK;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, INK);
    v.widgets.active.bg_fill = PANEL_DARK;
    v.widgets.active.fg_stroke = Stroke::new(1.0, ORANGE);
    v.selection.bg_fill = ORANGE;
    v.selection.stroke = Stroke::new(1.0, DISPLAY_FG);
    ctx.set_visuals(v);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(U, U * 0.5);
    style.spacing.button_padding = egui::vec2(U, U * 0.5);
    ctx.set_style(style);
}
