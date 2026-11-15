//! Description of OR Drop. Port indices and symbols are frozen (see
//! `ports.lock`).

use orx_dsp::Smoothing;
use orx_plugin::view::{View, ViewKind};
use orx_plugin::{Choice, Class, Face, KnobStyle, Param, PluginInfo, Port, Preset, Role, Unit, Version};

/// Latency against splice quality, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 6 ms: whole-period splices down to about 85 Hz
    Tight = 0,
    /// 14 ms: whole-period splices down to 36 Hz
    Quality = 1,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Tight, Mode::Quality];

    pub(crate) fn from_value(v: i32) -> Self {
        if v == 1 { Mode::Quality } else { Mode::Tight }
    }
}

const MODES: &[Choice] = &[Choice::new(0.0, "Tight"), Choice::new(1.0, "Quality")];

/// Lowest and highest shift, semitones
pub const SHIFT_MIN: i32 = -24;
pub const SHIFT_MAX: i32 = 12;

const PRESETS: &[Preset] = &[
    Preset { name: "Drop D", id: "drop-d", values: &[("shift", -2.0)] },
    Preset { name: "Half Down", id: "half-down", values: &[("shift", -1.0)] },
    Preset { name: "Drop C", id: "drop-c", values: &[("shift", -4.0)] },
    Preset { name: "Drop A", id: "drop-a", values: &[("shift", -7.0), ("mode", 1.0)] },
    Preset { name: "Octave Down", id: "octave-down", values: &[("shift", -12.0), ("mode", 1.0)] },
    Preset { name: "Octave Blend", id: "octave-blend", values: &[("shift", -12.0), ("mix", 50.0), ("mode", 1.0)] },
    Preset {
        name: "Dive Bomb",
        id: "dive-bomb",
        values: &[("shift", 0.0), ("dive_to", -24.0), ("dive_time", 600.0), ("mode", 1.0)],
    },
    Preset {
        name: "Octave Rise",
        id: "octave-rise",
        values: &[("shift", 0.0), ("dive_to", 12.0), ("dive_time", 100.0)],
    },
];

/// OR Drop
pub static DROP: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/drop",
    name: "OR Drop",
    description: "Down-tuning, octaves and momentary dives in the time domain: the input read from a delay line at the pitch ratio, spliced one detected period at a time with short crossfades, so the output is always the guitar's own waveform. Tight mode at 6 ms of latency, Quality mode at 14 ms for the lowest strings of extended-range guitars. The latency is reported to the host.",
    class: Class::Pitch,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param(
            "shift",
            "Shift",
            Param::int(-2, SHIFT_MIN, SHIFT_MAX).unit(Unit::Semitones).role(Role::PitchShift).primary(1),
        )
        .comment("Pitch shift in semitones: −2 for drop D from standard, −12 an octave down, up to +12."),
        Port::param(
            "mix",
            "Mix",
            Param::float(100.0, 0.0, 100.0)
                .unit(Unit::Percent)
                .smooth(Smoothing::Linear { ms: 30.0 })
                .role(Role::Mix)
                .primary(2),
        )
        .comment("Dry and shifted signals: 100 % for a down-tuning, around 50 % for an octave blend."),
        Port::param("mode", "Mode", Param::choice(0.0, MODES).primary(3)).comment(
            "Tight: 6 ms of latency; notes down to about 85 Hz are spliced on whole periods, lower ones on the best shorter lag. Quality: 14 ms; whole periods down to 36 Hz, for 7- and 8-string guitars and octaves down.",
        ),
        Port::param("dive", "Dive", Param::toggle(false).primary(4)).comment(
            "While on, the pitch glides to Dive To over Dive Time, and glides back when off. Assign it to a momentary footswitch for dive bombs and pitch rises.",
        ),
        Port::param("dive_to", "Dive To", Param::int(-12, SHIFT_MIN, SHIFT_MAX).unit(Unit::Semitones))
            .comment("Shift reached while Dive is on, semitones."),
        Port::param(
            "dive_time",
            "Dive Time",
            Param::float(250.0, 10.0, 2000.0).unit(Unit::Ms).log(),
        )
        .comment("Time of the glide to Dive To and back."),
        Port::enabled(),
        Port::latency(),
    ],
    presets: PRESETS,
    view: Some(View::new(ViewKind::Pitch)),
    face: Face::signature(
        "DROP",
        "Down-tuning and dives in the time domain",
        "#4A1418",
        KnobStyle::Chickenhead,
        &["shift", "mix", "mode", "dive_to", "dive_time", "dive"],
    )
    .wear(0.6),
    hard_rt: true,
};
