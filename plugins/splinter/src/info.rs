//! Description of OR Splinter. Port indices and symbols are frozen (see
//! `ports.lock`).

use orx_dsp::Smoothing;
use orx_plugin::tempo;
use orx_plugin::view::{StreamContent, View, ViewKind};
use orx_plugin::{
    Choice, Class, Face, Finish, KnobStyle, Param, PlateStyle, PluginInfo, Port, Preset, Role, Stream, Unit, Version,
};

/// What HOLD does, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Capture a slice and repeat it
    Repeat = 0,
    /// Capture a slice and repeat it backward
    Reverse = 1,
    /// Slow the tape down to a stop
    TapeStop = 2,
    /// Nothing but the chop and the crush (when they apply while held)
    None = 3,
}

impl Action {
    pub(crate) fn from_value(v: i32) -> Self {
        match v {
            1 => Action::Reverse,
            2 => Action::TapeStop,
            3 => Action::None,
            _ => Action::Repeat,
        }
    }
}

/// When the chop pattern and the crush apply, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    WhileHeld = 0,
    Always = 1,
}

impl Scope {
    pub(crate) fn from_value(v: i32) -> Self {
        if v == 1 { Scope::Always } else { Scope::WhileHeld }
    }
}

const ACTIONS: &[Choice] =
    &[Choice::new(0.0, "Repeat"), Choice::new(1.0, "Reverse"), Choice::new(2.0, "Tape Stop"), Choice::new(3.0, "None")];

/// Slice lengths: the tempo note values from a quarter to a 32nd triplet
const SLICES: &[Choice] = &[
    tempo::choice(4),
    tempo::choice(6),
    tempo::choice(7),
    tempo::choice(9),
    tempo::choice(10),
    tempo::choice(11),
    tempo::choice(12),
    tempo::choice(13),
];

const SCOPES: &[Choice] = &[Choice::new(0.0, "While Held"), Choice::new(1.0, "Always")];

/// Chop patterns (0 is Off; the others index [`crate::pattern::PATTERNS`])
const PATTERNS: &[Choice] = &[
    Choice::new(0.0, "Off"),
    Choice::new(1.0, "1/8"),
    Choice::new(2.0, "1/16"),
    Choice::new(3.0, "Triplets"),
    Choice::new(4.0, "Gallop"),
    Choice::new(5.0, "Offbeat"),
    Choice::new(6.0, "Breakdown"),
];

/// Tempo range the plugin follows (host tempo or the Tempo control)
pub const BPM_MIN: f64 = 40.0;
pub const BPM_MAX: f64 = 300.0;
/// Longest slice, quarter notes (1/4)
pub const SLICE_MAX_QUARTERS: f64 = 1.0;
/// Longest tape stop, beats
pub const STOP_MAX_BEATS: f64 = 4.0;
/// Most repeats a press plays (0: as long as HOLD is held)
pub const REPEATS_MAX: i32 = 16;
/// Bits and rate at which the crush is off
pub const BITS_OFF: f32 = 16.0;
pub const RATE_OFF: f32 = 48_000.0;
/// Cells of the grid stream per bar (whole cells for 1/16, triplets of
/// 1/8 and 1/16, and 1/32)
pub const GRID_CELLS: usize = 96;

const PRESETS: &[Preset] = &[
    Preset { name: "Repeat 1/8", id: "repeat-8", values: &[("action", 0.0), ("slice", 7.0)] },
    Preset { name: "Machine Gun 1/32", id: "machine-gun", values: &[("action", 0.0), ("slice", 12.0)] },
    Preset {
        name: "Falling Stutter",
        id: "falling-stutter",
        values: &[("action", 0.0), ("slice", 10.0), ("repeats", 12.0), ("step", -1.0), ("decay", 1.5)],
    },
    Preset {
        name: "Rising Roll",
        id: "rising-roll",
        values: &[("action", 0.0), ("slice", 11.0), ("repeats", 12.0), ("step", 1.0)],
    },
    Preset { name: "Reverse Quarter", id: "reverse-quarter", values: &[("action", 1.0), ("slice", 4.0)] },
    Preset {
        name: "Tape Stop",
        id: "tape-stop",
        values: &[("action", 2.0), ("slice", 4.0), ("stop_time", 1.0), ("tape_start", 0.0)],
    },
    Preset {
        name: "Spin Down and Up",
        id: "spin-down-up",
        values: &[("action", 2.0), ("slice", 4.0), ("stop_time", 2.0), ("tape_start", 1.0)],
    },
    Preset {
        name: "Breakdown Chop",
        id: "breakdown-chop",
        values: &[("action", 3.0), ("pattern", 6.0), ("duty", 60.0), ("scope", 0.0)],
    },
    Preset {
        name: "Gallop Gate",
        id: "gallop-gate",
        values: &[("action", 3.0), ("pattern", 4.0), ("duty", 70.0), ("scope", 1.0)],
    },
    Preset {
        name: "Lo-Fi Crush",
        id: "lofi-crush",
        values: &[("action", 3.0), ("bits", 6.0), ("rate", 8000.0), ("scope", 1.0)],
    },
    Preset {
        name: "Breakcore Chaos",
        id: "breakcore-chaos",
        values: &[
            ("action", 0.0),
            ("slice", 10.0),
            ("chaos", 70.0),
            ("pattern", 2.0),
            ("duty", 75.0),
            ("bits", 8.0),
            ("rate", 12000.0),
            ("scope", 0.0),
        ],
    },
];

/// OR Splinter
pub static SPLINTER: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/splinter",
    name: "OR Splinter",
    description: "Tempo-synced glitch for live playing. HOLD (a footswitch) captures a slice of 1/4 to 1/32 on the beat and repeats it, forward or backward, with a pitch step and a decay per repeat; or slows the tape to a stop and spins it back up. A rhythmic chop with metal and breakdown patterns, a bit and sample-rate crush, and a chaos amount for breakcore. Follows the host tempo and bar, or its own tempo.",
    class: Class::Delay,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::events("events", "Events", false, true).comment("Host transport: tempo and position in the bar."),
        Port::param("hold", "Hold", Param::toggle(false).primary(1)).comment(
            "While on, the action plays: assign it to a momentary footswitch (or a latching one). The capture starts on the slice grid when Quantize is on.",
        ),
        Port::param("action", "Action", Param::choice(0.0, ACTIONS).primary(3)).comment(
            "What HOLD does: repeat the slice, repeat it backward, stop the tape, or nothing but the chop and the crush.",
        ),
        Port::param("slice", "Slice", Param::choice(7.0, SLICES).primary(2))
            .comment("Length of the slice captured, and the grid HOLD starts on."),
        Port::param("repeats", "Repeats", Param::int(0, 0, REPEATS_MAX))
            .comment("Repeats played after the slice, then back to the input; 0 repeats as long as HOLD is held."),
        Port::param("decay", "Decay", Param::float(0.0, 0.0, 12.0).unit(Unit::Db))
            .comment("Level lost by each repeat after the first."),
        Port::param("step", "Pitch Step", Param::int(0, -12, 12).unit(Unit::Semitones))
            .comment("Pitch change of each repeat after the first (the slice is played faster or slower)."),
        Port::param("stop_time", "Stop Time", Param::float(1.0, 0.25, STOP_MAX_BEATS as f32).unit(Unit::Beats))
            .comment("Time the tape takes to stop, in beats at the tempo."),
        Port::param("tape_start", "Tape Start", Param::toggle(true))
            .comment("On release, the tape spins back up in half the Stop Time instead of cutting back to the input."),
        Port::param("pattern", "Chop", Param::choice(0.0, PATTERNS))
            .comment("Rhythmic gate over one bar: straight notes, a gallop, offbeats or a breakdown pattern."),
        Port::param(
            "duty",
            "Duty",
            Param::float(50.0, 10.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 20.0 }),
        )
        .comment("Share of each step the chop lets through."),
        Port::param("scope", "Chop and Crush", Param::choice(0.0, SCOPES))
            .comment("When the chop and the crush apply: while HOLD is held, or always."),
        Port::param("bits", "Bits", Param::float(BITS_OFF, 1.0, BITS_OFF).smooth(Smoothing::Linear { ms: 20.0 }))
            .comment("Resolution of the crush; 16 is off."),
        Port::param("rate", "Rate", Param::float(RATE_OFF, 500.0, RATE_OFF).unit(Unit::Hz).log())
            .comment("Sample rate of the crush; the top of the range is off."),
        Port::param("chaos", "Chaos", Param::float(0.0, 0.0, 100.0).unit(Unit::Percent).primary(4)).comment(
            "Random changes to the repeats (reversed, an octave up or down, rolls, drop-outs) and to the chop steps.",
        ),
        Port::param(
            "mix",
            "Mix",
            Param::float(100.0, 0.0, 100.0)
                .unit(Unit::Percent)
                .smooth(Smoothing::Linear { ms: 20.0 })
                .role(Role::Mix)
                .primary(5),
        ),
        Port::param("quantize", "Quantize", Param::toggle(true)).comment(
            "Start HOLD on the slice grid (from the host bar, or from a running chop). Off, or with no grid, the press starts at once and is beat one.",
        ),
        Port::param("tempo", "Tempo", Param::float(120.0, BPM_MIN as f32, BPM_MAX as f32).unit(Unit::Bpm))
            .comment("Tempo when the host sends none."),
        Port::enabled(),
        Port::stream(
            "grid",
            "Step grid",
            Stream {
                content: StreamContent::StepGrid,
                length: GRID_CELLS as u32,
                channels: StreamContent::STEP_GRID_ROWS,
                max_rate: 30.0,
            },
        ),
    ],
    presets: PRESETS,
    view: Some(View::new(ViewKind::StepGrid)),
    face: Face::signature(
        "SPLINTER",
        "Stutter, tape stop and chop on the beat",
        "#3A3D42",
        KnobStyle::Skirted,
        &[
            "slice", "repeats", "step", "decay", "stop_time", "duty", "bits", "rate", "chaos", "mix", "action", "pattern",
            "scope", "tape_start", "quantize", "hold",
        ],
    )
    .finish(Finish::Brushed)
    .wear(0.7)
    .plate_style(PlateStyle::Glitch),
    hard_rt: true,
};
