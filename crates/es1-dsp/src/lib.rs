//! ES 1 synthesis engine.
//!
//! A two-oscillator virtual analog with band-limited oscillators and zero-delay-feedback
//! filters. Parameters are keyed by their `.sy1` patch-format index.

pub mod arp;
pub mod env;
pub mod filter;
pub mod fx;
pub mod lfo;
pub mod osc;
pub mod params;
pub mod synth;
pub mod util;
pub mod voice;

pub use params::{Values, PARAMS};
pub use synth::Synth;
