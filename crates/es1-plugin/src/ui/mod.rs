//! ES 1 editor. "Weniger, aber besser": a warm grey panel, dark legends, one orange accent,
//! controls on an 8 px grid, grouped exactly by function.

mod theme;
mod widgets;

use crate::params::{Es1Params, HostParam, StoredBank, NO_PROGRAM};
use crate::Shared;
use es1_dsp::params::Kind;
use es1_presets::{factory_bank, load_path, parse_sy1, write_sy1, Bank, Sy1Patch};
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, pos2, vec2, Align2, CornerRadius, RichText, Sense, Stroke, StrokeKind};
use nih_plug_egui::{create_egui_editor, EguiState};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use theme::*;
use widgets::*;

pub const WIDTH: u32 = 1232;
pub const HEIGHT: u32 = 744;

struct UiState {
    banks: Vec<Bank>,
    browser: bool,
    bank_sel: usize,
    message: Option<String>,
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
        UiState { banks: Vec::new(), browser: false, bank_sel: 0, message: None },
        move |ctx, st| {
            theme::apply(ctx);
            st.banks = all_banks(&p2);
            st.bank_sel = p2.patch.read().bank.min(st.banks.len().saturating_sub(1));
        },
        move |ctx, setter, st| {
            egui::CentralPanel::default().frame(egui::Frame::new().fill(PANEL).inner_margin(2.0 * U)).show(ctx, |ui| {
                header(ui, &params, &shared, setter, st);
                ui.add_space(U);
                if st.browser {
                    browser(ui, &params, &shared, setter, st);
                } else {
                    panel(ui, &params, setter);
                }
            });
        },
    )
}

// ---------------------------------------------------------------------------------------------
// Patch handling
// ---------------------------------------------------------------------------------------------

fn load_patch(params: &Es1Params, shared: &Shared, setter: &ParamSetter, patch: &Sy1Patch, bank: usize, program: usize) {
    let v = patch.to_values();
    for (hp, def) in params.list.iter().zip(es1_dsp::PARAMS) {
        let norm = hp.normalized_for(def, v[def.sy1 as usize]);
        macro_rules! set {
            ($p:expr) => {{
                setter.begin_set_parameter($p);
                setter.set_parameter_normalized($p, norm);
                setter.end_set_parameter($p);
            }};
        }
        match hp {
            HostParam::F(p) => set!(p),
            HostParam::I(p) => set!(p),
            HostParam::B(p) => set!(p),
        }
    }
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

fn step_program(params: &Es1Params, shared: &Shared, setter: &ParamSetter, st: &mut UiState, delta: i32) {
    let (b, p) = {
        let s = params.patch.read();
        (s.bank.min(st.banks.len() - 1), s.program)
    };
    let n = st.banks[b].patches.len();
    if n == 0 {
        return;
    }
    let p = if p == NO_PROGRAM { if delta > 0 { 0 } else { n - 1 } } else { ((p as i32 + delta).rem_euclid(n as i32)) as usize };
    let patch = st.banks[b].patches[p].clone();
    load_patch(params, shared, setter, &patch, b, p);
}

fn import(params: &Es1Params, st: &mut UiState) {
    let Some(path) = rfd::FileDialog::new().add_filter("Synth1 patches", &["sy1", "zip"]).pick_file() else { return };
    match load_path(&path) {
        Ok(banks) => {
            let mut stored = params.banks.write();
            let mut count = 0;
            for b in banks {
                count += b.patches.len();
                stored.push(StoredBank { name: b.name.clone(), patches: b.patches.iter().map(write_sy1).collect() });
                st.banks.push(b);
            }
            st.bank_sel = st.banks.len() - 1;
            st.message = Some(format!("imported {count} patches"));
        }
        Err(e) => st.message = Some(format!("import failed: {e}")),
    }
}

fn save(params: &Es1Params, st: &mut UiState) {
    let name = params.patch.read().name.clone();
    let Some(path) = rfd::FileDialog::new().add_filter("Synth1 patch", &["sy1"]).set_file_name(format!("{name}.sy1")).save_file() else {
        return;
    };
    let p = Sy1Patch::from_values(&name, &current_values(params));
    st.message = Some(match std::fs::write(&path, write_sy1(&p)) {
        Ok(()) => "saved".into(),
        Err(e) => format!("save failed: {e}"),
    });
}

// ---------------------------------------------------------------------------------------------
// Header: wordmark, display, master
// ---------------------------------------------------------------------------------------------

fn header(ui: &mut egui::Ui, params: &Es1Params, shared: &Shared, setter: &ParamSetter, st: &mut UiState) {
    ui.horizontal(|ui| {
        // Wordmark.
        let (r, _) = ui.allocate_exact_size(vec2(18.0 * U, 7.0 * U), Sense::hover());
        let p = ui.painter_at(r);
        p.text(pos2(r.left(), r.top() + 2.5 * U), Align2::LEFT_CENTER, "ES 1", title_font(), INK);
        p.text(pos2(r.left(), r.top() + 5.0 * U), Align2::LEFT_CENTER, "elektronischer synthesizer", label_font(), INK_SOFT);

        // Display.
        let (r, _) = ui.allocate_exact_size(vec2(44.0 * U, 7.0 * U), Sense::hover());
        let p = ui.painter_at(r);
        p.rect(r, CornerRadius::same(3), DISPLAY_BG, Stroke::new(1.0, KNOB_EDGE), StrokeKind::Inside);
        let (name, bank, prog) = {
            let s = params.patch.read();
            (s.name.clone(), s.bank, s.program)
        };
        let bank_name = st.banks.get(bank).map(|b| b.name.as_str()).unwrap_or("");
        let num = if prog == NO_PROGRAM { "---".to_string() } else { format!("{:03}", prog + 1) };
        p.text(pos2(r.left() + 2.0 * U, r.top() + 2.5 * U), Align2::LEFT_CENTER, num, value_font(), ORANGE);
        p.text(pos2(r.left() + 6.0 * U, r.top() + 2.5 * U), Align2::LEFT_CENTER, &name, egui::FontId::proportional(16.0), DISPLAY_FG);
        let info = st.message.clone().unwrap_or_else(|| bank_name.to_lowercase());
        p.text(pos2(r.left() + 2.0 * U, r.bottom() - 1.75 * U), Align2::LEFT_CENTER, info, label_font(), RULE);
        let voices = shared.active_voices.load(Ordering::Relaxed);
        p.text(pos2(r.right() - 2.0 * U, r.bottom() - 1.75 * U), Align2::RIGHT_CENTER, format!("{voices} v"), value_font(), RULE);

        ui.add_space(U);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                if ui.button("◀").clicked() {
                    step_program(params, shared, setter, st, -1);
                }
                if ui.button("▶").clicked() {
                    step_program(params, shared, setter, st, 1);
                }
                let label = if st.browser { "panel" } else { "browse" };
                if ui.button(label).clicked() {
                    st.browser = !st.browser;
                }
            });
            ui.horizontal(|ui| {
                if ui.button("import").on_hover_text("Load a Synth1 .sy1 file or .zip bank").clicked() {
                    import(params, st);
                }
                if ui.button("save").on_hover_text("Save the current sound as a .sy1 file").clicked() {
                    save(params, st);
                }
            });
        });

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            knob(ui, &params.master, setter, "volume", true);
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Browser
// ---------------------------------------------------------------------------------------------

const COLS: usize = 5;

fn browser(ui: &mut egui::Ui, params: &Es1Params, shared: &Shared, setter: &ParamSetter, st: &mut UiState) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(24.0 * U);
            section(ui, "banks");
            let mut remove = None;
            for (i, b) in st.banks.iter().enumerate() {
                ui.horizontal(|ui| {
                    let sel = i == st.bank_sel;
                    let t = RichText::new(format!("{}  ({})", b.name, b.patches.len())).color(if sel { INK } else { INK_SOFT });
                    if ui.selectable_label(sel, t).clicked() {
                        st.bank_sel = i;
                    }
                    if i > 0 && ui.small_button("×").on_hover_text("remove bank").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                st.banks.remove(i);
                params.banks.write().remove(i - 1);
                st.bank_sel = st.bank_sel.min(st.banks.len() - 1);
            }
        });
        ui.add_space(2.0 * U);
        ui.vertical(|ui| {
            section(ui, "programs");
            let b = st.bank_sel.min(st.banks.len() - 1);
            let cur = {
                let s = params.patch.read();
                (s.bank == b).then_some(s.program)
            };
            let mut clicked = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("programs").num_columns(COLS).spacing(vec2(2.0 * U, 0.25 * U)).show(ui, |ui| {
                    let n = st.banks[b].patches.len();
                    let rows = n.div_ceil(COLS);
                    for r in 0..rows {
                        for c in 0..COLS {
                            let i = c * rows + r;
                            if let Some(p) = st.banks[b].patches.get(i) {
                                let sel = cur == Some(i);
                                let t = RichText::new(format!("{:03}  {}", i + 1, p.name)).size(11.0).color(if sel { ORANGE } else { INK });
                                if ui.add(egui::Label::new(t).sense(Sense::click())).clicked() {
                                    clicked = Some(i);
                                }
                            } else {
                                ui.label("");
                            }
                        }
                        ui.end_row();
                    }
                });
            });
            if let Some(i) = clicked {
                let patch = st.banks[b].patches[i].clone();
                load_patch(params, shared, setter, &patch, b, i);
                st.message = None;
            }
        });
    });
}

// ---------------------------------------------------------------------------------------------
// Main panel
// ---------------------------------------------------------------------------------------------

/// One control for a sy1 parameter, chosen by its kind.
fn ctl(ui: &mut egui::Ui, params: &Es1Params, setter: &ParamSetter, sy1: usize, label: &str) {
    let (hp, def) = params.get(sy1);
    match hp {
        HostParam::F(p) => {
            knob(ui, p, setter, label, sy1 == 19);
        }
        HostParam::B(p) => {
            switch(ui, p, setter, label);
        }
        HostParam::I(p) => match def.kind {
            Kind::Choice { labels, .. } if labels.len() <= 7 => {
                selector(ui, p, setter, label, labels);
            }
            _ => {
                knob(ui, p, setter, label, false);
            }
        },
    }
}

fn group(ui: &mut egui::Ui, params: &Es1Params, setter: &ParamSetter, title: &str, items: &[(usize, &str)]) {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(U as i8, (U * 0.5) as i8)).show(ui, |ui| {
        ui.vertical(|ui| {
            let resp = ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for &(sy1, label) in items {
                    ctl(ui, params, setter, sy1, label);
                }
            });
            let w = resp.response.rect.width();
            // Header drawn above the row, as wide as its controls.
            let top = resp.response.rect.top();
            let p = ui.painter();
            let g = p.layout_no_wrap(title.to_lowercase(), section_font(), INK);
            let y = top - 1.25 * U;
            let x0 = resp.response.rect.left();
            let tw = g.size().x;
            p.galley(pos2(x0, y - g.size().y * 0.5), g, INK);
            p.line_segment([pos2(x0 + tw + U, y), pos2(x0 + w, y)], Stroke::new(1.0, RULE));
        });
    });
}

fn row(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(2.5 * U);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0 * U;
        f(ui)
    });
}

fn panel(ui: &mut egui::Ui, params: &Es1Params, setter: &ParamSetter) {
    row(ui, |ui| {
        group(ui, params, setter, "oscillator 1", &[(0, "wave"), (76, "detune"), (45, "fm"), (95, "sub"), (96, "sub wave"), (97, "sub oct")]);
        group(ui, params, setter, "oscillator 2", &[(1, "wave"), (2, "pitch"), (3, "fine"), (6, "sync"), (7, "ring"), (4, "track")]);
        group(ui, params, setter, "mixer", &[(5, "mix"), (8, "p/w"), (91, "phase"), (9, "key"), (72, "tune")]);
    });
    row(ui, |ui| {
        group(ui, params, setter, "mod envelope", &[(10, "on"), (71, "dest"), (12, "attack"), (13, "decay"), (11, "amount")]);
        group(
            ui,
            params,
            setter,
            "filter",
            &[(14, "type"), (19, "cutoff"), (20, "reso"), (21, "env"), (23, "sat"), (22, "track"), (24, "vel"), (15, "a"), (16, "d"), (17, "s"), (18, "r")],
        );
    });
    row(ui, |ui| {
        group(ui, params, setter, "amplifier", &[(25, "a"), (26, "d"), (27, "s"), (28, "r"), (29, "gain"), (30, "vel")]);
        group(ui, params, setter, "lfo 1", &[(57, "on"), (42, "wave"), (41, "dest"), (43, "speed"), (44, "depth"), (67, "tempo"), (68, "key")]);
        group(ui, params, setter, "lfo 2", &[(58, "on"), (47, "wave"), (46, "dest"), (48, "speed"), (49, "depth"), (69, "tempo"), (70, "key")]);
    });
    row(ui, |ui| {
        group(
            ui,
            params,
            setter,
            "voice",
            &[(38, "mode"), (94, "poly"), (39, "porta"), (74, "auto"), (73, "unison"), (93, "voices"), (75, "detune"), (92, "phase"), (84, "spread"), (85, "pitch"), (40, "bend")],
        );
        group(ui, params, setter, "arpeggiator", &[(59, "on"), (31, "type"), (32, "range"), (33, "beat"), (34, "gate")]);
    });
    row(ui, |ui| {
        group(ui, params, setter, "effect", &[(77, "on"), (78, "type"), (79, "ctl 1"), (80, "ctl 2"), (81, "level")]);
        group(ui, params, setter, "equalizer", &[(61, "freq"), (62, "level"), (63, "q"), (60, "tone"), (90, "pan")]);
        group(ui, params, setter, "delay", &[(65, "on"), (82, "type"), (35, "time"), (83, "spread"), (36, "fdbk"), (98, "tone"), (37, "mix")]);
    });
    row(ui, |ui| {
        group(ui, params, setter, "chorus", &[(66, "on"), (64, "type"), (52, "time"), (53, "depth"), (54, "rate"), (55, "fdbk"), (56, "level")]);
        group(ui, params, setter, "control", &[(86, "src 1"), (87, "target 1"), (50, "amt 1"), (88, "src 2"), (89, "target 2"), (51, "amt 2")]);
    });
}
