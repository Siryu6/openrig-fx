//! Static description of a plugin. Everything here is `const`-constructible
//! so a plugin's description is plain data, readable by the TTL generator,
//! the standalone host and the tests without creating an instance.

use orx_dsp::Smoothing;

use crate::view::{Face, Role, View};

/// Version of a plugin, LV2 style: the major version is part of the URI
/// (a new major is a new plugin); `minor` and `micro` follow the crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub minor: u32,
    pub micro: u32,
}

impl Version {
    /// Parse "MAJOR.MINOR.MICRO" (as `env!("CARGO_PKG_VERSION")`) at compile
    /// time; pre-release suffixes are ignored
    pub const fn parse(v: &str) -> Self {
        let b = v.as_bytes();
        let mut parts = [0u32; 3];
        let (mut i, mut p) = (0, 0);
        while i < b.len() && p < 3 {
            let c = b[i];
            if c == b'.' {
                p += 1;
            } else if c.is_ascii_digit() {
                parts[p] = parts[p] * 10 + (c - b'0') as u32;
            } else {
                break;
            }
            i += 1;
        }
        Self { minor: parts[1], micro: parts[2] }
    }
}

/// LV2 plugin class (`lv2:…Plugin`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Generic,
    Amplifier,
    Analyser,
    Chorus,
    Compressor,
    Delay,
    Distortion,
    Dynamics,
    Eq,
    Filter,
    Flanger,
    Gate,
    Limiter,
    Mixer,
    Modulator,
    ParaEq,
    Phaser,
    Pitch,
    Reverb,
    Simulator,
    Spatial,
    Spectral,
    Utility,
    Waveshaper,
}

impl Class {
    /// Local name in the `lv2:` namespace
    pub const fn lv2_name(self) -> &'static str {
        match self {
            Class::Generic => "Plugin",
            Class::Amplifier => "AmplifierPlugin",
            Class::Analyser => "AnalyserPlugin",
            Class::Chorus => "ChorusPlugin",
            Class::Compressor => "CompressorPlugin",
            Class::Delay => "DelayPlugin",
            Class::Distortion => "DistortionPlugin",
            Class::Dynamics => "DynamicsPlugin",
            Class::Eq => "EQPlugin",
            Class::Filter => "FilterPlugin",
            Class::Flanger => "FlangerPlugin",
            Class::Gate => "GatePlugin",
            Class::Limiter => "LimiterPlugin",
            Class::Mixer => "MixerPlugin",
            Class::Modulator => "ModulatorPlugin",
            Class::ParaEq => "ParaEQPlugin",
            Class::Phaser => "PhaserPlugin",
            Class::Pitch => "PitchPlugin",
            Class::Reverb => "ReverbPlugin",
            Class::Simulator => "SimulatorPlugin",
            Class::Spatial => "SpatialPlugin",
            Class::Spectral => "SpectralPlugin",
            Class::Utility => "UtilityPlugin",
            Class::Waveshaper => "WaveshaperPlugin",
        }
    }
}

/// Unit of a value (`units:` vocabulary)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    None,
    Db,
    Hz,
    Ms,
    S,
    Percent,
    Semitones,
    Cents,
    Bpm,
    /// Beats (quarter notes at the tempo)
    Beats,
    Degrees,
    Octaves,
    Frames,
    /// Musical note number (MIDI, 69 = A4)
    Midi,
}

impl Unit {
    /// Local name in the `units:` namespace (None: no unit)
    pub const fn lv2_name(self) -> Option<&'static str> {
        Some(match self {
            Unit::None => return None,
            Unit::Db => "db",
            Unit::Hz => "hz",
            Unit::Ms => "ms",
            Unit::S => "s",
            Unit::Percent => "pc",
            Unit::Semitones => "semitone12TET",
            Unit::Cents => "cent",
            Unit::Bpm => "bpm",
            Unit::Beats => "beat",
            Unit::Degrees => "degree",
            Unit::Octaves => "oct",
            Unit::Frames => "frame",
            Unit::Midi => "midiNote",
        })
    }

    /// Short symbol for displays
    pub const fn symbol(self) -> &'static str {
        match self {
            Unit::None => "",
            Unit::Db => "dB",
            Unit::Hz => "Hz",
            Unit::Ms => "ms",
            Unit::S => "s",
            Unit::Percent => "%",
            Unit::Semitones => "st",
            Unit::Cents => "ct",
            Unit::Bpm => "BPM",
            Unit::Beats => "beats",
            Unit::Degrees => "°",
            Unit::Octaves => "oct",
            Unit::Frames => "smp",
            Unit::Midi => "note",
        }
    }
}

/// One choice of a [`ParamKind::Choice`] parameter
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub label: &'static str,
    pub value: f32,
}

impl Choice {
    pub const fn new(value: f32, label: &'static str) -> Self {
        Self { label, value }
    }
}

/// What values a parameter takes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamKind {
    /// Any value in the range
    Float,
    /// Whole numbers in the range
    Int,
    /// Off (0) or on (1)
    Toggle,
    /// One of a list of labelled values (`lv2:enumeration`)
    Choice(&'static [Choice]),
}

/// A control input
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    pub default: f32,
    pub min: f32,
    pub max: f32,
    pub unit: Unit,
    pub kind: ParamKind,
    /// Displayed and mapped on a log scale (`pprops:logarithmic`)
    pub log: bool,
    /// How the plugin smooths it (documentation for hosts, and the spec
    /// the plugin builds its smoother from)
    pub smoothing: Smoothing,
    /// What it does, for native views (`openrig:role`)
    pub role: Option<Role>,
    /// Rank among the main controls, 1 first (`openrig:primary`): the knob
    /// strip and footswitch defaults use the first four
    pub primary: Option<u8>,
    /// Number of steps for stepped encoders (`pprops:rangeSteps`)
    pub steps: Option<u32>,
}

impl Param {
    /// A continuous parameter
    pub const fn float(default: f32, min: f32, max: f32) -> Self {
        Self {
            default,
            min,
            max,
            unit: Unit::None,
            kind: ParamKind::Float,
            log: false,
            smoothing: Smoothing::None,
            role: None,
            primary: None,
            steps: None,
        }
    }

    /// A whole-number parameter
    pub const fn int(default: i32, min: i32, max: i32) -> Self {
        Self { kind: ParamKind::Int, ..Self::float(default as f32, min as f32, max as f32) }
    }

    /// An on/off switch
    pub const fn toggle(default: bool) -> Self {
        Self { kind: ParamKind::Toggle, ..Self::float(if default { 1.0 } else { 0.0 }, 0.0, 1.0) }
    }

    /// One of `choices`; the range spans their values
    pub const fn choice(default: f32, choices: &'static [Choice]) -> Self {
        let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
        let mut i = 0;
        while i < choices.len() {
            let v = choices[i].value;
            if v < min {
                min = v;
            }
            if v > max {
                max = v;
            }
            i += 1;
        }
        Self { kind: ParamKind::Choice(choices), ..Self::float(default, min, max) }
    }

    pub const fn unit(self, unit: Unit) -> Self {
        Self { unit, ..self }
    }

    pub const fn log(self) -> Self {
        Self { log: true, ..self }
    }

    pub const fn smooth(self, smoothing: Smoothing) -> Self {
        Self { smoothing, ..self }
    }

    pub const fn role(self, role: Role) -> Self {
        Self { role: Some(role), ..self }
    }

    pub const fn primary(self, rank: u8) -> Self {
        Self { primary: Some(rank), ..self }
    }

    pub const fn steps(self, steps: u32) -> Self {
        Self { steps: Some(steps), ..self }
    }

    /// The value a host-given `raw` value stands for: NaN gives the
    /// default, others are clamped, rounded (integers, toggles) or snapped
    /// to the nearest choice
    pub fn sanitize(&self, raw: f32) -> f32 {
        if raw.is_nan() {
            return self.default;
        }
        let v = raw.clamp(self.min, self.max);
        match self.kind {
            ParamKind::Float => v,
            ParamKind::Int => v.round(),
            ParamKind::Toggle => {
                if v >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            ParamKind::Choice(choices) => {
                let mut best = self.default;
                let mut dist = f32::INFINITY;
                for c in choices {
                    let d = (c.value - v).abs();
                    if d < dist {
                        dist = d;
                        best = c.value;
                    }
                }
                best
            }
        }
    }

    /// Position of `value` on the control's travel, 0 to 1 (log aware)
    pub fn to_normalized(&self, value: f32) -> f32 {
        let v = self.sanitize(value);
        if self.max <= self.min {
            return 0.0;
        }
        let t = if self.log && self.min > 0.0 {
            (v / self.min).ln() / (self.max / self.min).ln()
        } else {
            (v - self.min) / (self.max - self.min)
        };
        t.clamp(0.0, 1.0)
    }

    /// Value at position `t` (0 to 1) of the control's travel
    pub fn from_normalized(&self, t: f32) -> f32 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
        let v = if self.log && self.min > 0.0 {
            self.min * (self.max / self.min).powf(t)
        } else {
            self.min + t * (self.max - self.min)
        };
        self.sanitize(v)
    }

    /// Label of a choice value, if this is a choice
    pub fn choice_label(&self, value: f32) -> Option<&'static str> {
        match self.kind {
            ParamKind::Choice(choices) => {
                let v = self.sanitize(value);
                choices.iter().find(|c| c.value == v).map(|c| c.label)
            }
            _ => None,
        }
    }
}

/// A control output: a value the plugin reports (levels, phases, notes)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    pub min: f32,
    pub max: f32,
    pub unit: Unit,
    pub role: Option<Role>,
}

impl Meter {
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max, unit: Unit::None, role: None }
    }

    pub const fn unit(self, unit: Unit) -> Self {
        Self { unit, ..self }
    }

    pub const fn role(self, role: Role) -> Self {
        Self { role: Some(role), ..self }
    }
}

/// An array output streamed to views at a low rate (an LV2 atom output
/// port carrying `openrig:Frame` objects)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stream {
    /// What the array holds (`openrig:content`)
    pub content: crate::view::StreamContent,
    /// Values per channel in a frame
    pub length: u32,
    /// Channels per frame (values are channel after channel)
    pub channels: u32,
    /// Most frames per second
    pub max_rate: f32,
}

impl Stream {
    /// Values in a whole frame
    pub const fn values(&self) -> usize {
        (self.length * self.channels) as usize
    }
}

/// Kind of a port
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PortKind {
    AudioIn,
    AudioOut,
    /// Control input
    Param(Param),
    /// Control input designated `lv2:enabled`: the runner's click-free
    /// bypass (1: processing, 0: bypassed)
    Enabled,
    /// Control output
    Meter(Meter),
    /// Control output designated `lv2:latency`, filled by the runner
    Latency,
    /// Atom input: MIDI and/or transport (`time:Position`) events
    Events {
        midi: bool,
        time: bool,
    },
    /// Atom output of view data
    Stream(Stream),
}

/// A port: stable symbol, display name, kind
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Port {
    /// Stable identifier (`lv2:symbol`): C identifier, never renamed
    pub symbol: &'static str,
    pub name: &'static str,
    /// One-line description (`rdfs:comment`, may be empty)
    pub comment: &'static str,
    pub kind: PortKind,
}

impl Port {
    pub const fn audio_in(symbol: &'static str, name: &'static str) -> Self {
        Self { symbol, name, comment: "", kind: PortKind::AudioIn }
    }

    pub const fn audio_out(symbol: &'static str, name: &'static str) -> Self {
        Self { symbol, name, comment: "", kind: PortKind::AudioOut }
    }

    pub const fn param(symbol: &'static str, name: &'static str, param: Param) -> Self {
        Self { symbol, name, comment: "", kind: PortKind::Param(param) }
    }

    pub const fn meter(symbol: &'static str, name: &'static str, meter: Meter) -> Self {
        Self { symbol, name, comment: "", kind: PortKind::Meter(meter) }
    }

    /// The bypass switch, symbol `enabled`
    pub const fn enabled() -> Self {
        Self { symbol: "enabled", name: "Enabled", comment: "", kind: PortKind::Enabled }
    }

    /// The latency report, symbol `latency`
    pub const fn latency() -> Self {
        Self { symbol: "latency", name: "Latency", comment: "", kind: PortKind::Latency }
    }

    pub const fn events(symbol: &'static str, name: &'static str, midi: bool, time: bool) -> Self {
        Self { symbol, name, comment: "", kind: PortKind::Events { midi, time } }
    }

    pub const fn stream(symbol: &'static str, name: &'static str, stream: Stream) -> Self {
        Self { symbol, name, comment: "", kind: PortKind::Stream(stream) }
    }

    pub const fn comment(self, comment: &'static str) -> Self {
        Self { comment, ..self }
    }

    /// The parameter of a control input
    pub const fn as_param(&self) -> Option<&Param> {
        match &self.kind {
            PortKind::Param(p) => Some(p),
            _ => None,
        }
    }

    /// Control port (input or output), stored as one `f32`
    pub const fn is_control(&self) -> bool {
        matches!(self.kind, PortKind::Param(_) | PortKind::Enabled | PortKind::Meter(_) | PortKind::Latency)
    }

    /// Port the host writes (audio or control input, atom input)
    pub const fn is_input(&self) -> bool {
        matches!(self.kind, PortKind::AudioIn | PortKind::Param(_) | PortKind::Enabled | PortKind::Events { .. })
    }
}

/// A factory preset: parameter values by symbol
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    pub name: &'static str,
    /// Stable URI fragment, unique in the plugin (e.g. "surf")
    pub id: &'static str,
    pub values: &'static [(&'static str, f32)],
}

/// Static description of a plugin
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PluginInfo {
    /// Stable URI, e.g. `https://openrig.eu/plugins/tremolo`
    pub uri: &'static str,
    pub name: &'static str,
    /// One paragraph for stores and `rdfs:comment`
    pub description: &'static str,
    pub class: Class,
    pub version: Version,
    /// SPDX identifier
    pub license: &'static str,
    /// In index order (the LV2 port indices)
    pub ports: &'static [Port],
    pub presets: &'static [Preset],
    /// Native view metadata (`openrig:` vocabulary)
    pub view: Option<View>,
    /// Family and faceplate (`openrig:` vocabulary)
    pub face: Face,
    /// Declare `lv2:hardRTCapable`. Only set when the plugin passes the
    /// RT suite of `orx-test` (its conformance test enforces it).
    pub hard_rt: bool,
}

impl PluginInfo {
    /// Index of the port with `symbol`
    pub fn port_index(&self, symbol: &str) -> Option<usize> {
        self.ports.iter().position(|p| p.symbol == symbol)
    }

    /// Indices of the ports of one kind
    pub fn indices<'a>(&'a self, f: impl Fn(&PortKind) -> bool + 'a) -> impl Iterator<Item = usize> + 'a {
        self.ports.iter().enumerate().filter(move |(_, p)| f(&p.kind)).map(|(i, _)| i)
    }

    pub fn audio_inputs(&self) -> usize {
        self.ports.iter().filter(|p| matches!(p.kind, PortKind::AudioIn)).count()
    }

    pub fn audio_outputs(&self) -> usize {
        self.ports.iter().filter(|p| matches!(p.kind, PortKind::AudioOut)).count()
    }

    /// Default value of every control port, by index (0 for other ports)
    pub fn defaults(&self) -> impl Iterator<Item = f32> + '_ {
        self.ports.iter().map(|p| match p.kind {
            PortKind::Param(param) => param.default,
            PortKind::Enabled => 1.0,
            _ => 0.0,
        })
    }

    /// Value of a preset for each port index (defaults where not given)
    pub fn preset_values(&self, preset: &Preset) -> impl Iterator<Item = (usize, f32)> + '_ {
        let values = preset.values;
        self.ports.iter().enumerate().filter_map(move |(i, p)| {
            let param = p.as_param()?;
            let v = values.iter().find(|(s, _)| *s == p.symbol).map_or(param.default, |(_, v)| *v);
            Some((i, param.sanitize(v)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parses() {
        assert_eq!(Version::parse("0.1.0"), Version { minor: 1, micro: 0 });
        assert_eq!(Version::parse("1.12.3-beta"), Version { minor: 12, micro: 3 });
    }

    #[test]
    fn sanitizes() {
        const CHOICES: &[Choice] = &[Choice::new(0.0, "A"), Choice::new(2.0, "B")];
        let c = Param::choice(0.0, CHOICES);
        assert_eq!((c.min, c.max), (0.0, 2.0));
        assert_eq!(c.sanitize(1.4), 2.0);
        assert_eq!(c.sanitize(f32::NAN), 0.0);
        assert_eq!(c.choice_label(2.0), Some("B"));
        let t = Param::toggle(true);
        assert_eq!(t.sanitize(0.7), 1.0);
        let i = Param::int(3, 1, 8);
        assert_eq!(i.sanitize(4.6), 5.0);
        assert_eq!(i.sanitize(f32::INFINITY), 8.0);
    }

    #[test]
    fn log_mapping_round_trips() {
        let p = Param::float(1000.0, 20.0, 20_000.0).log();
        assert!((p.from_normalized(0.5) - 632.455).abs() < 0.01);
        for v in [20.0, 440.0, 19_999.0] {
            assert!((p.from_normalized(p.to_normalized(v)) - v).abs() / v < 1e-4);
        }
    }
}
