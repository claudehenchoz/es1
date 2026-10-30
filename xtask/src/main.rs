// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! `cargo xtask bundle es1-vst3 --release` (or `bundle-universal` on macOS): nih-plug's bundler,
//! then the license files.
//!
//! The GPL requires a copy of the license to travel with the binary, and the permissive
//! licenses of the dependencies require their notices to, so both go into
//! `ES 1.vst3/Contents/Resources/`. On macOS the bundle gets the package version in its
//! `Info.plist` and is signed again (ad hoc), since the files are added after nih-plug signs it.

use std::path::Path;
use std::process::Command;

const LICENSE_FILES: [&str; 2] = ["LICENSE", "THIRD_PARTY_LICENSES.md"];

fn main() -> nih_plug_xtask::Result<()> {
    nih_plug_xtask::main()?;
    if matches!(std::env::args().nth(1).as_deref(), Some("bundle" | "bundle-universal")) {
        finish_bundles()?;
    }
    Ok(())
}

fn finish_bundles() -> nih_plug_xtask::Result<()> {
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
            if cfg!(target_os = "macos") {
                set_plist_version(&bundle.join("Contents").join("Info.plist"))?;
                let ok = Command::new("codesign").args(["-f", "-s", "-"]).arg(&bundle).status().is_ok_and(|s| s.success());
                if !ok {
                    return Err(std::io::Error::other(format!("could not sign '{}'", bundle.display())).into());
                }
            }
        }
    }
    Ok(())
}

/// nih-plug writes version 1.0.0; use the package version, so installers can tell releases apart.
fn set_plist_version(plist: &Path) -> nih_plug_xtask::Result<()> {
    let Ok(s) = std::fs::read_to_string(plist) else { return Ok(()) };
    let v = env!("CARGO_PKG_VERSION");
    std::fs::write(plist, s.replace("<string>1.0.0</string>", &format!("<string>{v}</string>")))?;
    Ok(())
}
