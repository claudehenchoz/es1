// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! Embed `presets/factory/NNN.sy1` into the binary, in file-name order.
use std::{env, fs, path::PathBuf};

fn main() {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../presets/factory");
    let dir = dir.canonicalize().expect("presets/factory");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read presets/factory")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "sy1"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no factory patches in {}", dir.display());
    let mut out = String::from("pub static FACTORY: &[&[u8]] = &[\n");
    for f in &files {
        // Rebuild when a patch is edited in place (a directory's mtime need not change).
        println!("cargo:rerun-if-changed={}", f.display());
        out.push_str(&format!("    include_bytes!({:?}),\n", f.display().to_string()));
    }
    out.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("factory.rs"), out).unwrap();
}
