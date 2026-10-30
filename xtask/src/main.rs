// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! `cargo xtask bundle es1-plugin --release`: nih-plug's bundler, then the license files.
//!
//! The GPL requires a copy of the license to travel with the binary, and the permissive
//! licenses of the dependencies require their notices to, so both go into
//! `ES 1.vst3/Contents/Resources/`.

use std::path::Path;

const LICENSE_FILES: [&str; 2] = ["LICENSE", "THIRD_PARTY_LICENSES.md"];

fn main() -> nih_plug_xtask::Result<()> {
    nih_plug_xtask::main()?;
    if std::env::args().nth(1).as_deref() == Some("bundle") {
        add_license_files()?;
    }
    Ok(())
}

fn add_license_files() -> nih_plug_xtask::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("workspace root");
    let bundled = root.join("target").join("bundled");
    let Ok(entries) = std::fs::read_dir(&bundled) else { return Ok(()) };
    for e in entries.flatten() {
        let bundle = e.path();
        if bundle.extension().is_some_and(|x| x == "vst3") && bundle.is_dir() {
            let res = bundle.join("Contents").join("Resources");
            std::fs::create_dir_all(&res)?;
            for f in LICENSE_FILES {
                std::fs::copy(root.join(f), res.join(f))?;
            }
            println!("Added {} to '{}'", LICENSE_FILES.join(", "), bundle.display());
        }
    }
    Ok(())
}
