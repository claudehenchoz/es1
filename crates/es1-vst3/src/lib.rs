// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! The ES 1 VST3 plugin: `cargo xtask bundle es1-vst3 --release`.

nih_plug::nih_export_vst3!(es1_plugin::Es1);
