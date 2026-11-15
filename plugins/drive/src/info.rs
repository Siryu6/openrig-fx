//! Description of OR Drive. Port indices and symbols are frozen (see
//! `ports.lock`).

use orx_dsp::Smoothing;
use orx_plugin::{Choice, Class, Face, Param, PluginInfo, Port, Preset, Role, Unit, Version};

/// Circuits, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Boost = 0,
    Overdrive = 1,
    Distortion = 2,
    Fuzz = 3,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Boost, Mode::Overdrive, Mode::Distortion, Mode::Fuzz];

    pub(crate) fn from_value(v: i32) -> Self {
        match v {
            1 => Mode::Overdrive,
            2 => Mode::Distortion,
            3 => Mode::Fuzz,
            _ => Mode::Boost,
        }
    }
}

const MODES: &[Choice] = &[
    Choice::new(0.0, "Boost"),
    Choice::new(1.0, "Overdrive"),
    Choice::new(2.0, "Distortion"),
    Choice::new(3.0, "Fuzz"),
];

const fn percent(default: f32) -> Param {
    Param::float(default, 0.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 30.0 })
}

const PRESETS: &[Preset] = &[
    Preset { name: "Clean Boost", id: "clean-boost", values: &[("mode", 0.0), ("drive", 60.0), ("tone", 55.0)] },
    Preset {
        name: "Edge of Breakup",
        id: "edge-of-breakup",
        values: &[("mode", 1.0), ("drive", 25.0), ("tone", 55.0), ("level", 2.0)],
    },
    Preset {
        name: "Mid Lead",
        id: "mid-lead",
        values: &[("mode", 1.0), ("drive", 70.0), ("tone", 60.0), ("bass", -3.0)],
    },
    Preset { name: "Crunch", id: "crunch", values: &[("mode", 2.0), ("drive", 45.0), ("tone", 60.0), ("bass", -2.0)] },
    Preset { name: "Saturated Lead", id: "saturated-lead", values: &[("mode", 2.0), ("drive", 85.0), ("tone", 45.0)] },
    Preset { name: "Wall of Fuzz", id: "wall-of-fuzz", values: &[("mode", 3.0), ("drive", 80.0), ("tone", 50.0)] },
];

/// OR Drive
pub static DRIVE: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/drive",
    name: "OR Drive",
    description: "Boost, overdrive, distortion and fuzz modelled on how their circuits behave, anti-aliased (4x oversampling and antiderivative clipping), with a tone control voiced per circuit. Zero latency.",
    class: Class::Distortion,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param("mode", "Mode", Param::choice(1.0, MODES).primary(4)).comment(
            "Boost: a clean gain stage that rounds off only when pushed. Overdrive: an op-amp with diodes in its feedback, the clean signal plus a clipped, mid-focused copy. Distortion: a high-gain op-amp clipped hard by diodes to ground, darker as the gain rises. Fuzz: a two-transistor stage clipping asymmetrically, thick with even harmonics. Changes crossfade.",
        ),
        Port::param("drive", "Drive", percent(50.0).primary(1)).comment("Gain into the clipping stage."),
        Port::param("tone", "Tone", percent(50.0).primary(2)).comment(
            "Voiced per circuit: a treble shelf (boost), a treble cut (overdrive), a low-pass filter (distortion), a blend from bass to treble through a mid scoop (fuzz).",
        ),
        Port::param(
            "bass",
            "Bass",
            Param::float(0.0, -12.0, 12.0).unit(Unit::Db).smooth(Smoothing::Linear { ms: 30.0 }),
        )
        .comment("Low shelf before the clipping: less bass tightens the drive, more thickens it."),
        Port::param(
            "level",
            "Level",
            Param::float(0.0, -24.0, 12.0)
                .unit(Unit::Db)
                .smooth(Smoothing::Linear { ms: 20.0 })
                .role(Role::OutputGain)
                .primary(3),
        )
        .comment("Output level; 0 dB is about the bypassed level at the middle of Drive, except for the boost, whose Drive is its gain."),
        Port::enabled(),
    ],
    presets: PRESETS,
    view: None,
    face: Face::basic("Boost, overdrive, distortion and fuzz", &["drive", "tone", "bass", "level", "mode"]),
    hard_rt: true,
};
