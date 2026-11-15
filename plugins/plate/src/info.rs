//! Description of OR Plate. Port indices and symbols are frozen (see
//! `ports.lock`).

use orx_dsp::Smoothing;
use orx_plugin::view::{View, ViewKind};
use orx_plugin::{Class, Face, Param, PluginInfo, Port, Preset, Role, Unit, Version};

/// Longest pre-delay, ms
pub const MAX_PREDELAY_MS: f32 = 250.0;
/// Shortest and longest mid decay (RT60), s
pub const MIN_DECAY_S: f32 = 0.3;
pub const MAX_DECAY_S: f32 = 20.0;
/// The ends of the wet filters' ranges that turn them off
pub const LOW_CUT_OFF: f32 = 20.0;
pub const HIGH_CUT_OFF: f32 = 20_000.0;

const PREDELAY: Port = Port::param(
    "predelay",
    "Pre-delay",
    Param::float(15.0, 0.0, MAX_PREDELAY_MS).unit(Unit::Ms).role(Role::PreDelay).primary(3),
)
.comment(
    "Gap between the dry note and the start of the reverb. A change crossfades to the new time, without a pitch bend.",
);
const DECAY: Port = Port::param(
    "decay",
    "Decay",
    Param::float(2.2, MIN_DECAY_S, MAX_DECAY_S)
        .unit(Unit::S)
        .log()
        .smooth(Smoothing::Multiplicative { ms: 100.0 })
        .role(Role::Decay)
        .primary(1),
)
.comment("Time the reverb takes to fall by 60 dB at middle frequencies.");
const LOW_DECAY: Port = Port::param(
    "low_decay",
    "Low decay",
    Param::float(0.8, 0.25, 2.0).smooth(Smoothing::Multiplicative { ms: 100.0 }).role(Role::DecayLow),
)
.comment("Decay of the lows as a multiple of Decay: below 1 keeps the low end tight.");
const HIGH_DECAY: Port = Port::param(
    "high_decay",
    "High decay",
    Param::float(0.5, 0.1, 1.0).smooth(Smoothing::Multiplicative { ms: 100.0 }).role(Role::DecayHigh).primary(4),
)
.comment("Damping: decay of the highs as a multiple of Decay. Lower is darker, 1 is a bright metal plate.");
const CROSSOVER_LOW: Port = Port::param(
    "crossover_low",
    "Low crossover",
    Param::float(300.0, 50.0, 1000.0)
        .unit(Unit::Hz)
        .log()
        .smooth(Smoothing::Multiplicative { ms: 100.0 })
        .role(Role::CrossoverLow),
)
.comment("Frequency below which Low decay applies.");
const CROSSOVER_HIGH: Port = Port::param(
    "crossover_high",
    "High crossover",
    Param::float(3500.0, 1000.0, 12_000.0)
        .unit(Unit::Hz)
        .log()
        .smooth(Smoothing::Multiplicative { ms: 100.0 })
        .role(Role::CrossoverHigh),
)
.comment("Frequency above which High decay applies.");
const LOW_CUT: Port = Port::param(
    "low_cut",
    "Low cut",
    Param::float(100.0, LOW_CUT_OFF, 1000.0).unit(Unit::Hz).log().smooth(Smoothing::Multiplicative { ms: 50.0 }),
)
.comment("High pass on the reverb only. Off at the minimum.");
const HIGH_CUT: Port = Port::param(
    "high_cut",
    "High cut",
    Param::float(HIGH_CUT_OFF, 1000.0, HIGH_CUT_OFF)
        .unit(Unit::Hz)
        .log()
        .smooth(Smoothing::Multiplicative { ms: 50.0 }),
)
.comment("Low pass on the reverb only. Off at the maximum.");
const MODULATION: Port = Port::param(
    "modulation",
    "Modulation",
    Param::float(30.0, 0.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 100.0 }),
)
.comment("Slow movement inside the plate: smooths metallic ringing, and adds a chorus-like shimmer when high.");
const MIX: Port = Port::param(
    "mix",
    "Mix",
    Param::float(25.0, 0.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 20.0 }).role(Role::Mix).primary(2),
)
.comment("Up to 50 % the dry signal stays at unity and the reverb rises; above, the dry signal falls, to reverb only at 100 %.");
const FREEZE: Port = Port::param("freeze", "Freeze", Param::toggle(false)).comment(
    "Holds the reverb as it is, endlessly, and lets no new sound in. Bypassing still lets it ring for up to 30 s.",
);

const VIEW: View = View::new(ViewKind::DecayCurve);

const PRESETS: &[Preset] = &[
    Preset {
        name: "Studio Plate",
        id: "studio",
        values: &[("decay", 2.2), ("predelay", 15.0), ("high_decay", 0.5), ("mix", 25.0)],
    },
    Preset {
        name: "Short Bright Plate",
        id: "short-bright",
        values: &[("decay", 0.9), ("predelay", 5.0), ("high_decay", 0.8), ("low_decay", 0.6), ("mix", 22.0)],
    },
    Preset {
        name: "Dark Plate",
        id: "dark",
        values: &[("decay", 3.5), ("predelay", 25.0), ("high_decay", 0.25), ("high_cut", 6000.0), ("mix", 28.0)],
    },
    Preset {
        name: "Huge Shimmering Plate",
        id: "huge",
        values: &[("decay", 9.0), ("predelay", 40.0), ("high_decay", 0.6), ("modulation", 60.0), ("mix", 35.0)],
    },
];

/// OR Plate
pub static PLATE: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/plate",
    name: "OR Plate",
    description: "Plate reverb: a dense, modulated figure-eight tank of all-pass and delay lines, with the decay set in seconds for the lows, the mids and the highs, pre-delay, filters on the reverb, freeze, and a tail that rings out when bypassed.",
    class: Class::Reverb,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in_l", "In L"),
        Port::audio_in("in_r", "In R"),
        Port::audio_out("out_l", "Out L"),
        Port::audio_out("out_r", "Out R"),
        PREDELAY,
        DECAY,
        LOW_DECAY,
        HIGH_DECAY,
        CROSSOVER_LOW,
        CROSSOVER_HIGH,
        LOW_CUT,
        HIGH_CUT,
        MODULATION,
        MIX,
        FREEZE,
        Port::enabled(),
    ],
    presets: PRESETS,
    view: Some(VIEW),
    face: Face::basic(
        "A plate reverb with a frozen tail",
        &[
            "decay",
            "mix",
            "predelay",
            "low_decay",
            "high_decay",
            "crossover_low",
            "crossover_high",
            "low_cut",
            "high_cut",
            "modulation",
            "freeze",
        ],
    ),
    hard_rt: true,
};
