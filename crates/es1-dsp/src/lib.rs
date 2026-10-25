//! ES 1 synthesis engine.
//!
//! Architecture and parameter mapping follow Daichi's Synth1 (reverse-engineered, see
//! `docs/synth1-spec.md`); the DSP itself is modern: band-limited oscillators and
//! zero-delay-feedback filters.

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
