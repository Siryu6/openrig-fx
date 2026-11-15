//! Descriptions of the two equalisers and their bands. Port indices and
//! symbols are frozen (see `ports-parametric.lock`, `ports-graphic.lock`).

use crate::design::{BandDef, curves};
use orx_dsp::Smoothing;
use orx_plugin::view::{Curve, FilterShape, Value, View, ViewKind};
use orx_plugin::{Class, Face, Param, PluginInfo, Port, Preset, Role, Unit, Version};

/// Gain range of every band, dB
pub const GAIN_DB: f32 = 15.0;
/// Q of the cut filters and the shelves: Butterworth, no overshoot
const FLAT_Q: f32 = core::f32::consts::FRAC_1_SQRT_2;
/// Q of the graphic bands: about an octave wide, so neighbours blend
pub const GRAPHIC_Q: f32 = 1.4;

const fn freq(default: f32, min: f32, max: f32) -> Param {
    Param::float(default, min, max).unit(Unit::Hz).log().smooth(Smoothing::Multiplicative { ms: 30.0 })
}

const fn gain() -> Param {
    Param::float(0.0, -GAIN_DB, GAIN_DB).unit(Unit::Db).smooth(Smoothing::Linear { ms: 30.0 })
}

const fn q(default: f32) -> Param {
    Param::float(default, 0.3, 8.0).log().smooth(Smoothing::Multiplicative { ms: 30.0 })
}

const fn level(param: Param) -> Port {
    Port::param("level", "Level", param.role(Role::OutputGain))
        .comment("Output level, to match the bypassed level after boosts or cuts.")
}

const LEVEL: Param = Param::float(0.0, -GAIN_DB, GAIN_DB).unit(Unit::Db).smooth(Smoothing::Linear { ms: 20.0 });

// Parametric

pub(crate) const PARAMETRIC_BANDS: [BandDef; 7] = [
    BandDef {
        shape: FilterShape::Highpass,
        frequency: Value::Port("low_cut"),
        gain: None,
        q: Value::Fixed(FLAT_Q),
        enable: Some("low_cut_on"),
    },
    BandDef {
        shape: FilterShape::LowShelf,
        frequency: Value::Port("low_freq"),
        gain: Some(Value::Port("low_gain")),
        q: Value::Fixed(FLAT_Q),
        enable: None,
    },
    BandDef {
        shape: FilterShape::Peak,
        frequency: Value::Port("low_mid_freq"),
        gain: Some(Value::Port("low_mid_gain")),
        q: Value::Port("low_mid_q"),
        enable: None,
    },
    BandDef {
        shape: FilterShape::Peak,
        frequency: Value::Port("mid_freq"),
        gain: Some(Value::Port("mid_gain")),
        q: Value::Port("mid_q"),
        enable: None,
    },
    BandDef {
        shape: FilterShape::Peak,
        frequency: Value::Port("high_mid_freq"),
        gain: Some(Value::Port("high_mid_gain")),
        q: Value::Port("high_mid_q"),
        enable: None,
    },
    BandDef {
        shape: FilterShape::HighShelf,
        frequency: Value::Port("high_freq"),
        gain: Some(Value::Port("high_gain")),
        q: Value::Fixed(FLAT_Q),
        enable: None,
    },
    BandDef {
        shape: FilterShape::Lowpass,
        frequency: Value::Port("high_cut"),
        gain: None,
        q: Value::Fixed(FLAT_Q),
        enable: Some("high_cut_on"),
    },
];
const PARAMETRIC_CURVES: [Curve; 7] = curves(&PARAMETRIC_BANDS);

const PARAMETRIC_PRESETS: &[Preset] = &[
    Preset {
        name: "Tighten",
        id: "tighten",
        values: &[("low_cut_on", 1.0), ("low_cut", 90.0), ("high_cut_on", 1.0), ("high_cut", 9000.0)],
    },
    Preset {
        name: "Mid Push",
        id: "mid-push",
        values: &[("mid_freq", 800.0), ("mid_gain", 5.0), ("mid_q", 0.8), ("level", -2.0)],
    },
    Preset {
        name: "Scooped",
        id: "scooped",
        values: &[("low_gain", 3.0), ("mid_freq", 700.0), ("mid_gain", -6.0), ("mid_q", 0.7), ("high_gain", 3.0)],
    },
    Preset {
        name: "De-Fizz",
        id: "de-fizz",
        values: &[
            ("high_mid_freq", 4500.0),
            ("high_mid_gain", -5.0),
            ("high_mid_q", 2.0),
            ("high_cut_on", 1.0),
            ("high_cut", 7000.0),
        ],
    },
];

/// OR EQ
pub static PARAMETRIC: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/eq",
    name: "OR EQ",
    description: "Parametric equaliser: low and high cut, low and high shelves and three bells, zero latency, smooth under automation. A host can draw its exact curve.",
    class: Class::ParaEq,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param("low_cut_on", "Low cut on", Param::toggle(false)),
        Port::param("low_cut", "Low cut", freq(80.0, 20.0, 400.0)).comment("High pass, 12 dB per octave."),
        Port::param("low_freq", "Low freq", freq(120.0, 40.0, 600.0)),
        Port::param("low_gain", "Low", gain().primary(1)).comment("Low shelf."),
        Port::param("low_mid_freq", "Low mid freq", freq(400.0, 100.0, 1500.0)),
        Port::param("low_mid_gain", "Low mid", gain()),
        Port::param("low_mid_q", "Low mid Q", q(1.0)),
        Port::param("mid_freq", "Mid freq", freq(900.0, 300.0, 4000.0)),
        Port::param("mid_gain", "Mid", gain().primary(2)),
        Port::param("mid_q", "Mid Q", q(1.0)),
        Port::param("high_mid_freq", "High mid freq", freq(2500.0, 1000.0, 10_000.0)),
        Port::param("high_mid_gain", "High mid", gain()),
        Port::param("high_mid_q", "High mid Q", q(1.0)),
        Port::param("high_freq", "High freq", freq(4500.0, 1000.0, 12_000.0)),
        Port::param("high_gain", "High", gain().primary(3)).comment("High shelf."),
        Port::param("high_cut_on", "High cut on", Param::toggle(false)),
        Port::param("high_cut", "High cut", freq(8000.0, 1500.0, 20_000.0)).comment("Low pass, 12 dB per octave."),
        level(LEVEL.primary(4)),
        Port::enabled(),
    ],
    presets: PARAMETRIC_PRESETS,
    view: Some(View::new(ViewKind::Eq).curves(&PARAMETRIC_CURVES)),
    face: Face::basic(
        "Five bands and two cuts, drawn as a curve",
        &[
            "low_cut_on",
            "low_cut",
            "low_freq",
            "low_gain",
            "low_mid_freq",
            "low_mid_gain",
            "low_mid_q",
            "mid_freq",
            "mid_gain",
            "mid_q",
            "high_mid_freq",
            "high_mid_gain",
            "high_mid_q",
            "high_freq",
            "high_gain",
            "high_cut_on",
            "high_cut",
            "level",
        ],
    ),
    hard_rt: true,
};

// Graphic

/// Centres of the graphic bands, Hz: octaves from 100 Hz, the layout of
/// the classic seven-band guitar graphic
pub const GRAPHIC_HZ: [f32; 7] = [100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0];
const GRAPHIC_SYMBOLS: [&str; 7] = ["g100", "g200", "g400", "g800", "g1600", "g3200", "g6400"];

const fn graphic_band(i: usize) -> BandDef {
    BandDef {
        shape: FilterShape::Peak,
        frequency: Value::Fixed(GRAPHIC_HZ[i]),
        gain: Some(Value::Port(GRAPHIC_SYMBOLS[i])),
        q: Value::Fixed(GRAPHIC_Q),
        enable: None,
    }
}

pub(crate) const GRAPHIC_BANDS: [BandDef; 7] = [
    graphic_band(0),
    graphic_band(1),
    graphic_band(2),
    graphic_band(3),
    graphic_band(4),
    graphic_band(5),
    graphic_band(6),
];
const GRAPHIC_CURVES: [Curve; 7] = curves(&GRAPHIC_BANDS);

const fn slider(i: usize, label: &'static str) -> Port {
    Port::param(GRAPHIC_SYMBOLS[i], label, gain())
}

const GRAPHIC_PRESETS: &[Preset] = &[
    Preset {
        name: "Mid Hump",
        id: "mid-hump",
        values: &[("g400", 2.0), ("g800", 5.0), ("g1600", 3.0), ("g6400", -2.0), ("level", -2.0)],
    },
    Preset {
        name: "Smile",
        id: "smile",
        values: &[("g100", 4.0), ("g200", 2.0), ("g800", -4.0), ("g3200", 2.0), ("g6400", 4.0)],
    },
    Preset {
        name: "Lead Boost",
        id: "lead-boost",
        values: &[("g100", -3.0), ("g800", 3.0), ("g1600", 4.0), ("g3200", 1.0), ("level", 4.0)],
    },
];

/// OR Graphic EQ
pub static GRAPHIC: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/graphic-eq",
    name: "OR Graphic EQ",
    description: "Seven-band graphic equaliser for guitar, one band per octave from 100 Hz to 6.4 kHz, with an output level. Zero latency; a host can draw its exact curve.",
    class: Class::Eq,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        slider(0, "100 Hz"),
        slider(1, "200 Hz"),
        slider(2, "400 Hz"),
        slider(3, "800 Hz"),
        slider(4, "1.6 kHz"),
        slider(5, "3.2 kHz"),
        slider(6, "6.4 kHz"),
        level(LEVEL),
        Port::enabled(),
    ],
    presets: GRAPHIC_PRESETS,
    view: Some(View::new(ViewKind::Eq).curves(&GRAPHIC_CURVES)),
    face: Face::basic(
        "Seven bands an octave apart",
        &["g100", "g200", "g400", "g800", "g1600", "g3200", "g6400", "level"],
    ),
    hard_rt: true,
};
