// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Braun / Dieter Rams palettes, type and the persisted light/dark preference.
//!
//! Light: the warm grey bodies of the T 1000 / RT 20. Dark: the black finishes of the ET 66 and
//! the Regie series. Both share a single signal orange (used only for selection and values in
//! motion) and the green "on" lamp.

use nih_plug_egui::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, Visuals};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct Palette {
    /// Body colour behind the tiles (top of its gradient in `body_hi`).
    pub body: Color32,
    pub body_hi: Color32,
    /// Section tile face; the face is lit from above, from `tile_hi` down to `tile`.
    pub tile: Color32,
    pub tile_hi: Color32,
    /// Hairline highlight along the top edge of raised parts.
    pub sheen: Color32,
    /// Cast shadow under raised parts.
    pub shadow: Color32,
    /// Fader and slider slot.
    pub groove: Color32,
    /// Brushed top plate carrying the header.
    pub plate_hi: Color32,
    pub plate_lo: Color32,
    pub plate_edge: Color32,
    pub tile_edge: Color32,
    pub rule: Color32,
    pub ink: Color32,
    pub ink_soft: Color32,
    /// Knob skirt gradient (lit top, shaded bottom); the dished cap reverses it.
    pub knob_hi: Color32,
    pub knob_lo: Color32,
    pub knob_edge: Color32,
    pub knob_line: Color32,
    /// Primary (cutoff, volume) knob: inverted cap, from `main_hi` to `main_lo`.
    pub knob_main_line: Color32,
    pub main_hi: Color32,
    pub main_lo: Color32,
    pub key: Color32,
    pub key_hi: Color32,
    pub key_lo: Color32,
    pub key_edge: Color32,
    /// Selected segment: filled with ink, glyph in `key_sel_fg`.
    pub key_sel: Color32,
    pub key_sel_fg: Color32,
    pub orange: Color32,
    pub green: Color32,
    pub display_bg: Color32,
    pub display_fg: Color32,
    pub display_dim: Color32,
}

pub static LIGHT: Palette = Palette {
    body: Color32::from_rgb(0xD6, 0xD3, 0xCC),
    body_hi: Color32::from_rgb(0xDE, 0xDB, 0xD4),
    tile: Color32::from_rgb(0xE9, 0xE6, 0xE0),
    tile_hi: Color32::from_rgb(0xF0, 0xEE, 0xE9),
    sheen: Color32::from_rgba_premultiplied(0xC0, 0xC0, 0xC0, 0xC0),
    shadow: Color32::from_rgba_premultiplied(0x30, 0x2A, 0x20, 0x30),
    groove: Color32::from_rgb(0xC4, 0xC0, 0xB7),
    plate_hi: Color32::from_rgb(0xF1, 0xF0, 0xEC),
    plate_lo: Color32::from_rgb(0xD2, 0xD0, 0xCA),
    plate_edge: Color32::from_rgb(0x9C, 0x98, 0x90),
    tile_edge: Color32::from_rgb(0xD2, 0xCE, 0xC6),
    rule: Color32::from_rgb(0xBD, 0xB8, 0xAE),
    ink: Color32::from_rgb(0x26, 0x25, 0x23),
    ink_soft: Color32::from_rgb(0x74, 0x70, 0x69),
    knob_hi: Color32::from_rgb(0xFF, 0xFE, 0xFB),
    knob_lo: Color32::from_rgb(0xD9, 0xD5, 0xCC),
    knob_edge: Color32::from_rgb(0xAE, 0xA9, 0x9F),
    knob_line: Color32::from_rgb(0x26, 0x25, 0x23),
    knob_main_line: Color32::from_rgb(0xF3, 0xF0, 0xE9),
    main_hi: Color32::from_rgb(0x4A, 0x48, 0x45),
    main_lo: Color32::from_rgb(0x1C, 0x1B, 0x1A),
    key: Color32::from_rgb(0xF6, 0xF4, 0xEF),
    key_hi: Color32::from_rgb(0xFC, 0xFB, 0xF8),
    key_lo: Color32::from_rgb(0xEB, 0xE8, 0xE2),
    key_edge: Color32::from_rgb(0xB8, 0xB3, 0xA9),
    key_sel: Color32::from_rgb(0x2E, 0x2D, 0x2B),
    key_sel_fg: Color32::from_rgb(0xF6, 0xF4, 0xEF),
    orange: Color32::from_rgb(0xE5, 0x5B, 0x17),
    green: Color32::from_rgb(0x3D, 0xA9, 0x4C),
    display_bg: Color32::from_rgb(0x1E, 0x1F, 0x1D),
    display_fg: Color32::from_rgb(0xF1, 0xEE, 0xE6),
    display_dim: Color32::from_rgb(0x8A, 0x87, 0x80),
};

pub static DARK: Palette = Palette {
    body: Color32::from_rgb(0x12, 0x12, 0x11),
    body_hi: Color32::from_rgb(0x1A, 0x1A, 0x19),
    tile: Color32::from_rgb(0x20, 0x20, 0x1E),
    tile_hi: Color32::from_rgb(0x27, 0x27, 0x25),
    sheen: Color32::from_rgba_premultiplied(0x16, 0x16, 0x16, 0x16),
    shadow: Color32::from_rgba_premultiplied(0x00, 0x00, 0x00, 0x80),
    groove: Color32::from_rgb(0x0E, 0x0E, 0x0D),
    plate_hi: Color32::from_rgb(0x30, 0x30, 0x2E),
    plate_lo: Color32::from_rgb(0x1D, 0x1D, 0x1C),
    plate_edge: Color32::from_rgb(0x03, 0x03, 0x03),
    tile_edge: Color32::from_rgb(0x2E, 0x2E, 0x2B),
    rule: Color32::from_rgb(0x45, 0x44, 0x40),
    ink: Color32::from_rgb(0xEC, 0xE9, 0xE2),
    ink_soft: Color32::from_rgb(0x8E, 0x8A, 0x83),
    knob_hi: Color32::from_rgb(0x4C, 0x4B, 0x47),
    knob_lo: Color32::from_rgb(0x2A, 0x29, 0x27),
    knob_edge: Color32::from_rgb(0x12, 0x12, 0x11),
    knob_line: Color32::from_rgb(0xEC, 0xE9, 0xE2),
    knob_main_line: Color32::from_rgb(0x22, 0x22, 0x20),
    main_hi: Color32::from_rgb(0xF4, 0xF1, 0xEA),
    main_lo: Color32::from_rgb(0xBF, 0xBB, 0xB2),
    key: Color32::from_rgb(0x2C, 0x2C, 0x29),
    key_hi: Color32::from_rgb(0x35, 0x35, 0x32),
    key_lo: Color32::from_rgb(0x29, 0x29, 0x26),
    key_edge: Color32::from_rgb(0x11, 0x11, 0x10),
    key_sel: Color32::from_rgb(0xE6, 0xE2, 0xDA),
    key_sel_fg: Color32::from_rgb(0x1E, 0x1E, 0x1C),
    orange: Color32::from_rgb(0xEE, 0x67, 0x22),
    green: Color32::from_rgb(0x52, 0xC2, 0x5F),
    display_bg: Color32::from_rgb(0x0D, 0x0D, 0x0C),
    display_fg: Color32::from_rgb(0xF1, 0xEE, 0xE6),
    display_dim: Color32::from_rgb(0x7E, 0x7B, 0x74),
};

static IS_DARK: AtomicBool = AtomicBool::new(false);

pub fn pal() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn is_dark() -> bool {
    IS_DARK.load(Ordering::Relaxed)
}

/// Grid unit: every dimension in the layout is a multiple of this.
pub const U: f32 = 8.0;

pub fn label_font() -> FontId {
    FontId::new(11.0, FontFamily::Proportional)
}

pub fn small_font() -> FontId {
    FontId::new(10.0, FontFamily::Proportional)
}

pub fn value_font() -> FontId {
    FontId::new(10.5, FontFamily::Monospace)
}

pub fn heading_font() -> FontId {
    FontId::new(15.0, FontFamily::Name("heading".into()))
}

pub fn title_font() -> FontId {
    FontId::new(22.0, FontFamily::Name("heading".into()))
}

/// The header wordmark: sized so "ES 1" and its subtitle fill the space left of the display.
pub fn wordmark_font() -> FontId {
    FontId::new(36.0, FontFamily::Name("heading".into()))
}

// ---- persisted preference -------------------------------------------------------------------

fn settings_path() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
    Some(base.join("ES 1").join("settings.txt"))
}

/// Read the global preference (shared by all instances).
pub fn load_settings() {
    if let Some(s) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        let dark = s.lines().any(|l| l.trim() == "theme=dark");
        IS_DARK.store(dark, Ordering::Relaxed);
    }
}

pub fn set_dark(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    apply(ctx);
    if let Some(p) = settings_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let _ = std::fs::write(p, if dark { "theme=dark\n" } else { "theme=light\n" });
    }
}

// ---- fonts / visuals --------------------------------------------------------------------------

/// Prefer a neo-grotesk from the system (Segoe UI on Windows, Helvetica Neue / Arial elsewhere),
/// falling back to egui's built-in font.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let try_load = |paths: &[&str]| paths.iter().find_map(|p| std::fs::read(p).ok());
    let regular = try_load(&[
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ]);
    let heading = try_load(&[
        "C:\\Windows\\Fonts\\seguisb.ttf",
        "C:\\Windows\\Fonts\\segoeuib.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    ]);
    if let Some(r) = regular {
        defs.font_data.insert("es1-regular".into(), Arc::new(FontData::from_owned(r)));
        defs.families.entry(FontFamily::Proportional).or_default().insert(0, "es1-regular".into());
    }
    let mut heading_fam = Vec::new();
    if let Some(h) = heading {
        defs.font_data.insert("es1-heading".into(), Arc::new(FontData::from_owned(h)));
        heading_fam.push("es1-heading".to_string());
    }
    heading_fam.extend(defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    defs.families.insert(FontFamily::Name("heading".into()), heading_fam);
    ctx.set_fonts(defs);
}

pub fn apply(ctx: &egui::Context) {
    let p = pal();
    let mut v = if is_dark() { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.body;
    v.window_fill = p.tile;
    v.window_stroke = Stroke::new(1.0, p.tile_edge);
    v.extreme_bg_color = p.display_bg;
    v.override_text_color = Some(p.ink);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.rule);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active] {
        w.bg_fill = p.key;
        w.weak_bg_fill = p.key;
        w.bg_stroke = Stroke::new(1.0, p.key_edge);
        w.fg_stroke = Stroke::new(1.0, p.ink);
    }
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.ink_soft);
    v.selection.bg_fill = p.key_sel;
    v.selection.stroke = Stroke::new(1.0, p.key_sel_fg);
    ctx.set_visuals(v);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(U, U * 0.5);
    style.spacing.button_padding = egui::vec2(U, U * 0.375);
    ctx.set_style(style);
}
