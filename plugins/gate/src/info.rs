//! Description of OR Gate. Port indices and symbols are frozen (see
//! `ports.lock`).

use orx_plugin::view::DynamicsKind;
use orx_plugin::{Class, Face, Meter, Param, PluginInfo, Port, Preset, Role, Unit, Version, View, ViewKind};

/// Lowest threshold and deepest range, dB
pub const FLOOR_DB: f32 = -90.0;

const PRESETS: &[Preset] = &[
    Preset { name: "Natural", id: "natural", values: &[("threshold", -65.0), ("decay", 300.0), ("hold", 30.0)] },
    Preset { name: "High Gain", id: "high-gain", values: &[("threshold", -55.0), ("decay", 120.0), ("hold", 15.0)] },
    Preset { name: "Tight Riffs", id: "tight-riffs", values: &[("threshold", -48.0), ("decay", 40.0), ("hold", 5.0)] },
    Preset {
        name: "Soft Expander",
        id: "soft-expander",
        values: &[("threshold", -60.0), ("decay", 500.0), ("hold", 50.0), ("range", -18.0)],
    },
];

/// OR Gate
pub static GATE: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/gate",
    name: "OR Gate",
    description: "Noise gate for high-gain rigs: opens above the threshold in a millisecond, holds, then fades over the decay once the signal falls 6 dB below the threshold (no chatter on fading notes). Bit-transparent while open. Zero latency.",
    class: Class::Gate,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param(
            "threshold",
            "Threshold",
            Param::float(-60.0, FLOOR_DB, -20.0).unit(Unit::Db).role(Role::Threshold).primary(1),
        )
        .comment("Level that opens the gate; it closes 6 dB lower."),
        Port::param(
            "decay",
            "Decay",
            Param::float(150.0, 5.0, 2000.0).unit(Unit::Ms).log().role(Role::Release).primary(2),
        )
        .comment("Time the gate takes to close over its whole range."),
        Port::param("hold", "Hold", Param::float(20.0, 0.0, 500.0).unit(Unit::Ms).primary(4))
            .comment("Time the gate stays open after the signal falls below the closing level."),
        Port::param(
            "range",
            "Range",
            Param::float(FLOOR_DB, FLOOR_DB, 0.0).unit(Unit::Db).role(Role::Range).primary(3),
        )
        .comment("How far the closed gate lowers the signal; -90 dB mutes, a few dB is a gentle expander."),
        Port::meter("reduction", "Reduction", Meter::new(0.0, -FLOOR_DB).unit(Unit::Db).role(Role::GainReduction)),
        Port::enabled(),
    ],
    presets: PRESETS,
    view: Some(View::new(ViewKind::Dynamics).dynamics(DynamicsKind::Gate)),
    face: Face::basic("Silence between the notes", &["threshold", "range", "decay", "hold"]),
    hard_rt: true,
};
