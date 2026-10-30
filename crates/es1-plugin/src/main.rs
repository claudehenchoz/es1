// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Standalone ES 1 (for development: `cargo run -p es1-plugin --features standalone`).
fn main() {
    nih_plug::nih_export_standalone::<es1_plugin::Es1>();
}
