//! Description of OR Green Drive. Port indices and symbols are frozen
//! (see `ports.lock`).

use orx_dsp::Smoothing;
use orx_plugin::{Choice, Class, Face, KnobStyle, Param, PluginInfo, Port, Preset, Role, Unit, Version};

/// Variants of the circuit, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voicing {
    /// The classic values
    Classic = 0,
    /// A larger capacitor in the gain leg: the clipped band reaches
    /// lower, a fuller low-mid
    Fat = 1,
    /// Two diodes in series on one side: clips later on that side, more
    /// output and even harmonics
    Asymmetric = 2,
}

impl Voicing {
    pub const ALL: [Voicing; 3] = [Voicing::Classic, Voicing::Fat, Voicing::Asymmetric];

    pub(crate) fn from_value(v: i32) -> Self {
        match v {
            1 => Voicing::Fat,
            2 => Voicing::Asymmetric,
            _ => Voicing::Classic,
        }
    }
}

const VOICINGS: &[Choice] = &[Choice::new(0.0, "Classic"), Choice::new(1.0, "Fat"), Choice::new(2.0, "Asymmetric")];

const fn percent(default: f32) -> Param {
    Param::float(default, 0.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 30.0 })
}

const PRESETS: &[Preset] = &[
    Preset { name: "Clean Push", id: "clean-push", values: &[("drive", 0.0), ("tone", 50.0), ("level", 6.0)] },
    Preset {
        name: "Tight Boost",
        id: "tight-boost",
        values: &[("drive", 0.0), ("tone", 55.0), ("level", 9.0), ("tight", 250.0)],
    },
    Preset { name: "Classic Green", id: "classic-green", values: &[("drive", 50.0), ("tone", 50.0)] },
    Preset { name: "Lead Hump", id: "lead-hump", values: &[("drive", 80.0), ("tone", 60.0)] },
    Preset { name: "Fat Rhythm", id: "fat-rhythm", values: &[("drive", 40.0), ("tone", 40.0), ("voicing", 1.0)] },
    Preset {
        name: "Asymmetric Crunch",
        id: "asymmetric-crunch",
        values: &[("drive", 70.0), ("tone", 55.0), ("voicing", 2.0), ("level", -2.0)],
    },
];

/// OR Green Drive
pub static GREEN_DRIVE: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/green-drive",
    name: "OR Green Drive",
    description: "A circuit model of the classic mid-hump op-amp overdrive: the diodes in the op-amp's feedback solved sample by sample at four times the rate, the 720 Hz gain leg, the tone network and the buffers, with three historic voicings, and a Tight high pass before the clipping for boosting high-gain amps. Zero latency.",
    class: Class::Distortion,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param("drive", "Drive", percent(50.0).primary(1))
            .comment("The 500 kΩ audio-taper pot in the op-amp's feedback: gain from 21 to 41 dB in the mids, less treble as it rises."),
        Port::param("tone", "Tone", percent(50.0).primary(2))
            .comment("The active tone network after the fixed 723 Hz low pass: from a treble cut to a treble boost."),
        Port::param(
            "level",
            "Level",
            Param::float(0.0, -24.0, 12.0)
                .unit(Unit::Db)
                .smooth(Smoothing::Linear { ms: 20.0 })
                .role(Role::OutputGain)
                .primary(3),
        )
        .comment("Output level; 0 dB is about the bypassed level at the middle of Drive."),
        Port::param("voicing", "Voicing", Param::choice(0.0, VOICINGS).primary(4)).comment(
            "Classic: the original values. Fat: a larger capacitor in the gain leg, the clipped band reaching lower. Asymmetric: two diodes in series on one side, more output and even harmonics.",
        ),
        Port::enabled(),
        Port::param(
            "tight",
            "Tight",
            Param::float(5.0, 5.0, 700.0).unit(Unit::Hz).log().smooth(Smoothing::Multiplicative { ms: 30.0 }),
        )
        .comment("OpenRig: a 12 dB/octave high pass before the clipping, from 5 Hz (out of the way) to 700 Hz. Around 150 to 300 Hz in front of a high-gain amp, it keeps palm mutes tight."),
    ],
    presets: PRESETS,
    view: None,
    face: Face::signature(
        "GREEN",
        "The mid-hump overdrive, in circuit", "#2E5A3A", KnobStyle::Skirted, &["drive", "tone", "tight", "level", "voicing"])
    .wear(0.35),
    hard_rt: true,
};
