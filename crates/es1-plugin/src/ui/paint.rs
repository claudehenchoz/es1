//! Material rendering: vertical gradients (light from above), soft cast shadows and the glass
//! display. Gradient fills are plain meshes without feathering, so every one is finished with an
//! anti-aliased outline drawn on top.

use super::theme::*;
use nih_plug_egui::egui::{epaint::Mesh, epaint::Shadow, pos2, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Translucent white. (`Color32::from_white_alpha` treats alpha as linear, which comes out far
/// brighter than intended on a light face.)
pub fn white(a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(a, a, a, a)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Outline of a rounded rectangle, clockwise from the top-left corner.
fn rrect_points(r: Rect, rad: f32) -> Vec<Pos2> {
    let rad = rad.min(r.width() * 0.5).min(r.height() * 0.5);
    let mut pts = Vec::with_capacity(28);
    let corners = [
        (pos2(r.left() + rad, r.top() + rad), PI),
        (pos2(r.right() - rad, r.top() + rad), PI + FRAC_PI_2),
        (pos2(r.right() - rad, r.bottom() - rad), 0.0),
        (pos2(r.left() + rad, r.bottom() - rad), FRAC_PI_2),
    ];
    for (c, a0) in corners {
        for k in 0..=6 {
            let a = a0 + FRAC_PI_2 * k as f32 / 6.0;
            pts.push(c + rad * nih_plug_egui::egui::vec2(a.cos(), a.sin()));
        }
    }
    pts
}

/// Fill a convex outline with a vertical gradient from `top` (at `y0`) to `bot` (at `y1`).
fn gradient_fill(painter: &Painter, pts: &[Pos2], y0: f32, y1: f32, top: Color32, bot: Color32) {
    let at = |y: f32| mix(top, bot, ((y - y0) / (y1 - y0)).clamp(0.0, 1.0));
    let mut mesh = Mesh::default();
    let c = pts.iter().fold(pos2(0.0, 0.0), |a, p| a + p.to_vec2()) / pts.len() as f32;
    mesh.colored_vertex(c, at(c.y));
    for p in pts {
        mesh.colored_vertex(*p, at(p.y));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
}

pub fn gradient_rrect(painter: &Painter, r: Rect, rad: f32, top: Color32, bot: Color32) {
    gradient_fill(painter, &rrect_points(r, rad), r.top(), r.bottom(), top, bot);
}

pub fn gradient_circle(painter: &Painter, c: Pos2, rad: f32, top: Color32, bot: Color32) {
    let pts: Vec<Pos2> = (0..48).map(|i| c + rad * nih_plug_egui::egui::Vec2::angled(TAU * i as f32 / 48.0)).collect();
    gradient_fill(painter, &pts, c.y - rad, c.y + rad, top, bot);
}

/// Soft shadow under a raised rounded rectangle.
pub fn shadow(painter: &Painter, r: Rect, rad: f32, lift: i8, blur: u8) {
    let s = Shadow { offset: [0, lift], blur, spread: 0, color: pal().shadow };
    painter.add(s.as_shape(r, CornerRadius::same(rad.round() as u8)));
}

/// A raised key or cap: shadow, lit face, outline and a sheen along the top edge.
pub fn raised(painter: &Painter, r: Rect, rad: f32, edge: Color32) {
    let pl = pal();
    shadow(painter, r, rad, 1, 3);
    gradient_rrect(painter, r, rad, pl.key_hi, pl.key_lo);
    painter.rect_stroke(r, CornerRadius::same(rad.round() as u8), Stroke::new(1.0, edge), StrokeKind::Inside);
    sheen(painter, r, rad);
}

pub fn sheen(painter: &Painter, r: Rect, rad: f32) {
    painter.line_segment([pos2(r.left() + rad, r.top() + 1.5), pos2(r.right() - rad, r.top() + 1.5)], Stroke::new(1.0, pal().sheen));
}

/// Recessed glass display: darker at the top where the bezel shades it, with a fine highlight
/// on the lower lip.
pub fn glass(painter: &Painter, r: Rect, rad: f32) {
    let pl = pal();
    let top = mix(pl.display_bg, Color32::BLACK, 0.35);
    gradient_rrect(painter, r, rad, top, pl.display_bg);
    painter.rect_stroke(r, CornerRadius::same(rad.round() as u8), Stroke::new(1.0, mix(pl.display_bg, Color32::BLACK, 0.5)), StrokeKind::Inside);
    let lip = if is_dark() { white(18) } else { white(150) };
    painter.line_segment([pos2(r.left() + rad, r.bottom() + 0.5), pos2(r.right() - rad, r.bottom() + 0.5)], Stroke::new(1.0, lip));
}

/// Recessed slot for faders and sliders.
pub fn groove(painter: &Painter, r: Rect) {
    let pl = pal();
    let rad = r.width().min(r.height()) * 0.5;
    painter.rect_filled(r, CornerRadius::same(rad.round() as u8), pl.groove);
    let lip = if is_dark() { white(14) } else { white(170) };
    if r.width() < r.height() {
        painter.line_segment([pos2(r.right() + 0.5, r.top() + rad), pos2(r.right() + 0.5, r.bottom() - rad)], Stroke::new(1.0, lip));
    } else {
        painter.line_segment([pos2(r.left() + rad, r.bottom() + 0.5), pos2(r.right() - rad, r.bottom() + 0.5)], Stroke::new(1.0, lip));
    }
}

/// Brushed aluminium plate: a lit gradient crossed by fine horizontal grain, ending in a hard
/// edge that casts a shadow onto the body below.
pub fn brushed_plate(painter: &Painter, r: Rect) {
    let pl = pal();
    shadow(painter, r, 0.0, 3, 10);
    gradient_rrect(painter, r, 0.0, pl.plate_hi, pl.plate_lo);
    let mut seed = 0x9E37_79B9u32;
    let mut y = r.top() + 0.5;
    while y < r.bottom() - 1.0 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let a = (seed >> 24) as u8 / 64;
        let c = if seed & 1 == 0 { white(a * if is_dark() { 2 } else { 5 }) } else { Color32::from_black_alpha(a * 2) };
        painter.line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0, c));
        y += 1.0;
    }
    painter.line_segment([pos2(r.left(), r.bottom() - 0.5), pos2(r.right(), r.bottom() - 0.5)], Stroke::new(1.0, pl.plate_edge));
}
