//! Description of OR Black Dist. Port indices and symbols are frozen
//! (see `ports.lock`).

use orx_dsp::Smoothing;
use orx_plugin::{Choice, Class, Face, Finish, KnobStyle, Param, PluginInfo, Port, Preset, Role, Unit, Version};

/// Clipping diodes, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voicing {
    /// Silicon diodes to ground: the original hard clipping
    Silicon = 0,
    /// LEDs to ground: clipping at about 1.7 V instead of 0.6 V, more
    /// open
    Led = 1,
    /// No diodes: the op-amp clips on its own rails, slew-limited
    None = 2,
}

impl Voicing {
    pub const ALL: [Voicing; 3] = [Voicing::Silicon, Voicing::Led, Voicing::None];

    pub(crate) fn from_value(v: i32) -> Self {
        match v {
            1 => Voicing::Led,
            2 => Voicing::None,
            _ => Voicing::Silicon,
        }
    }

    /// Output gain bringing the voicing to the silicon diodes' level at
    /// the middle of Distortion, dB (see the level test)
    pub fn compensation_db(self) -> f32 {
        match self {
            Voicing::Silicon => 0.0,
            Voicing::Led => -7.3,
            Voicing::None => -11.4,
        }
    }
}

const VOICINGS: &[Choice] = &[Choice::new(0.0, "Silicon"), Choice::new(1.0, "LED"), Choice::new(2.0, "None")];

const fn percent(default: f32) -> Param {
    Param::float(default, 0.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 30.0 })
}

const PRESETS: &[Preset] = &[
    Preset { name: "Grit", id: "grit", values: &[("distortion", 30.0), ("tone", 60.0)] },
    Preset { name: "Classic Black", id: "classic-black", values: &[("distortion", 60.0), ("tone", 50.0)] },
    Preset { name: "Wall", id: "wall", values: &[("distortion", 90.0), ("tone", 40.0), ("level", -2.0)] },
    Preset { name: "Open LED", id: "open-led", values: &[("distortion", 70.0), ("tone", 55.0), ("voicing", 1.0)] },
    Preset { name: "Raw Rails", id: "raw-rails", values: &[("distortion", 80.0), ("tone", 35.0), ("voicing", 2.0)] },
];

/// OR Black Dist
pub static BLACK_DIST: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/black-dist",
    name: "OR Black Dist",
    description: "A circuit model of the classic op-amp hard-clipping distortion: a high-gain op-amp stage with its gain-bandwidth and slew-rate limits, a choice of silicon, LED or no diodes to ground, the passive filter and the buffer, solved sample by sample at eight times the rate. Zero latency.",
    class: Class::Distortion,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param("distortion", "Distortion", percent(50.0).primary(1)).comment(
            "The 100 kΩ audio-taper pot in the op-amp's feedback: from unity to 67 dB of gain, the op-amp's bandwidth closing as it rises.",
        ),
        Port::param("tone", "Tone", percent(50.0).primary(2))
            .comment("The passive filter after the diodes, from 475 Hz to 32 kHz: up is brighter (the original's Filter turned down)."),
        Port::param(
            "level",
            "Level",
            Param::float(0.0, -24.0, 12.0)
                .unit(Unit::Db)
                .smooth(Smoothing::Linear { ms: 20.0 })
                .role(Role::OutputGain)
                .primary(3),
        )
        .comment("Output level; 0 dB is about the bypassed level at the middle of Distortion."),
        Port::param("voicing", "Diodes", Param::choice(0.0, VOICINGS).primary(4)).comment(
            "The clipping diodes to ground, level-matched at the middle of Distortion. Silicon: the original, tight. LED: clipping later, more open. None: the op-amp clips on its own rails, slew-limited, rawer.",
        ),
        Port::enabled(),
    ],
    presets: PRESETS,
    view: None,
    face: Face::signature(
        "BLACK",
        "Op-amp hard clipping, in circuit", "#1E1E21", KnobStyle::Domed, &["distortion", "tone", "level", "voicing"])
    .finish(Finish::Brushed)
    .wear(0.5),
    hard_rt: true,
};
