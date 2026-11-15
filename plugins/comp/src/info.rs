//! Description of OR Comp. Port indices and symbols are frozen (see
//! `ports.lock`).

use orx_dsp::Smoothing;
use orx_plugin::view::DynamicsKind;
use orx_plugin::{Class, Face, Meter, Param, PluginInfo, Port, Preset, Role, Unit, Version, View, ViewKind};

/// Deepest gain reduction the meter shows, dB
pub const METER_DB: f32 = 40.0;

const PRESETS: &[Preset] = &[
    Preset {
        name: "Country Squash",
        id: "country-squash",
        values: &[("threshold", -38.0), ("ratio", 10.0), ("attack", 1.0), ("release", 250.0), ("level", 12.0)],
    },
    Preset {
        name: "Studio Smooth",
        id: "studio-smooth",
        values: &[("threshold", -26.0), ("ratio", 3.0), ("attack", 15.0), ("release", 200.0), ("level", 5.0)],
    },
    Preset {
        name: "Parallel Sustain",
        id: "parallel-sustain",
        values: &[
            ("threshold", -42.0),
            ("ratio", 20.0),
            ("attack", 3.0),
            ("release", 400.0),
            ("level", 16.0),
            ("mix", 50.0),
        ],
    },
    Preset {
        name: "Funk Snap",
        id: "funk-snap",
        values: &[("threshold", -30.0), ("ratio", 6.0), ("attack", 25.0), ("release", 80.0), ("level", 6.0)],
    },
];

/// OR Comp
pub static COMP: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/comp",
    name: "OR Comp",
    description: "Guitar compressor: threshold, ratio and soft knee in decibels, attack and release, makeup level and a blend for parallel compression, with a gain reduction meter. Zero latency.",
    class: Class::Compressor,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param(
            "threshold",
            "Threshold",
            Param::float(-30.0, -60.0, 0.0).unit(Unit::Db).role(Role::Threshold).primary(1),
        )
        .comment("Level above which the signal is compressed; lower it for more sustain."),
        Port::param("ratio", "Ratio", Param::float(4.0, 1.0, 20.0).log().role(Role::Ratio).primary(2))
            .comment("Decibels in for one decibel out above the threshold."),
        Port::param("knee", "Knee", Param::float(6.0, 0.0, 24.0).unit(Unit::Db).role(Role::Knee))
            .comment("Width of the soft knee around the threshold."),
        Port::param("attack", "Attack", Param::float(5.0, 0.1, 100.0).unit(Unit::Ms).log().role(Role::Attack))
            .comment("Time to clamp down; longer lets the pick attack through."),
        Port::param("release", "Release", Param::float(150.0, 10.0, 2000.0).unit(Unit::Ms).log().role(Role::Release))
            .comment("Time to let go once the level falls."),
        Port::param(
            "level",
            "Level",
            Param::float(0.0, 0.0, 30.0)
                .unit(Unit::Db)
                .smooth(Smoothing::Linear { ms: 20.0 })
                .role(Role::Makeup)
                .primary(3),
        )
        .comment("Makeup gain of the compressed signal."),
        Port::param(
            "mix",
            "Blend",
            Param::float(100.0, 0.0, 100.0)
                .unit(Unit::Percent)
                .smooth(Smoothing::Linear { ms: 20.0 })
                .role(Role::Mix)
                .primary(4),
        )
        .comment("Share of the compressed signal; the rest is the dry signal (parallel compression)."),
        Port::meter("reduction", "Reduction", Meter::new(0.0, METER_DB).unit(Unit::Db).role(Role::GainReduction)),
        Port::enabled(),
    ],
    presets: PRESETS,
    view: Some(View::new(ViewKind::Dynamics).dynamics(DynamicsKind::Compressor)),
    face: Face::basic(
        "Smooth guitar compression",
        &["threshold", "ratio", "knee", "attack", "release", "level", "mix"],
    ),
    hard_rt: true,
};
