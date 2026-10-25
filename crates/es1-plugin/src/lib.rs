//! ES 1: VST3 instrument. Synth1's architecture, a modern engine, a Braun face.

mod params;
mod ui;

use es1_dsp::params::NUM_SY1;
use es1_dsp::Synth;
use nih_plug::prelude::*;
use params::Es1Params;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// State shared between the audio thread and the editor.
#[derive(Default)]
pub struct Shared {
    /// A new patch was loaded from the GUI: silence voices once its values arrive.
    pub patch_loaded: AtomicBool,
    pub active_voices: AtomicUsize,
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
        self.shared.active_voices.store(self.synth.active_voices(), Ordering::Relaxed);
        ProcessStatus::KeepAlive
    }
}

impl Vst3Plugin for Es1 {
    const VST3_CLASS_ID: [u8; 16] = *b"ES1-ElektronSyn1";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nih_export_vst3!(Es1);
