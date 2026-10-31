// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Braun / Dieter Rams palettes, type and the persisted light/dark preference.
//!
//! Light: the warm grey bodies of the T 1000 / RT 20. Dark: the black finishes of the ET 66 and
//! the Regie series. Both are dark ink on a light face (or the reverse) with one accent, signal
//! orange, kept for the primary controls, the selection and values in motion.

use nih_plug_egui::egui::text::LayoutJob;
use nih_plug_egui::egui::{self, vec2, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Painter, Pos2, Rect, Stroke, TextFormat, Visuals};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    FontId::new(10.5, FontFamily::Proportional)
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

/// Editor scale presets, in percent of the design size.
pub const SCALES: [u32; 10] = [50, 60, 70, 75, 80, 90, 100, 125, 150, 200];

static SCALE: AtomicU32 = AtomicU32::new(100);

/// The editor scale in percent (one of [`SCALES`]).
pub fn scale_pct() -> u32 {
    SCALE.load(Ordering::Relaxed)
}

/// Read the global preferences (shared by all instances on this computer).
pub fn load_settings() {
    if let Some(s) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        for l in s.lines().map(str::trim) {
            match l.split_once('=') {
                Some(("theme", v)) => IS_DARK.store(v == "dark", Ordering::Relaxed),
                Some(("scale", v)) => {
                    if let Some(pct) = v.parse().ok().filter(|p| SCALES.contains(p)) {
                        SCALE.store(pct, Ordering::Relaxed);
                    }
                }
                _ => {}
            }
        }
    }
}

fn save_settings() {
    if let Some(p) = settings_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap());
        let theme = if is_dark() { "dark" } else { "light" };
        let _ = std::fs::write(p, format!("theme={theme}\nscale={}\n", scale_pct()));
    }
}

pub fn set_dark(ctx: &egui::Context, dark: bool) {
    IS_DARK.store(dark, Ordering::Relaxed);
    apply(ctx);
    save_settings();
}

pub fn set_scale(pct: u32) {
    SCALE.store(pct, Ordering::Relaxed);
    save_settings();
}

// ---- fonts / visuals --------------------------------------------------------------------------

/// Aileron by Sora Sagano (CC0, see `assets/fonts/`), embedded so the panel looks the same on
/// every system. One family in two weights: Regular for labels and values (its figures are
/// tabular, so changing numbers do not shift), SemiBold for headings and the wordmark.
static AILERON_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Aileron-Regular.otf");
static AILERON_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Aileron-SemiBold.otf");

pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::empty();
    defs.font_data.insert("regular".into(), Arc::new(FontData::from_static(AILERON_REGULAR)));
    defs.font_data.insert("semibold".into(), Arc::new(FontData::from_static(AILERON_SEMIBOLD)));
    defs.families.insert(FontFamily::Proportional, vec!["regular".into()]);
    defs.families.insert(FontFamily::Monospace, vec!["regular".into()]);
    defs.families.insert(FontFamily::Name("heading".into()), vec!["semibold".into(), "regular".into()]);
    ctx.set_fonts(defs);
}

/// Extra space between letters: tiny labels are tracked out, larger type is set as drawn.
pub fn tracking(font: &FontId) -> f32 {
    if font.size <= 10.0 {
        0.8
    } else if font.size <= 11.0 {
        0.2
    } else {
        0.0
    }
}

/// A single line of text in `font`, letterspaced by [`tracking`].
pub fn spaced(text: impl Into<String>, font: FontId, color: Color32) -> LayoutJob {
    let extra_letter_spacing = tracking(&font);
    LayoutJob::single_section(text.into(), TextFormat { font_id: font, color, extra_letter_spacing, ..Default::default() })
}

/// Width of `text` as [`put_text`] draws it.
pub fn text_width(ctx: &egui::Context, text: &str, font: FontId) -> f32 {
    let sp = tracking(&font);
    ctx.fonts(|f| f.layout_job(spaced(text, font, Color32::WHITE)).size().x) - sp
}

/// `Painter::text` with letterspacing. The spacing egui adds after the last letter is left out
/// when the text is aligned, so centred and right-aligned labels sit where they should.
pub fn put_text(painter: &Painter, pos: Pos2, anchor: Align2, text: impl ToString, font: FontId, color: Color32) -> Rect {
    let sp = tracking(&font);
    let galley = painter.layout_job(spaced(text.to_string(), font, color));
    let rect = anchor.anchor_size(pos, galley.size() - vec2(sp, 0.0));
    painter.galley(rect.min, galley, color);
    rect
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aileron_covers_the_panel_text() {
        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let _ = ctx.run(Default::default(), |_| {});
        let text = "abcdefghijklmnopqrstuvwxyz ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789 .,:;-+/&()%'\"‹›×±−·≈…° äöüßéèàçñ";
        for font in [label_font(), small_font(), value_font(), heading_font(), wordmark_font()] {
            ctx.fonts(|f| {
                for c in text.chars() {
                    assert!(f.has_glyph(&font, c), "{c:?} missing in {font:?}");
                }
            });
        }
    }
}
