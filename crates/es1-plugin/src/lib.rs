// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Claude Henchoz

//! ES 1: VST3 instrument. A virtual analog engine with a Braun face.

mod params;
mod ui;

use es1_dsp::params::NUM_SY1;
use es1_dsp::Synth;
use nih_plug::prelude::*;
use params::Es1Params;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

/// Samples kept for the scope: about 85 ms at 48 kHz, enough for two periods of a low note.
pub const SCOPE_LEN: usize = 4096;

/// Lock-free ring of the most recent mono output, written by the audio thread and read by the
/// editor's scope. A reader can see a partly overwritten ring; for a display that is harmless.
pub struct Scope {
    buf: Box<[AtomicU32]>,
    pos: AtomicUsize,
}

impl Default for Scope {
    fn default() -> Self {
        Self { buf: (0..SCOPE_LEN).map(|_| AtomicU32::new(0)).collect(), pos: AtomicUsize::new(0) }
    }
}

impl Scope {
    fn push(&self, l: &[f32], r: &[f32]) {
        let mut p = self.pos.load(Ordering::Relaxed);
        for (a, b) in l.iter().zip(r) {
            self.buf[p % SCOPE_LEN].store((0.5 * (a + b)).to_bits(), Ordering::Relaxed);
            p = p.wrapping_add(1);
        }
        self.pos.store(p, Ordering::Release);
    }

    /// Copy the newest `out.len()` samples, oldest first.
    pub fn snapshot(&self, out: &mut [f32]) {
        let end = self.pos.load(Ordering::Acquire);
        let n = out.len().min(SCOPE_LEN);
        for (i, o) in out[..n].iter_mut().enumerate() {
            let at = end.wrapping_sub(n - i) % SCOPE_LEN;
            *o = f32::from_bits(self.buf[at].load(Ordering::Relaxed));
        }
    }
}

/// State shared between the audio thread and the editor.
pub struct Shared {
    /// A new patch was loaded from the GUI: silence voices once its values arrive.
    pub patch_loaded: AtomicBool,
    pub active_voices: AtomicUsize,
    pub scope: Scope,
    /// Debug UI self-test only: a note to play (key number), or -1.
    pub test_note: AtomicI32,
}

impl Default for Shared {
    fn default() -> Self {
        Self { patch_loaded: AtomicBool::new(false), active_voices: AtomicUsize::new(0), scope: Scope::default(), test_note: AtomicI32::new(-1) }
    }
}

pub struct Es1 {
    params: Arc<Es1Params>,
    shared: Arc<Shared>,
    synth: Synth,
    sent: [f32; NUM_SY1],
    sent_master: f32,
}

impl Default for Es1 {
    fn default() -> Self {
        Self {
            params: Arc::new(Es1Params::default()),
            shared: Arc::new(Shared::default()),
            synth: Synth::new(48_000.0),
            sent: [f32::NAN; NUM_SY1],
            sent_master: f32::NAN,
        }
    }
}

impl Es1 {
    /// Forward every changed host parameter to the engine.
    fn sync_params(&mut self) {
        let vals = self.params.values();
        for (i, &v) in vals.iter().enumerate() {
            if v.to_bits() != self.sent[i].to_bits() {
                self.sent[i] = v;
                self.synth.set_param(i, v);
            }
        }
        let m = self.params.master.value();
        if m != self.sent_master {
            self.sent_master = m;
            self.synth.set_master_volume(m);
        }
    }

    fn handle(&mut self, ev: NoteEvent<()>) {
        match ev {
            NoteEvent::NoteOn { note, velocity, .. } => self.synth.note_on(note, (velocity * 127.0).round().clamp(1.0, 127.0) as u8),
            NoteEvent::NoteOff { note, .. } => self.synth.note_off(note),
            NoteEvent::Choke { .. } => self.synth.all_notes_off(),
            NoteEvent::MidiPitchBend { value, .. } => self.synth.pitch_bend(value * 2.0 - 1.0),
            NoteEvent::MidiCC { cc, value, .. } => self.synth.midi_cc(cc, (value * 127.0).round() as u8),
            NoteEvent::MidiChannelPressure { pressure, .. } => self.synth.aftertouch(pressure),
            _ => {}
        }
    }
}

impl Plugin for Es1 {
    const NAME: &'static str = "ES 1";
    const VENDOR: &'static str = "ES";
    const URL: &'static str = "";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];
    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        ui::create(self.params.clone(), self.shared.clone())
    }

    fn initialize(&mut self, _layout: &AudioIOLayout, config: &BufferConfig, _ctx: &mut impl InitContext<Self>) -> bool {
        self.synth = Synth::new(config.sample_rate);
        self.sent = [f32::NAN; NUM_SY1];
        self.sent_master = f32::NAN;
        self.sync_params();
        true
    }

    fn reset(&mut self) {
        self.synth.panic();
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, ctx: &mut impl ProcessContext<Self>) -> ProcessStatus {
        if let Some(bpm) = ctx.transport().tempo {
            self.synth.set_tempo(bpm as f32);
        }
        self.sync_params();
        if self.shared.patch_loaded.swap(false, Ordering::AcqRel) {
            self.synth.panic();
        }
        let t = self.shared.test_note.swap(-1, Ordering::Relaxed);
        if t >= 0 {
            self.synth.all_notes_off();
            self.synth.note_on(t as u8, 100);
        }

        let n = buffer.samples();
        let out = buffer.as_slice();
        let (l, rest) = out.split_at_mut(1);
        let (l, r) = (&mut *l[0], &mut *rest[0]);
        let mut pos = 0usize;
        while let Some(ev) = ctx.next_event() {
            let t = (ev.timing() as usize).min(n);
            if t > pos {
                self.synth.process(&mut l[pos..t], &mut r[pos..t]);
                pos = t;
            }
            self.handle(ev);
        }
        if pos < n {
            self.synth.process(&mut l[pos..n], &mut r[pos..n]);
        }
        self.shared.scope.push(l, r);
        self.shared.active_voices.store(self.synth.active_voices(), Ordering::Relaxed);
        ProcessStatus::KeepAlive
    }
}

impl Vst3Plugin for Es1 {
    const VST3_CLASS_ID: [u8; 16] = *b"ES1-ElektronSyn1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_vst3!(Es1);
