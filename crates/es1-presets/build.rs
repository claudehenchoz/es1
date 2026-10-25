//! Embed `presets/factory/001.sy1 ..= 128.sy1` into the binary.
use std::{env, fs, path::PathBuf};

fn main() {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../presets/factory");
    let dir = dir.canonicalize().expect("presets/factory");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut out = String::from("pub static FACTORY: &[&[u8]] = &[\n");
    for i in 1..=128 {
        let f = dir.join(format!("{i:03}.sy1"));
        assert!(f.exists(), "missing {}", f.display());
        out.push_str(&format!("    include_bytes!({:?}),\n", f.display().to_string()));
    }
    out.push_str("];\n");
    fs::write(PathBuf::from(env::var("OUT_DIR").unwrap()).join("factory.rs"), out).unwrap();
}
