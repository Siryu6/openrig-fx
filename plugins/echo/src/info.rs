//! Descriptions of the two variants. Port indices and symbols are frozen
//! (see `ports-mono.lock` and `ports-stereo.lock`).

use orx_dsp::Smoothing;
use orx_plugin::view::{View, ViewKind};
use orx_plugin::{Choice, Class, Face, Meter, Param, PluginInfo, Port, Preset, Role, Unit, Version, tempo};

/// Shortest and longest delay time of the knob, ms
pub const MIN_MS: f32 = 20.0;
pub const MAX_MS: f32 = 2000.0;
/// Longest delay a tempo division may ask for, ms (a whole note at 60 BPM)
pub const MAX_SYNC_MS: f32 = 4000.0;
/// Lowest high cut and highest low cut setting; the other end of each
/// range turns the filter off
pub const LOW_CUT_OFF: f32 = 20.0;
pub const HIGH_CUT_OFF: f32 = 20_000.0;

/// Voicings, in choice order
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Digital = 0,
    Tape = 1,
    Analog = 2,
}

impl Mode {
    #[cfg(test)]
    pub(crate) const ALL: [Mode; 3] = [Mode::Digital, Mode::Tape, Mode::Analog];

    pub(crate) fn from_value(v: i32) -> Self {
        match v {
            1 => Mode::Tape,
            2 => Mode::Analog,
            _ => Mode::Digital,
        }
    }
}

const MODES: &[Choice] = &[Choice::new(0.0, "Digital"), Choice::new(1.0, "Tape"), Choice::new(2.0, "Analog")];

const TIME: Port = Port::param(
    "time",
    "Time",
    Param::float(400.0, MIN_MS, MAX_MS).unit(Unit::Ms).log().role(Role::DelayTime).primary(1),
)
.comment("Time between echoes when Sync is off. Changes crossfade (digital) or glide like a tape motor or a BBD clock (tape, analog).");
const SYNC: Port = Port::param("sync", "Sync", Param::choice(0.0, &tempo::CHOICES).role(Role::DelayTempoDivision))
    .comment("Note value of the delay at the host tempo; Off uses Time.");
const FEEDBACK: Port = Port::param(
    "feedback",
    "Feedback",
    Param::float(35.0, 0.0, 110.0)
        .unit(Unit::Percent)
        .smooth(Smoothing::Linear { ms: 20.0 })
        .role(Role::Feedback)
        .primary(2),
)
.comment("Share of each echo fed into the next. Above 100 % the echoes build up into a runaway, held by the saturation or the loop limiter.");
const LEVEL: Port = Port::param(
    "level",
    "Level",
    Param::float(40.0, 0.0, 100.0)
        .unit(Unit::Percent)
        .smooth(Smoothing::Linear { ms: 20.0 })
        .role(Role::WetLevel)
        .primary(3),
)
.comment("Level of the echoes added to the dry signal, which stays at unity.");
const MODE: Port = Port::param("mode", "Mode", Param::choice(0.0, MODES).primary(4)).comment(
    "Digital: clean repeats. Tape: darkening, saturating repeats with wow and flutter. Analog: a bucket-brigade line, darker as the time grows, with a softer clip and a chorus-like modulation.",
);
const LOW_CUT: Port = Port::param(
    "low_cut",
    "Low cut",
    Param::float(LOW_CUT_OFF, LOW_CUT_OFF, 1000.0).unit(Unit::Hz).log().smooth(Smoothing::Multiplicative { ms: 50.0 }),
)
.comment("High pass in the feedback loop: each repeat gets thinner. Off at the minimum.");
const HIGH_CUT: Port = Port::param(
    "high_cut",
    "High cut",
    Param::float(HIGH_CUT_OFF, 1000.0, HIGH_CUT_OFF)
        .unit(Unit::Hz)
        .log()
        .smooth(Smoothing::Multiplicative { ms: 50.0 }),
)
.comment("Low pass in the feedback loop: each repeat gets darker. Off at the maximum.");
const MODULATION: Port = Port::param(
    "modulation",
    "Modulation",
    Param::float(20.0, 0.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 100.0 }),
)
.comment("Wow and flutter on tape, a slow chorus-like sweep on analog and digital.");
const DRIVE: Port = Port::param(
    "drive",
    "Drive",
    Param::float(33.0, 0.0, 100.0).unit(Unit::Percent).smooth(Smoothing::Linear { ms: 30.0 }),
)
.comment("Saturation of the tape or bucket-brigade line, from −6 to +12 dB into it (no effect on digital).");
const DUCK: Port =
    Port::param("duck", "Duck", Param::float(0.0, 0.0, 30.0).unit(Unit::Db).smooth(Smoothing::Linear { ms: 20.0 }))
        .comment(
            "How far the echoes dip while the input is above the duck threshold; they come back when you stop playing.",
        );
const DUCK_THRESHOLD: Port =
    Port::param("duck_threshold", "Duck threshold", Param::float(-30.0, -60.0, 0.0).unit(Unit::Db))
        .comment("Input level, dBFS, from which ducking takes effect.");
const DRY: Port = Port::param("dry", "Dry", Param::toggle(true))
    .comment("Off: echoes only, for a parallel loop. The bypass still passes the dry signal.");
const DELAY_MS: Port =
    Port::meter("delay_ms", "Delay", Meter::new(0.0, MAX_SYNC_MS).unit(Unit::Ms).role(Role::CurrentDelayTime))
        .comment("Time between echoes playing now.");
const DUCKING: Port = Port::meter("ducking", "Ducking", Meter::new(0.0, 30.0).unit(Unit::Db).role(Role::GainReduction))
    .comment("How far the echoes are lowered now, dB.");
const EVENTS: Port = Port::events("events", "Events", false, true).comment("Host transport, for Sync.");
const PING_PONG: Port = Port::param("ping_pong", "Ping-pong", Param::toggle(false).role(Role::PingPong))
    .comment("Echoes of both inputs alternate between left and right.");

const VIEW: View = View::new(ViewKind::EchoTimeline);

const PRESETS_MONO: &[Preset] = &[
    Preset {
        name: "Slapback",
        id: "slapback",
        values: &[("mode", 1.0), ("time", 110.0), ("feedback", 10.0), ("level", 55.0), ("modulation", 15.0)],
    },
    Preset {
        name: "Tape Echo",
        id: "tape-echo",
        values: &[
            ("mode", 1.0),
            ("time", 380.0),
            ("feedback", 45.0),
            ("level", 45.0),
            ("modulation", 35.0),
            ("drive", 55.0),
            ("low_cut", 120.0),
        ],
    },
    Preset {
        name: "Analog Dotted Eighth",
        id: "analog-dotted",
        values: &[("mode", 2.0), ("sync", 8.0), ("feedback", 40.0), ("level", 45.0), ("modulation", 30.0)],
    },
    Preset {
        name: "Ducked Lead",
        id: "ducked-lead",
        values: &[
            ("mode", 0.0),
            ("time", 450.0),
            ("feedback", 40.0),
            ("level", 60.0),
            ("high_cut", 5000.0),
            ("duck", 15.0),
            ("duck_threshold", -36.0),
        ],
    },
    Preset {
        name: "Runaway",
        id: "runaway",
        values: &[("mode", 1.0), ("time", 300.0), ("feedback", 105.0), ("level", 40.0), ("drive", 70.0)],
    },
];

const PRESETS_STEREO: &[Preset] = &[
    Preset {
        name: "Tape Echo",
        id: "tape-echo",
        values: &[
            ("mode", 1.0),
            ("time", 380.0),
            ("feedback", 45.0),
            ("level", 45.0),
            ("modulation", 35.0),
            ("drive", 55.0),
            ("low_cut", 120.0),
        ],
    },
    Preset {
        name: "Ping-Pong Quarter",
        id: "ping-pong",
        values: &[("mode", 0.0), ("sync", 4.0), ("feedback", 45.0), ("level", 45.0), ("ping_pong", 1.0)],
    },
    Preset {
        name: "Wide Analog",
        id: "wide-analog",
        values: &[("mode", 2.0), ("time", 520.0), ("feedback", 50.0), ("level", 40.0), ("modulation", 60.0)],
    },
    Preset {
        name: "Ducked Lead",
        id: "ducked-lead",
        values: &[
            ("mode", 0.0),
            ("time", 450.0),
            ("feedback", 40.0),
            ("level", 60.0),
            ("high_cut", 5000.0),
            ("duck", 15.0),
            ("duck_threshold", -36.0),
            ("ping_pong", 1.0),
        ],
    },
];

const DESCRIPTION: &str = "Echo with three voicings: digital (clean repeats, crossfaded time changes), tape (darkening, saturating repeats with wow and flutter; time changes glide like a tape motor) and analog (a bucket-brigade line that darkens as the time grows). Tempo sync with note values, filters in the feedback loop, ducking, and trails that ring out when bypassed.";

/// OR Echo (mono)
pub static MONO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/echo",
    name: "OR Echo",
    description: DESCRIPTION,
    class: Class::Delay,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        TIME,
        SYNC,
        FEEDBACK,
        LEVEL,
        MODE,
        LOW_CUT,
        HIGH_CUT,
        MODULATION,
        DRIVE,
        DUCK,
        DUCK_THRESHOLD,
        DRY,
        Port::enabled(),
        DELAY_MS,
        DUCKING,
        EVENTS,
    ],
    presets: PRESETS_MONO,
    view: Some(VIEW),
    face: Face::basic(
        "Tempo-synced echo, clean to worn",
        &[
            "time",
            "sync",
            "feedback",
            "level",
            "mode",
            "low_cut",
            "high_cut",
            "modulation",
            "drive",
            "duck",
            "duck_threshold",
            "dry",
        ],
    ),
    hard_rt: true,
};

/// OR Echo Stereo
pub static STEREO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/echo-stereo",
    name: "OR Echo Stereo",
    description: "Stereo version of OR Echo: a line per channel, or ping-pong echoes alternating between the sides, with the modulation spread across them on analog and digital.",
    class: Class::Delay,
    version: Version::parse("0.2.0"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in_l", "In L"),
        Port::audio_in("in_r", "In R"),
        Port::audio_out("out_l", "Out L"),
        Port::audio_out("out_r", "Out R"),
        TIME,
        SYNC,
        FEEDBACK,
        LEVEL,
        MODE,
        LOW_CUT,
        HIGH_CUT,
        MODULATION,
        DRIVE,
        DUCK,
        DUCK_THRESHOLD,
        DRY,
        PING_PONG,
        Port::enabled(),
        DELAY_MS,
        DUCKING,
        EVENTS,
    ],
    presets: PRESETS_STEREO,
    view: Some(VIEW),
    face: Face::basic(
        "Tempo-synced stereo echo, clean to worn",
        &[
            "time",
            "sync",
            "feedback",
            "level",
            "mode",
            "low_cut",
            "high_cut",
            "modulation",
            "drive",
            "duck",
            "duck_threshold",
            "dry",
            "ping_pong",
        ],
    ),
    hard_rt: true,
};
