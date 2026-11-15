//! Descriptions of the two variants. Port indices and symbols are frozen
//! (see `ports-mono.lock` and `ports-stereo.lock`).

use orx_dsp::Smoothing;
use orx_plugin::tempo;
use orx_plugin::view::{StreamContent, View, ViewKind, Waveform};
use orx_plugin::{Choice, Class, Face, Meter, Param, PluginInfo, Port, Preset, Role, Stream, Unit, Version};

/// Modes, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Classic = 0,
    Opto = 1,
    Harmonic = 2,
    Pan = 3,
}

impl Mode {
    pub(crate) fn from_value(v: i32) -> Self {
        match v {
            1 => Mode::Opto,
            2 => Mode::Harmonic,
            3 => Mode::Pan,
            _ => Mode::Classic,
        }
    }
}

const MODES_MONO: &[Choice] = &[Choice::new(0.0, "Classic"), Choice::new(1.0, "Opto"), Choice::new(2.0, "Harmonic")];
const MODES_STEREO: &[Choice] =
    &[Choice::new(0.0, "Classic"), Choice::new(1.0, "Opto"), Choice::new(2.0, "Harmonic"), Choice::new(3.0, "Pan")];

const RATE: Port = Port::param(
    "rate",
    "Rate",
    Param::float(5.0, 0.1, 20.0)
        .unit(Unit::Hz)
        .log()
        .smooth(Smoothing::Multiplicative { ms: 50.0 })
        .role(Role::LfoRate)
        .primary(1),
)
.comment("LFO frequency when Sync is off.");
const SYNC_PORT: Port = Port::param("sync", "Sync", Param::choice(0.0, &tempo::CHOICES).role(Role::LfoTempoDivision))
    .comment("Note value of one LFO cycle at the host tempo; Off uses Rate.");
const DEPTH: Port = Port::param(
    "depth",
    "Depth",
    Param::float(50.0, 0.0, 100.0)
        .unit(Unit::Percent)
        .smooth(Smoothing::Linear { ms: 20.0 })
        .role(Role::LfoDepth)
        .primary(2),
);
const SHAPE: Port = Port::param(
    "shape",
    "Shape",
    Param::float(0.0, 0.0, 100.0)
        .unit(Unit::Percent)
        .smooth(Smoothing::Linear { ms: 20.0 })
        .role(Role::LfoShape)
        .primary(3),
)
.comment("From a sine (0 %) to a rounded square (100 %).");
const XOVER: Port = Port::param(
    "crossover",
    "Crossover",
    Param::float(800.0, 150.0, 2500.0)
        .unit(Unit::Hz)
        .log()
        .smooth(Smoothing::Multiplicative { ms: 30.0 })
        .role(Role::CrossoverFrequency),
)
.comment("Split between the bands of the harmonic mode.");
const OUTPUT: Port = Port::param(
    "output",
    "Output",
    Param::float(0.0, -12.0, 6.0).unit(Unit::Db).smooth(Smoothing::Linear { ms: 20.0 }).role(Role::OutputGain),
);
const PHASE: Port = Port::meter("lfo_phase", "LFO phase", Meter::new(0.0, 1.0).role(Role::LfoPhase));
const GAIN: Port = Port::meter("lfo_gain", "Modulation gain", Meter::new(0.0, 1.5).role(Role::ModulationGain))
    .comment("Gain applied now (the low band in harmonic mode).");
const EVENTS: Port = Port::events("events", "Events", false, true).comment("Host transport, for Sync.");

const VIEW: View = View::new(ViewKind::Lfo).waveform(Waveform::SineToSquare);

const PRESETS_MONO: &[Preset] = &[
    Preset {
        name: "Harmonic Vintage",
        id: "harmonic-vintage",
        values: &[("mode", 2.0), ("rate", 4.2), ("depth", 85.0), ("crossover", 650.0)],
    },
    Preset {
        name: "Opto Pulse",
        id: "opto-pulse",
        values: &[("mode", 1.0), ("rate", 5.5), ("depth", 75.0), ("shape", 40.0)],
    },
    Preset {
        name: "Surf Chop",
        id: "surf-chop",
        values: &[("mode", 0.0), ("rate", 7.5), ("depth", 100.0), ("shape", 85.0)],
    },
    Preset { name: "Slow Swell", id: "slow-swell", values: &[("mode", 0.0), ("rate", 1.2), ("depth", 60.0)] },
    Preset { name: "Helicopter 1/16", id: "helicopter", values: &[("sync", 10.0), ("depth", 100.0), ("shape", 100.0)] },
];

const PRESETS_STEREO: &[Preset] = &[
    Preset {
        name: "Harmonic Vintage",
        id: "harmonic-vintage",
        values: &[("mode", 2.0), ("rate", 4.2), ("depth", 85.0), ("crossover", 650.0)],
    },
    Preset {
        name: "Opto Pulse",
        id: "opto-pulse",
        values: &[("mode", 1.0), ("rate", 5.5), ("depth", 75.0), ("shape", 40.0)],
    },
    Preset { name: "Wide Pan", id: "wide-pan", values: &[("mode", 3.0), ("rate", 1.5), ("depth", 100.0)] },
    Preset {
        name: "Spread Chop",
        id: "spread-chop",
        values: &[("mode", 0.0), ("rate", 6.0), ("depth", 100.0), ("shape", 70.0), ("spread", 180.0)],
    },
];

const DESCRIPTION: &str = "Tremolo with three voicings: classic (linear amplitude), opto (the lag and soft curve of a lamp and photocell) and harmonic (low and high bands modulated in antiphase, as in early 1960s amps). The LFO morphs from a sine to a rounded square and can follow the host tempo.";

/// OR Tremolo (mono)
pub static MONO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/tremolo",
    name: "OR Tremolo",
    description: DESCRIPTION,
    class: Class::Modulator,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        RATE,
        SYNC_PORT,
        DEPTH,
        SHAPE,
        Port::param("mode", "Mode", Param::choice(0.0, MODES_MONO).primary(4)),
        XOVER,
        OUTPUT,
        Port::enabled(),
        PHASE,
        GAIN,
        EVENTS,
        Port::stream(
            "cycle",
            "Modulation cycle",
            Stream { content: StreamContent::ModulationCycle, length: 64, channels: 1, max_rate: 30.0 },
        ),
    ],
    presets: PRESETS_MONO,
    view: Some(VIEW),
    face: Face::basic(
        "Tempo-synced tremolo, sine to square",
        &["rate", "depth", "shape", "sync", "mode", "crossover", "output"],
    ),
    hard_rt: true,
};

/// OR Tremolo Stereo
pub static STEREO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/tremolo-stereo",
    name: "OR Tremolo Stereo",
    description: "Stereo version of OR Tremolo, with an LFO phase spread between the channels and an equal-power auto-pan mode.",
    class: Class::Modulator,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in_l", "In L"),
        Port::audio_in("in_r", "In R"),
        Port::audio_out("out_l", "Out L"),
        Port::audio_out("out_r", "Out R"),
        RATE,
        SYNC_PORT,
        DEPTH,
        SHAPE,
        Port::param("mode", "Mode", Param::choice(0.0, MODES_STEREO).primary(4)),
        XOVER,
        OUTPUT,
        Port::param(
            "spread",
            "Spread",
            Param::float(0.0, 0.0, 180.0)
                .unit(Unit::Degrees)
                .smooth(Smoothing::Linear { ms: 20.0 })
                .role(Role::StereoPhase),
        )
        .comment("LFO phase of the right channel after the left (not used by Pan)."),
        Port::enabled(),
        PHASE,
        GAIN,
        EVENTS,
        Port::stream(
            "cycle",
            "Modulation cycle",
            Stream { content: StreamContent::ModulationCycle, length: 64, channels: 2, max_rate: 30.0 },
        ),
    ],
    presets: PRESETS_STEREO,
    view: Some(VIEW),
    face: Face::basic(
        "Tempo-synced stereo tremolo, sine to square",
        &["rate", "depth", "shape", "sync", "mode", "crossover", "output", "spread"],
    ),
    hard_rt: true,
};
