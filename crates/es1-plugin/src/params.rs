//! Host-facing parameters, generated from the engine's parameter table (`es1_dsp::PARAMS`).
//!
//! * knobs → `FloatParam` in sy1 units (0..127),
//! * switches → `BoolParam`,
//! * choices / stepped values → `IntParam` (choices store the UI position, mapped back to the
//!   sy1 value with `Kind::Choice::values`).

use es1_dsp::params::{self, Kind, ParamDef, NUM_SY1};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;

pub enum HostParam {
    F(FloatParam),
    I(IntParam),
    B(BoolParam),
}

impl HostParam {
    pub fn ptr(&self) -> ParamPtr {
        match self {
            HostParam::F(p) => p.as_ptr(),
            HostParam::I(p) => p.as_ptr(),
            HostParam::B(p) => p.as_ptr(),
        }
    }

    /// Current value in sy1 units.
    pub fn sy1_value(&self, def: &ParamDef) -> f32 {
        match (self, def.kind) {
            (HostParam::F(p), _) => p.value(),
            (HostParam::B(p), _) => p.value() as i32 as f32,
            (HostParam::I(p), Kind::Choice { values, .. }) => values[(p.value() as usize).min(values.len() - 1)] as f32,
            (HostParam::I(p), _) => p.value() as f32,
        }
    }

    /// Normalized host value for a sy1 value (for preset loading through the `ParamSetter`).
    pub fn normalized_for(&self, def: &ParamDef, v: i32) -> f32 {
        match (self, def.kind) {
            (HostParam::F(p), _) => p.preview_normalized(v as f32),
            (HostParam::B(p), _) => p.preview_normalized(v != 0),
            (HostParam::I(p), Kind::Choice { .. }) => p.preview_normalized(def.choice_index(v) as i32),
            (HostParam::I(p), _) => p.preview_normalized(v),
        }
    }
}

fn make(def: &'static ParamDef) -> HostParam {
    let sy1 = def.sy1 as usize;
    match def.kind {
        Kind::Continuous => HostParam::F(
            FloatParam::new(def.name, def.default as f32, FloatRange::Linear { min: def.min as f32, max: def.max as f32 })
                .with_step_size(0.01)
                .with_value_to_string(Arc::new(move |v| params::display(sy1, v))),
        ),
        Kind::Switch => HostParam::B(BoolParam::new(def.name, def.default != 0)),
        Kind::Choice { labels, values } => {
            let n = labels.len() as i32;
            HostParam::I(
                IntParam::new(def.name, def.choice_index(def.default) as i32, IntRange::Linear { min: 0, max: n - 1 })
                    .with_value_to_string(Arc::new(move |i| labels.get(i as usize).copied().unwrap_or("?").to_string()))
                    .with_string_to_value(Arc::new(move |s| labels.iter().position(|l| l.eq_ignore_ascii_case(s.trim())).map(|i| i as i32)))
                    .non_automatable_if(values.len() > 64),
            )
        }
        Kind::Int => HostParam::I(
            IntParam::new(def.name, def.default, IntRange::Linear { min: def.min, max: def.max })
                .with_value_to_string(Arc::new(move |v| params::display(sy1, v as f32))),
        ),
    }
}

trait NonAutomatableIf {
    fn non_automatable_if(self, c: bool) -> Self;
}

impl NonAutomatableIf for IntParam {
    fn non_automatable_if(self, c: bool) -> Self {
        if c {
            self.non_automatable()
        } else {
            self
        }
    }
}

/// An imported bank, kept in the plugin state so projects reopen with it.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct StoredBank {
    pub name: String,
    /// `.sy1` file contents.
    pub patches: Vec<String>,
}

/// `PatchState::program` when the sound is not (or no longer) a stored program.
pub const NO_PROGRAM: usize = usize::MAX;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PatchState {
    pub name: String,
    /// (bank index, program index); bank 0 = factory.
    pub bank: usize,
    pub program: usize,
}

impl Default for PatchState {
    fn default() -> Self {
        Self { name: "initial sound".into(), bank: 0, program: NO_PROGRAM }
    }
}

pub struct Es1Params {
    pub editor_state: Arc<EguiState>,
    pub patch: RwLock<PatchState>,
    pub banks: RwLock<Vec<StoredBank>>,
    pub master: FloatParam,
    /// Parallel to `es1_dsp::PARAMS`.
    pub list: Vec<HostParam>,
    /// sy1 index → position in `list`.
    pub by_sy1: [usize; NUM_SY1],
}

impl Default for Es1Params {
    fn default() -> Self {
        let list: Vec<HostParam> = params::PARAMS.iter().map(make).collect();
        let mut by_sy1 = [0; NUM_SY1];
        for (i, d) in params::PARAMS.iter().enumerate() {
            by_sy1[d.sy1 as usize] = i;
        }
        Self {
            editor_state: EguiState::from_size(crate::ui::WIDTH, crate::ui::HEIGHT),
            patch: RwLock::new(PatchState::default()),
            banks: RwLock::new(Vec::new()),
            master: FloatParam::new("Master Volume", 64.0, FloatRange::Linear { min: 0.0, max: 127.0 })
                .with_step_size(0.01)
                .with_value_to_string(Arc::new(|v| {
                    let g = es1_dsp::synth::master_gain(v);
                    if g <= 0.0 {
                        "-inf dB".into()
                    } else {
                        format!("{:+.1} dB", 20.0 * g.log10())
                    }
                })),
            list,
            by_sy1,
        }
    }
}

impl Es1Params {
    pub fn get(&self, sy1: usize) -> (&HostParam, &'static ParamDef) {
        let i = self.by_sy1[sy1];
        (&self.list[i], &params::PARAMS[i])
    }

    /// All current values in sy1 units.
    pub fn values(&self) -> [f32; NUM_SY1] {
        let mut v = [0.0; NUM_SY1];
        for (p, d) in self.list.iter().zip(params::PARAMS) {
            v[d.sy1 as usize] = p.sy1_value(d);
        }
        v
    }
}

fn group_name(g: params::Group) -> &'static str {
    use params::Group::*;
    match g {
        Osc => "Oscillators",
        Filter => "Filter",
        Amp => "Amplifier",
        Lfo1 => "LFO 1",
        Lfo2 => "LFO 2",
        Effect => "Effect",
        Eq => "Equalizer",
        Delay => "Delay",
        Chorus => "Chorus",
        Voice => "Voice",
        Arp => "Arpeggiator",
        Control => "Control",
    }
}

unsafe impl Params for Es1Params {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        let mut v = vec![("master".to_string(), self.master.as_ptr(), String::new())];
        for (p, d) in self.list.iter().zip(params::PARAMS) {
            v.push((d.id.to_string(), p.ptr(), group_name(d.group).to_string()));
        }
        v
    }

    fn serialize_fields(&self) -> BTreeMap<String, String> {
        use nih_plug::params::persist::{serialize_field, PersistentField};
        let mut m = BTreeMap::new();
        if let Ok(s) = PersistentField::map(&self.editor_state, serialize_field) {
            m.insert("editor-state".into(), s);
        }
        if let Ok(s) = PersistentField::map(&self.patch, serialize_field) {
            m.insert("patch".into(), s);
        }
        if let Ok(s) = PersistentField::map(&self.banks, serialize_field) {
            m.insert("banks".into(), s);
        }
        m
    }

    fn deserialize_fields(&self, serialized: &BTreeMap<String, String>) {
        use nih_plug::params::persist::{deserialize_field, PersistentField};
        if let Some(Ok(v)) = serialized.get("editor-state").map(|s| deserialize_field(s)) {
            PersistentField::set(&self.editor_state, v);
        }
        if let Some(Ok(v)) = serialized.get("patch").map(|s| deserialize_field::<PatchState>(s)) {
            *self.patch.write() = v;
        }
        if let Some(Ok(v)) = serialized.get("banks").map(|s| deserialize_field::<Vec<StoredBank>>(s)) {
            *self.banks.write() = v;
        }
    }
}
