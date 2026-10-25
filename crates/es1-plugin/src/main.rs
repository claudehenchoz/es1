//! Standalone ES 1 (for development: `cargo run -p es1-plugin --features standalone`).
fn main() {
    nih_plug::nih_export_standalone::<es1_plugin::Es1>();
}
