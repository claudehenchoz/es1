# Synth1 → ES 1: reverse-engineering spec and port notes

Source analysed: `synth1-decomp/Synth1 VST64.dll` (Synth1 1.13 beta3, 2014, sha256 `51c6fe60…`),
with REA + Ghidra 12.1.3. The analysis was static only; no Synth1 code was run.

| Document | Contents |
|---|---|
| [re/param_table.md](re/param_table.md) | Decoded parameter descriptor table (0x180093f10): sy1 index, range, names |
| [re/voice_patch_arp.md](re/voice_patch_arp.md) | `.sy1` loading, the default template, the version-conversion chain, VST float↔int mapping, voice allocation, unison, portamento, bend, MIDI-controller modulation, arpeggiator, master volume |
| [re/osc_env_lfo.md](re/osc_env_lfo.md) | Oscillators (wavetables, FM, sync, ring, sub, detune, phase), mixer law, envelopes, mod envelope, LFOs |
| [re/filter_fx.md](re/filter_fx.md) | Filter types, cutoff/key-track/envelope mapping, post-filter limiter/saturation, effect block types 0–9, EQ/tone/pan, delay, chorus, master chain order |

Facts are tagged **[O]** observed, **[I]** inferred or **[U]** unknown, and each cites a function address.

## Where it lives in the port

| Synth1 | ES 1 |
|---|---|
| Descriptor table, template, `FUN_180026a10` conversion | `crates/es1-dsp/src/params.rs` (`PARAMS`, `template()`, `convert()`, `from_sparse()`) |
| `.sy1` reader / writer, zip banks | `crates/es1-presets` (factory bank embedded from `presets/factory/`) |
| `CSynthesizer1` dispatch, allocator, unison, LFO routing, controllers | `crates/es1-dsp/src/synth.rs` |
| `CVoice` / `CVco` / `CVcf` / `CVca` | `crates/es1-dsp/src/voice.rs`, `osc.rs`, `filter.rs`, `env.rs` |
| `CLFO` | `crates/es1-dsp/src/lfo.rs` |
| `CArpeggiator` | `crates/es1-dsp/src/arp.rs` |
| `CEffector`, `CEq`, `CPan`, `CDelay`, `CChorus` | `crates/es1-dsp/src/fx/` |

## Kept exactly
Parameter set and sy1 indices, template defaults, version conversion (including the chorus-feedback quirk), every 0..127 → physical-unit curve (envelope times, sustain/gain law, cutoff table, key track, envelope headroom clamp, LFO rates and sync tables, FM depth, PW law, mixer normalisation, unison detune, portamento curve, effect/EQ/delay/chorus mappings), voice allocation policy, mono/legato priority, the arpeggiator patterns and beat table, the effect chain order (including the delay wet signal being routed through the chorus), and the master volume curve.

## Deliberate deviations ("same architecture, modern DSP")
* **Oscillators:** Synth1 reads 2048-sample additive wavetables with mip levels fixed at 22050 Hz. ES 1 uses PolyBLEP/PolyBLAMP oscillators that are band-limited at any sample rate, plus BLEP-corrected hard sync. FM still modulates osc 1's phase increment by the raw osc 2 sample, as Synth1 does.
* **Filters:** the double-sampled Chamberlin SVF becomes a Cytomic ZDF SVF (LP12/HP12/BP12). The Stilson/Smith LP24 becomes a Zavalishin TPT ladder with tanh feedback. The diode ladder (LPDL) is a simplified coupled 4-stage model at 2× oversampling, not a transcription of the Wurtz-style ZDF code. Cutoff is computed continuously instead of from Synth1's 1024-entry integer table.
* **Control rate:** Synth1 holds LFO values and envelope gains over 32–64-sample chunks. ES 1 updates pitch, cutoff and LFOs every 16 samples, and runs the amp envelope per sample.
* **Osc 1 detune mode:** the sub oscillator is a single oscillator (gain-compensated) rather than 9 summed copies.
* **Unison offsets** are assigned by position within the note's voice group rather than by voice-slot number.
* **Sync reset** in Synth1 is "≈ phase 0" in integer arithmetic. ES 1 uses exactly 0, at the sub-sample position.
* **Phaser, compressor, decimator and distortion** follow the recovered formulas. The peak EQ uses an RBJ biquad in place of Synth1's pole/zero design, with the same frequency, gain and Q ranges.

## Open items
* The UI names of effect and delay types are inferred ([U] in the spec).
* Delay-time entry 0 maps to 0 ticks in Synth1's table; ES 1 keeps the value but clamps it to the minimum delay.
* The sustain-pedal path was not traced; ES 1 uses standard CC64 behaviour.
