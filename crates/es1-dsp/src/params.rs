//! The parameter set, keyed by `.sy1` patch-format index: names, ranges, the defaults of the
//! "initial sound" template, and the conversion of files written by older format versions
//! (100..111).

/// Number of sy1 slots used (0..=98).
pub const NUM_SY1: usize = 99;
/// The current patch format version.
pub const CURRENT_VERSION: i32 = 113;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// 0..127 knob (or another integer range treated as continuous).
    Continuous,
    /// Integer stepped value (min..=max).
    Int,
    /// On/off.
    Switch,
    /// Enumerated: `labels[i]` displays sy1 value `values[i]`. Order is the UI order.
    Choice { labels: &'static [&'static str], values: &'static [i32] },
}

#[derive(Clone, Copy, Debug)]
pub struct ParamDef {
    pub sy1: u8,
    /// Stable plugin parameter id. Never rename: hosts store automation by id.
    pub id: &'static str,
    pub name: &'static str,
    pub group: Group,
    pub min: i32,
    pub max: i32,
    pub default: i32,
    pub kind: Kind,
    /// MIDI-controller modulation may target it.
    pub modulatable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Osc,
    Filter,
    Amp,
    Lfo1,
    Lfo2,
    Effect,
    Eq,
    Delay,
    Chorus,
    Voice,
    Arp,
    Control,
}

macro_rules! p {
    ($sy1:expr, $id:expr, $name:expr, $g:ident, $min:expr, $max:expr, $def:expr, $kind:expr, $m:expr) => {
        ParamDef { sy1: $sy1, id: $id, name: $name, group: Group::$g, min: $min, max: $max, default: $def, kind: $kind, modulatable: $m }
    };
}

const C: Kind = Kind::Continuous;
const I: Kind = Kind::Int;
const S: Kind = Kind::Switch;

pub const OSC1_WAVES: Kind = Kind::Choice { labels: &["sine", "tri", "saw", "pulse"], values: &[0, 3, 1, 2] };
pub const OSC2_WAVES: Kind = Kind::Choice { labels: &["tri", "saw", "pulse", "noise"], values: &[3, 1, 2, 4] };
pub const SUB_WAVES: Kind = Kind::Choice { labels: &["sine", "tri", "saw", "square"], values: &[0, 1, 2, 3] };
pub const SUB_OCT: Kind = Kind::Choice { labels: &["0", "-1 oct"], values: &[0, 1] };
pub const MOD_DEST: Kind = Kind::Choice { labels: &["osc 2", "fm", "p/w"], values: &[0, 1, 2] };
pub const FILTER_TYPES: Kind = Kind::Choice { labels: &["lp 12", "lp 24", "hp 12", "bp 12", "lp dl"], values: &[0, 1, 2, 3, 4] };
pub const EFFECT_TYPES: Kind = Kind::Choice {
    labels: &["drive 1", "drive 2", "fold", "lo-fi", "ring", "comp", "phase 1", "phase 2", "phase 4", "phase 6"],
    values: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
};
pub const DELAY_TYPES: Kind = Kind::Choice { labels: &["stereo", "cross", "ping-pong"], values: &[0, 1, 2] };
pub const DELAY_TIMES: Kind = Kind::Choice {
    labels: &[
        "0", "1/96", "1/32T", "1/32", "1/16T", "1/16", "1/8T", "1/16.", "1/8", "1/4T", "1/8.", "1/8..", "1/4", "1/2T", "1/4.", "1/4..",
        "1/2", "1/2.", "1/2..", "1/1",
    ],
    values: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19],
};
pub const CHORUS_TYPES: Kind = Kind::Choice { labels: &["1", "2", "3", "4"], values: &[1, 2, 3, 4] };
pub const PLAY_MODES: Kind = Kind::Choice { labels: &["poly", "mono", "legato"], values: &[0, 1, 2] };
pub const ARP_TYPES: Kind = Kind::Choice { labels: &["up/down", "up", "down", "random"], values: &[1, 2, 3, 4] };
pub const ARP_OCT: Kind = Kind::Choice { labels: &["1 oct", "2 oct", "3 oct", "4 oct"], values: &[0, 1, 2, 3] };
/// Arp beat (ticks at 24/quarter).
pub const ARP_TICKS: [u32; 19] = [96, 84, 72, 48, 42, 36, 32, 24, 21, 18, 16, 12, 9, 8, 6, 4, 3, 2, 1];
pub const ARP_BEATS: Kind = Kind::Choice {
    labels: &[
        "1/1", "1/2..", "1/2.", "1/2", "1/4..", "1/4.", "1/2T", "1/4", "1/8..", "1/8.", "1/4T", "1/8", "1/16.", "1/8T", "1/16",
        "1/16T", "1/32", "1/32T", "1/96",
    ],
    values: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18],
};
pub const LFO_WAVES: Kind = Kind::Choice { labels: &["saw", "tri", "sine", "square", "s&h", "smooth"], values: &[0, 1, 5, 2, 3, 4] };
pub const LFO_DESTS: Kind =
    Kind::Choice { labels: &["osc 2", "osc 1+2", "filter", "amp", "p/w", "fm", "pan"], values: &[1, 2, 3, 4, 5, 6, 7] };
/// MIDI controller sources: CC1..CC11, channel pressure, pitch bend.
pub const CTRL_SRCS: Kind = Kind::Choice {
    labels: &[
        "mod wheel", "breath", "cc 3", "foot", "porta time", "data entry", "volume", "balance", "cc 9", "pan", "expression",
        "pressure", "bend",
    ],
    values: &[0xB001, 0xB002, 0xB003, 0xB004, 0xB005, 0xB006, 0xB007, 0xB008, 0xB009, 0xB00A, 0xB00B, 0xD000, 0xE000],
};

/// All parameters, in panel order. `default` is the template value (clamped into range).
pub static PARAMS: &[ParamDef] = &[
    // Oscillators
    p!(0, "osc1_wave", "Osc 1 Wave", Osc, 0, 3, 2, OSC1_WAVES, false),
    p!(76, "osc1_detune", "Osc 1 Detune", Osc, 0, 127, 0, C, true),
    p!(45, "osc1_fm", "Osc 1 FM", Osc, 0, 127, 0, C, true),
    p!(95, "sub_level", "Sub Level", Osc, 0, 127, 0, C, true),
    p!(96, "sub_wave", "Sub Wave", Osc, 0, 3, 1, SUB_WAVES, false),
    p!(97, "sub_octave", "Sub Octave", Osc, 0, 1, 1, SUB_OCT, false),
    p!(1, "osc2_wave", "Osc 2 Wave", Osc, 1, 4, 1, OSC2_WAVES, false),
    p!(6, "osc2_sync", "Osc 2 Sync", Osc, 0, 1, 0, S, false),
    p!(7, "osc2_ring", "Osc 2 Ring", Osc, 0, 1, 0, S, false),
    p!(4, "osc2_track", "Osc 2 Key Track", Osc, 0, 1, 1, S, false),
    p!(2, "osc2_pitch", "Osc 2 Pitch", Osc, 0, 127, 64, C, true),
    p!(3, "osc2_fine", "Osc 2 Fine", Osc, 0, 127, 81, C, true),
    p!(10, "menv_on", "Mod Env On", Osc, 0, 1, 0, S, false),
    p!(12, "menv_attack", "Mod Env Attack", Osc, 0, 127, 0, C, true),
    p!(13, "menv_decay", "Mod Env Decay", Osc, 0, 127, 0, C, true),
    p!(11, "menv_amount", "Mod Env Amount", Osc, 0, 127, 64, C, true),
    p!(71, "menv_dest", "Mod Env Dest", Osc, 0, 2, 0, MOD_DEST, false),
    p!(9, "key_shift", "Key Shift", Osc, -24, 24, 0, I, false),
    p!(5, "osc_mix", "Osc Mix", Osc, 0, 127, 64, C, true),
    p!(8, "pulse_width", "Pulse Width", Osc, 0, 127, 64, C, true),
    p!(91, "osc_phase", "Osc Phase", Osc, 0, 127, 0, C, true),
    p!(72, "fine_tune", "Fine Tune", Osc, 0, 127, 64, C, false),
    // Filter
    p!(14, "flt_type", "Filter Type", Filter, 0, 4, 1, FILTER_TYPES, false),
    p!(15, "flt_attack", "Filter Attack", Filter, 0, 127, 0, C, true),
    p!(16, "flt_decay", "Filter Decay", Filter, 0, 127, 64, C, true),
    p!(17, "flt_sustain", "Filter Sustain", Filter, 0, 127, 32, C, true),
    p!(18, "flt_release", "Filter Release", Filter, 0, 127, 64, C, true),
    p!(19, "flt_freq", "Cutoff", Filter, 0, 127, 81, C, true),
    p!(20, "flt_res", "Resonance", Filter, 0, 127, 14, C, true),
    p!(21, "flt_amount", "Filter Env Amount", Filter, 0, 127, 127, C, true),
    p!(23, "flt_sat", "Saturation", Filter, 0, 127, 0, C, true),
    p!(22, "flt_track", "Filter Key Track", Filter, 0, 127, 64, C, true),
    p!(24, "flt_vel", "Filter Velocity", Filter, 0, 1, 1, S, false),
    // Amp
    p!(25, "amp_attack", "Amp Attack", Amp, 0, 127, 64, C, true),
    p!(26, "amp_decay", "Amp Decay", Amp, 0, 127, 64, C, true),
    p!(27, "amp_sustain", "Amp Sustain", Amp, 0, 127, 107, C, true),
    p!(28, "amp_release", "Amp Release", Amp, 0, 127, 64, C, true),
    p!(29, "amp_gain", "Amp Gain", Amp, 0, 127, 107, C, true),
    p!(30, "amp_vel", "Amp Velocity", Amp, 0, 127, 64, C, false),
    // Effect block
    p!(77, "fx_on", "Effect On", Effect, 0, 1, 0, S, false),
    p!(78, "fx_type", "Effect Type", Effect, 0, 9, 0, EFFECT_TYPES, false),
    p!(79, "fx_ctl1", "Effect Control 1", Effect, 0, 127, 64, C, true),
    p!(80, "fx_ctl2", "Effect Control 2", Effect, 0, 127, 64, C, true),
    p!(81, "fx_level", "Effect Level", Effect, 0, 127, 64, C, true),
    // EQ / pan
    p!(61, "eq_freq", "EQ Freq", Eq, 0, 127, 64, C, true),
    p!(62, "eq_level", "EQ Level", Eq, 0, 127, 64, C, true),
    p!(63, "eq_q", "EQ Q", Eq, 0, 127, 64, C, true),
    p!(60, "eq_tone", "Tone", Eq, 0, 127, 64, C, true),
    p!(90, "pan", "Pan", Eq, 0, 127, 64, C, true),
    // Delay
    p!(65, "dly_on", "Delay On", Delay, 0, 1, 1, S, false),
    p!(82, "dly_type", "Delay Type", Delay, 0, 2, 0, DELAY_TYPES, false),
    p!(35, "dly_time", "Delay Time", Delay, 0, 19, 8, DELAY_TIMES, true),
    p!(83, "dly_spread", "Delay Spread", Delay, 0, 127, 66, C, true),
    p!(36, "dly_feedback", "Delay Feedback", Delay, 0, 127, 40, C, true),
    p!(98, "dly_tone", "Delay Tone", Delay, 0, 127, 64, C, true),
    p!(37, "dly_mix", "Delay Mix", Delay, 0, 127, 20, C, true),
    // Chorus
    p!(66, "cho_on", "Chorus On", Chorus, 0, 1, 1, S, false),
    p!(64, "cho_type", "Chorus Type", Chorus, 1, 4, 2, CHORUS_TYPES, false),
    p!(52, "cho_time", "Chorus Time", Chorus, 0, 127, 64, C, true),
    p!(53, "cho_depth", "Chorus Depth", Chorus, 0, 127, 64, C, true),
    p!(54, "cho_rate", "Chorus Rate", Chorus, 0, 127, 50, C, true),
    p!(55, "cho_feedback", "Chorus Feedback", Chorus, 0, 127, 64, C, true),
    p!(56, "cho_level", "Chorus Level", Chorus, 0, 127, 40, C, true),
    // Voice
    p!(38, "play_mode", "Play Mode", Voice, 0, 2, 0, PLAY_MODES, false),
    p!(94, "polyphony", "Polyphony", Voice, 1, 32, 16, I, false),
    p!(39, "porta_time", "Portamento", Voice, 0, 127, 0, C, true),
    p!(74, "porta_auto", "Portamento Auto", Voice, 0, 1, 0, S, false),
    p!(73, "uni_on", "Unison", Voice, 0, 1, 0, S, false),
    p!(93, "uni_voices", "Unison Voices", Voice, 2, 8, 2, I, false),
    p!(75, "uni_detune", "Unison Detune", Voice, 0, 127, 22, C, true),
    p!(92, "uni_phase", "Unison Phase", Voice, 0, 127, 0, C, true),
    p!(84, "uni_spread", "Unison Spread", Voice, 0, 127, 64, C, true),
    p!(85, "uni_pitch", "Unison Pitch", Voice, 0, 48, 24, I, true),
    p!(40, "bend_range", "Bend Range", Voice, 0, 24, 12, I, false),
    // MIDI control
    p!(86, "ctl1_src", "Control 1 Source", Control, 0, 0xFFFF, 0xB001, CTRL_SRCS, false),
    p!(87, "ctl1_dest", "Control 1 Target", Control, -1, 98, 44, I, false),
    p!(50, "ctl1_amount", "Control 1 Amount", Control, 0, 127, 74, C, false),
    p!(88, "ctl2_src", "Control 2 Source", Control, 0, 0xFFFF, 0xB001, CTRL_SRCS, false),
    p!(89, "ctl2_dest", "Control 2 Target", Control, -1, 98, 43, I, false),
    p!(51, "ctl2_amount", "Control 2 Amount", Control, 0, 127, 74, C, false),
    // Arpeggiator
    p!(59, "arp_on", "Arp On", Arp, 0, 1, 0, S, false),
    p!(31, "arp_type", "Arp Type", Arp, 1, 4, 1, ARP_TYPES, false),
    p!(32, "arp_range", "Arp Range", Arp, 0, 3, 0, ARP_OCT, false),
    p!(33, "arp_beat", "Arp Beat", Arp, 0, 18, 11, ARP_BEATS, false),
    p!(34, "arp_gate", "Arp Gate", Arp, 0, 127, 64, C, true),
    // LFO 1
    p!(57, "lfo1_on", "LFO 1 On", Lfo1, 0, 1, 1, S, false),
    p!(42, "lfo1_wave", "LFO 1 Wave", Lfo1, 0, 5, 1, LFO_WAVES, false),
    p!(41, "lfo1_dest", "LFO 1 Dest", Lfo1, 1, 7, 2, LFO_DESTS, false),
    p!(43, "lfo1_speed", "LFO 1 Speed", Lfo1, 0, 127, 64, C, true),
    p!(44, "lfo1_depth", "LFO 1 Depth", Lfo1, 0, 127, 0, C, true),
    p!(67, "lfo1_sync", "LFO 1 Tempo Sync", Lfo1, 0, 1, 0, S, false),
    p!(68, "lfo1_key", "LFO 1 Key Sync", Lfo1, 0, 1, 0, S, false),
    // LFO 2
    p!(58, "lfo2_on", "LFO 2 On", Lfo2, 0, 1, 1, S, false),
    p!(47, "lfo2_wave", "LFO 2 Wave", Lfo2, 0, 5, 1, LFO_WAVES, false),
    p!(46, "lfo2_dest", "LFO 2 Dest", Lfo2, 1, 7, 5, LFO_DESTS, false),
    p!(48, "lfo2_speed", "LFO 2 Speed", Lfo2, 0, 127, 64, C, true),
    p!(49, "lfo2_depth", "LFO 2 Depth", Lfo2, 0, 127, 64, C, true),
    p!(69, "lfo2_sync", "LFO 2 Tempo Sync", Lfo2, 0, 1, 0, S, false),
    p!(70, "lfo2_key", "LFO 2 Key Sync", Lfo2, 0, 1, 0, S, false),
];

/// Look up a parameter by sy1 index.
pub fn by_sy1(sy1: usize) -> Option<&'static ParamDef> {
    static MAP: std::sync::OnceLock<[Option<u8>; NUM_SY1]> = std::sync::OnceLock::new();
    let m = MAP.get_or_init(|| {
        let mut m = [None; NUM_SY1];
        for (i, p) in PARAMS.iter().enumerate() {
            m[p.sy1 as usize] = Some(i as u8);
        }
        m
    });
    m.get(sy1).copied().flatten().map(|i| &PARAMS[i as usize])
}

/// A complete set of sy1 values (index = sy1 index).
pub type Values = [i32; NUM_SY1];

/// The "initial sound" template (filter amount stored as 128 in the format, clamped to 127).
pub fn template() -> Values {
    let mut v = [0; NUM_SY1];
    for p in PARAMS {
        v[p.sy1 as usize] = p.default;
    }
    v
}

impl ParamDef {
    pub fn clamp(&self, v: i32) -> i32 {
        match self.kind {
            Kind::Choice { values, .. } if self.sy1 == 86 || self.sy1 == 88 => {
                if values.contains(&v) {
                    v
                } else {
                    values[0]
                }
            }
            _ => v.clamp(self.min, self.max),
        }
    }

    /// For `Choice`, the UI position of a sy1 value.
    pub fn choice_index(&self, v: i32) -> usize {
        match self.kind {
            Kind::Choice { values, .. } => values.iter().position(|&x| x == v).unwrap_or(0),
            _ => 0,
        }
    }
}

// ------------------------------------------------------------------------------------------
// Version conversion
// ------------------------------------------------------------------------------------------

/// Index of the 0..127 value whose new-curve output is nearest to `target`.
fn nearest(target: f64, f: impl Fn(f64) -> f64) -> i32 {
    let mut best = 0;
    let mut err = f64::INFINITY;
    for n in 0..128 {
        let e = (f(n as f64) - target).abs();
        if e < err {
            err = e;
            best = n;
        }
    }
    best
}

fn fine_cents(n: f64) -> f64 {
    if n < 62.0 {
        n - 62.0
    } else if n <= 66.0 {
        0.0
    } else {
        n - 66.0
    }
}

/// Convert raw values of a file written by format `version` (<113) to the current curves.
/// Each version step applies in turn, quirks included, so old files load as they were made.
pub fn convert(v: &mut Values, version: i32) {
    if version >= CURRENT_VERSION {
        return;
    }
    let t = template();
    let s100 = version == 100;
    let s106 = s100 || version == 105 || version == 106;
    let s108 = s106 || version == 107 || version == 108;
    let s109 = s108 || version == 109;
    let s111 = s109 || version == 111;
    let orig = *v;
    let x = |i: usize| orig[i] as f64 / 127.0;
    let mut out = *v;

    if s100 {
        // osc2 fine: old ±1 semitone linear.
        let mut f = (x(3) - 0.5) * 2.0;
        for s in [-1.0, 0.0, 1.0] {
            if (f - s).abs() < 0.03 {
                f = s;
            }
        }
        let target = 2f64.powf(f / 12.0);
        out[3] = nearest(target, |n| 2f64.powf(fine_cents(n) / 1200.0));
        for i in [12, 15, 25] {
            let xi = x(i);
            let target = if xi < 0.5 { 1.75f64.powf(xi * xi) - 1.0 + 0.0005 } else { 30f64.powf(xi.powf(4.6)) - 1.0 };
            out[i] = nearest(target, |n| 0.0005 * 56000f64.powf(n / 127.0));
        }
        for i in [27, 29] {
            let target = x(i);
            out[i] = nearest(target, |n| if n == 0.0 { 0.0 } else { 2048f64.powf((n / 127.0).powf(0.3)) / 2048.0 });
        }
        {
            let target = 1.0 - 0.9 * x(30);
            out[30] = nearest(target, |n| 10f64.powf(-1.5 * n / 127.0));
        }
        for i in [43, 48] {
            let target = 20f64.powf(x(i)) - 0.983_33;
            out[i] = nearest(target, |n| 1500f64.powf(n / 127.0) / 12.0);
        }
        for i in 67..=72 {
            out[i] = t[i];
        }
    }
    if s106 {
        for i in [13, 16, 18, 26, 28] {
            let target = 60f64.powf(x(i).powf(2.95)) - 1.0;
            out[i] = nearest(target, |n| 0.01 * 6000f64.powf(n / 127.0));
        }
        out[21] = nearest(x(21), |n| (n - 63.0) / 64.0);
        out[37] = orig[37] / 2;
        {
            let xi = x(52);
            let target = if xi < 0.5 { 0.3 * 50f64.powf(2.0 * xi) } else { 30.0 * xi };
            out[52] = nearest(target, |n| {
                let y = n / 127.0;
                if y < 0.5 {
                    0.05 * 300f64.powf((2.0 * y).powf(0.6))
                } else {
                    30.0 * y
                }
            });
        }
        {
            let target = 10f64.powf(x(54).powf(1.8)) - 1.0 + 0.05;
            out[54] = nearest(target, |n| if n == 0.0 { 0.01 } else { 0.02 * 20000f64.powf(n / 127.0) });
        }
        {
            // Format quirk: trunc(-v/127) is 0 unless v == 127.
            let target = 0.9 * (-(orig[55] as f64) / 127.0).trunc();
            out[55] = nearest(target, |n| (n - 64.0) / 64.0 * 0.99);
        }
        for i in (75..=85).chain([90]) {
            out[i] = t[i];
        }
        out[84] = 64;
        out[85] = 24;
        for i in 86..=89 {
            out[i] = 0;
        }
    }
    if s108 {
        let sens = |v: i32, p: i32| 64 + ((1.0 - p as f64 / 127.0) * (v as f64 / 127.0) * 63.0) as i32;
        out[50] = sens(out[50], out[44]);
        out[51] = sens(out[51], out[43]);
        {
            let c = if out[38] == 0 { 100.0 } else { 180.0 };
            let e2 = std::f64::consts::E.powi(2);
            let d = out[75] as f64;
            out[75] = ((63.5 * (1.0 + 0.01 * (e2 - 1.0) * (d * c / 127.0 + 0.5)).ln()) as i32).min(127);
        }
        for i in [86, 87, 88, 89, 91, 92, 94] {
            out[i] = t[i];
        }
        out[93] = if out[38] == 0 { 2 } else { 4 };
    }
    if s109 {
        for i in [95, 96, 97] {
            out[i] = t[i];
        }
    }
    if s111 {
        out[98] = t[98];
    }
    *v = out;
}

/// Build a complete, clamped value set from sparse file entries (`None` = absent).
pub fn from_sparse(entries: &[Option<i32>], version: i32) -> Values {
    let mut v = template();
    for (i, e) in entries.iter().enumerate().take(NUM_SY1) {
        if let Some(x) = e {
            v[i] = *x;
        }
    }
    convert(&mut v, version);
    for p in PARAMS {
        let i = p.sy1 as usize;
        v[i] = p.clamp(v[i]);
    }
    v
}

// ------------------------------------------------------------------------------------------
// Display
// ------------------------------------------------------------------------------------------

fn fmt_time(s: f32) -> String {
    if s < 1.0 {
        format!("{:.0} ms", s * 1000.0)
    } else {
        format!("{s:.2} s")
    }
}

fn fmt_hz(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.2} kHz", f / 1000.0)
    } else if f >= 10.0 {
        format!("{f:.0} Hz")
    } else {
        format!("{f:.2} Hz")
    }
}

fn bipolar64(v: f32) -> f32 {
    if v < 64.0 {
        v / 64.0 - 1.0
    } else {
        (v - 64.0) / 63.0
    }
}

/// Human-readable value with units, for a raw sy1 value (`v` may be fractional for knobs).
pub fn display(sy1: usize, v: f32) -> String {
    use crate::{env, lfo};
    let x = (v / 127.0).clamp(0.0, 1.0);
    let Some(p) = by_sy1(sy1) else { return format!("{v:.0}") };
    if let Kind::Choice { labels, values } = p.kind {
        let vi = v.round() as i32;
        return values.iter().position(|&x| x == vi).map(|i| labels[i].to_string()).unwrap_or_else(|| format!("{vi}"));
    }
    if p.kind == Kind::Switch {
        return if v >= 0.5 { "on".into() } else { "off".into() };
    }
    match sy1 {
        12 | 15 | 25 => fmt_time(env::attack_time(v)),
        13 | 16 | 18 | 26 | 28 => fmt_time(env::decay_time(v)),
        19 => fmt_hz(crate::voice::cutoff_hz(v * 1023.0 / 127.0)),
        2 => format!("{:+} st", ((v * 120.0 / 127.0).floor() as i32) - 60),
        3 | 72 => format!("{:+.0} ct", fine_cents(v.round() as f64)),
        9 | 40 => format!("{:+}", v.round() as i32),
        11 | 21 => format!("{:+.0} %", bipolar64(v) * 100.0),
        27 | 29 => format!("{:.1} dB", if v <= 0.0 { -99.0 } else { 20.0 * (2048f32.powf(x.powf(0.3)) / 2048.0).log10() }),
        30 => format!("{:.0} dB", -30.0 * x),
        39 => {
            if v <= 0.0 {
                "off".into()
            } else {
                fmt_time(2.5 * x * (2.5 * (x - 1.0)).exp())
            }
        }
        43 | 48 => fmt_hz(lfo::free_rate(v)),
        52 => format!("{:.2} ms", if x < 0.5 { 0.05 * 300f32.powf((2.0 * x).powf(0.6)) } else { 30.0 * x }),
        54 => fmt_hz(if v <= 0.0 { 0.01 } else { 0.02 * 20000f32.powf(x) }),
        55 | 50 | 51 => format!("{:+.0} %", bipolar64(v) * 100.0),
        61 => fmt_hz(50.0 * 320f32.powf(x)),
        62 => format!("{:+.1} dB", (v - 64.0) / 127.0 * 50.0),
        63 => format!("Q {:.1}", 10f32.powf(x)),
        60 | 98 => {
            if (v - 64.0).abs() < 0.5 {
                "flat".into()
            } else if v < 64.0 {
                format!("lp {}", fmt_hz(200.0 * 110f32.powf(v / 64.0)))
            } else {
                format!("hp {}", fmt_hz(10.0 * 800f32.powf((v - 64.0) / 64.0)))
            }
        }
        90 => {
            let p = if v <= 64.0 { (v - 64.0) / 64.0 } else { (v - 64.0) / 63.0 };
            if p.abs() < 0.01 {
                "C".into()
            } else if p < 0.0 {
                format!("L {:.0}", -p * 100.0)
            } else {
                format!("R {:.0}", p * 100.0)
            }
        }
        75 => format!("{:.1} ct", crate::synth::unison_detune_cents(v)),
        76 => format!("{:.1} ct", x * 20.0),
        85 => format!("{:+} st", v.round() as i32 - 24),
        83 => {
            let d = v - 64.0;
            if d.abs() < 0.5 {
                "0 ms".into()
            } else {
                format!("{:+.1} ms", d.signum() * 1000f32.powf((d.abs() / 63.0).min(1.0)) * 0.1)
            }
        }
        87 | 89 => {
            let t = v.round() as i32;
            by_sy1(t.max(0) as usize).filter(|_| t >= 0).map(|p| p.name.to_string()).unwrap_or_else(|| "none".into())
        }
        _ if p.kind == Kind::Int => format!("{}", v.round() as i32),
        _ => format!("{:.0}", x * 100.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_index_once() {
        let mut seen = [false; NUM_SY1];
        for p in PARAMS {
            assert!(!seen[p.sy1 as usize], "dup {}", p.sy1);
            seen[p.sy1 as usize] = true;
            assert_eq!(p.clamp(p.default), p.default, "{}", p.id);
        }
        assert!(seen.iter().all(|&s| s), "missing indices");
        let mut ids: Vec<_> = PARAMS.iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), PARAMS.len());
    }

    #[test]
    fn filter_amount_unipolar_to_bipolar() {
        let mut v = template();
        v[21] = 0;
        convert(&mut v, 105);
        assert_eq!(v[21], 63);
        let mut v = template();
        v[21] = 127;
        convert(&mut v, 105);
        assert_eq!(v[21], 127);
    }

    #[test]
    fn current_version_untouched() {
        let mut v = template();
        v[13] = 5;
        convert(&mut v, 113);
        assert_eq!(v[13], 5);
    }

    #[test]
    fn displays_do_not_panic() {
        for p in PARAMS {
            for v in [p.min, p.default, p.max] {
                let _ = display(p.sy1 as usize, v as f32);
            }
        }
    }
}
