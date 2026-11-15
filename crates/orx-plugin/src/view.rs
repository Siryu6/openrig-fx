//! Native view metadata: the `openrig:` vocabulary (see
//! `docs/OPENRIG-VIEWS.md` for the specification, and `ns/openrig-view.ttl`
//! for its RDF schema, both generated from or checked against this module).
//!
//! A plugin says which view fits it, what each port does in that view, and
//! the curves the host can compute exactly from parameter values. Data the
//! host cannot compute comes from meters (control outputs) and streams
//! (atom outputs).

/// Namespace of the vocabulary
pub const NS: &str = "https://openrig.eu/ns/view#";

/// The view a host should draw above the controls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewKind {
    /// Frequency response from [`Curve`]s
    Eq,
    /// Static level curve (input → output dB) from dynamics roles
    Dynamics,
    /// Echoes on a time axis from delay roles
    EchoTimeline,
    /// Decay time against frequency from reverb roles
    DecayCurve,
    /// One cycle of a modulation from LFO roles and [`Waveform`]
    Lfo,
    /// Pitch shift on a neck or keyboard from pitch roles
    Pitch,
    /// Filter sweep (wah, envelope filter) from sweep roles
    Sweep,
    /// One bar of steps (gate pattern, captured slices, playback speed)
    /// from a [`StreamContent::StepGrid`] stream
    StepGrid,
}

impl ViewKind {
    pub const ALL: [ViewKind; 8] = [
        ViewKind::Eq,
        ViewKind::Dynamics,
        ViewKind::EchoTimeline,
        ViewKind::DecayCurve,
        ViewKind::Lfo,
        ViewKind::Pitch,
        ViewKind::Sweep,
        ViewKind::StepGrid,
    ];

    pub const fn local_name(self) -> &'static str {
        match self {
            ViewKind::Eq => "EqView",
            ViewKind::Dynamics => "DynamicsView",
            ViewKind::EchoTimeline => "EchoTimelineView",
            ViewKind::DecayCurve => "DecayCurveView",
            ViewKind::Lfo => "LfoView",
            ViewKind::Pitch => "PitchView",
            ViewKind::Sweep => "SweepView",
            ViewKind::StepGrid => "StepGridView",
        }
    }

    pub const fn comment(self) -> &'static str {
        match self {
            ViewKind::Eq => "Frequency response drawn from the plugin's openrig:curve descriptions.",
            ViewKind::Dynamics => {
                "Static input/output level curve drawn from the threshold, ratio, knee and makeup roles, with the gain reduction meter."
            }
            ViewKind::EchoTimeline => {
                "Echoes on a time axis drawn from the delay time, feedback and mix or wet level roles, over the host's tempo grid; the first echo can be dragged to set the delay time."
            }
            ViewKind::DecayCurve => "Decay time against frequency drawn from the decay and crossover roles.",
            ViewKind::Lfo => {
                "One cycle of the modulation drawn from the LFO roles and openrig:waveform, or from a ModulationCycle stream."
            }
            ViewKind::Pitch => "Pitch shift on a neck or keyboard drawn from the pitch roles and note meters.",
            ViewKind::Sweep => "Filter response along its sweep drawn from the sweep roles.",
            ViewKind::StepGrid => {
                "One bar in steps drawn from a StepGrid stream: the level each step lets through, the slice captured and its repeats, the playback speed and the playhead."
            }
        }
    }
}

/// What a port does, for the view (`openrig:role`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    // Common
    Mix,
    WetLevel,
    InputGain,
    OutputGain,
    InputLevel,
    OutputLevel,
    // Modulation
    LfoRate,
    LfoDepth,
    LfoShape,
    LfoWaveform,
    LfoTempoDivision,
    LfoPhase,
    StereoPhase,
    ModulationGain,
    CrossoverFrequency,
    // Dynamics
    Threshold,
    Ratio,
    Knee,
    Attack,
    Release,
    Makeup,
    Range,
    GainReduction,
    // Delay
    DelayTime,
    Feedback,
    DelayTempoDivision,
    CurrentDelayTime,
    PingPong,
    // Reverb
    PreDelay,
    Decay,
    DecayLow,
    DecayHigh,
    CrossoverLow,
    CrossoverHigh,
    Size,
    Damping,
    // Pitch
    PitchShift,
    DetectedNote,
    DetectedCents,
    TrackingQuality,
    // Sweep
    PedalPosition,
    SweepLow,
    SweepHigh,
    Resonance,
}

impl Role {
    pub const ALL: [Role; 44] = [
        Role::Mix,
        Role::WetLevel,
        Role::InputGain,
        Role::OutputGain,
        Role::InputLevel,
        Role::OutputLevel,
        Role::LfoRate,
        Role::LfoDepth,
        Role::LfoShape,
        Role::LfoWaveform,
        Role::LfoTempoDivision,
        Role::LfoPhase,
        Role::StereoPhase,
        Role::ModulationGain,
        Role::CrossoverFrequency,
        Role::Threshold,
        Role::Ratio,
        Role::Knee,
        Role::Attack,
        Role::Release,
        Role::Makeup,
        Role::Range,
        Role::GainReduction,
        Role::DelayTime,
        Role::Feedback,
        Role::DelayTempoDivision,
        Role::CurrentDelayTime,
        Role::PingPong,
        Role::PreDelay,
        Role::Decay,
        Role::DecayLow,
        Role::DecayHigh,
        Role::CrossoverLow,
        Role::CrossoverHigh,
        Role::Size,
        Role::Damping,
        Role::PitchShift,
        Role::DetectedNote,
        Role::DetectedCents,
        Role::TrackingQuality,
        Role::PedalPosition,
        Role::SweepLow,
        Role::SweepHigh,
        Role::Resonance,
    ];

    /// Local name (lower camel case: roles are individuals, not classes)
    pub const fn local_name(self) -> &'static str {
        match self {
            Role::Mix => "mix",
            Role::WetLevel => "wetLevel",
            Role::InputGain => "inputGain",
            Role::OutputGain => "outputGain",
            Role::InputLevel => "inputLevel",
            Role::OutputLevel => "outputLevel",
            Role::LfoRate => "lfoRate",
            Role::LfoDepth => "lfoDepth",
            Role::LfoShape => "lfoShape",
            Role::LfoWaveform => "lfoWaveform",
            Role::LfoTempoDivision => "lfoTempoDivision",
            Role::LfoPhase => "lfoPhase",
            Role::StereoPhase => "stereoPhase",
            Role::ModulationGain => "modulationGain",
            Role::CrossoverFrequency => "crossoverFrequency",
            Role::Threshold => "threshold",
            Role::Ratio => "ratio",
            Role::Knee => "knee",
            Role::Attack => "attack",
            Role::Release => "release",
            Role::Makeup => "makeup",
            Role::Range => "range",
            Role::GainReduction => "gainReduction",
            Role::DelayTime => "delayTime",
            Role::Feedback => "feedback",
            Role::DelayTempoDivision => "delayTempoDivision",
            Role::CurrentDelayTime => "currentDelayTime",
            Role::PingPong => "pingPong",
            Role::PreDelay => "preDelay",
            Role::Decay => "decay",
            Role::DecayLow => "decayLow",
            Role::DecayHigh => "decayHigh",
            Role::CrossoverLow => "crossoverLow",
            Role::CrossoverHigh => "crossoverHigh",
            Role::Size => "size",
            Role::Damping => "damping",
            Role::PitchShift => "pitchShift",
            Role::DetectedNote => "detectedNote",
            Role::DetectedCents => "detectedCents",
            Role::TrackingQuality => "trackingQuality",
            Role::PedalPosition => "pedalPosition",
            Role::SweepLow => "sweepLow",
            Role::SweepHigh => "sweepHigh",
            Role::Resonance => "resonance",
        }
    }

    /// Meaning, value convention included
    pub const fn comment(self) -> &'static str {
        match self {
            Role::Mix => "Wet share. 0 is dry only, the maximum of the range is wet only.",
            Role::WetLevel => {
                "Level of the effect added to the dry signal, which stays at unity: 0 adds nothing; linear 0 to 1 (or 100 %), or dB when the unit says."
            }
            Role::InputGain => "Gain applied before the effect, dB.",
            Role::OutputGain => "Gain applied after the effect, dB.",
            Role::InputLevel => "Meter: input level, dBFS.",
            Role::OutputLevel => "Meter: output level, dBFS.",
            Role::LfoRate => "LFO frequency, Hz.",
            Role::LfoDepth => {
                "Modulation depth. The minimum of the range is no modulation, the maximum is full modulation."
            }
            Role::LfoShape => {
                "Continuous waveform morph of openrig:waveform, from the minimum to the maximum of the range."
            }
            Role::LfoWaveform => {
                "Choice of waveform; scale point labels name the waveforms of openrig:Waveform (Sine, Triangle, Square, Saw Up, Saw Down)."
            }
            Role::LfoTempoDivision => {
                "Choice of note value the LFO follows when synced to the tempo; scale point labels are note values (1/4, 1/8., 1/8T). A value of 0 means not synced."
            }
            Role::LfoPhase => "Meter: current LFO phase in turns, 0 to 1.",
            Role::StereoPhase => "Phase offset of the right channel's LFO, degrees, or spread (0 to 100 %).",
            Role::ModulationGain => "Meter: gain the modulation applies now, linear 0 to 1.",
            Role::CrossoverFrequency => "Split frequency between bands modulated differently, Hz.",
            Role::Threshold => "Level where the dynamics curve bends, dBFS.",
            Role::Ratio => "Compression ratio (input dB over output dB above the threshold).",
            Role::Knee => "Width of the soft knee, dB.",
            Role::Attack => "Attack time, ms.",
            Role::Release => "Release time, ms.",
            Role::Makeup => "Gain added after the reduction, dB.",
            Role::Range => "How far a gate or expander lowers the signal, dB (negative).",
            Role::GainReduction => "Meter: current gain reduction, dB (positive or negative, units say).",
            Role::DelayTime => "Time between echoes, in the port's unit (s or ms).",
            Role::Feedback => "Share of each echo fed into the next, 0 to the maximum of the range (1 or 100 %).",
            Role::DelayTempoDivision => {
                "Choice of note value the delay follows when synced to the tempo; a value of 0 means not synced."
            }
            Role::CurrentDelayTime => {
                "Meter: delay time playing now, after tempo sync and any glide, in the port's unit (s or ms)."
            }
            Role::PingPong => "Toggle: echoes alternate between the left and right outputs.",
            Role::PreDelay => "Delay before the reverb, ms.",
            Role::Decay => "Decay time (RT60) at middle frequencies, s.",
            Role::DecayLow => "Decay time of the low band, s, or a multiplier of the mid decay when unitless.",
            Role::DecayHigh => "Decay time of the high band, s, or a multiplier of the mid decay when unitless.",
            Role::CrossoverLow => "Frequency between the low and mid decay bands, Hz.",
            Role::CrossoverHigh => "Frequency between the mid and high decay bands, Hz.",
            Role::Size => "Room size, unitless or m.",
            Role::Damping => "High frequency damping of the tail.",
            Role::PitchShift => "Pitch shift, semitones (or cents per the unit).",
            Role::DetectedNote => "Meter: detected input note, MIDI note number (0 when none).",
            Role::DetectedCents => "Meter: deviation of the detected note, cents.",
            Role::TrackingQuality => "Meter: confidence of the pitch tracking, 0 to 1.",
            Role::PedalPosition => "Expression pedal position, minimum heel down to maximum toe down.",
            Role::SweepLow => "Filter frequency at the heel, Hz.",
            Role::SweepHigh => "Filter frequency at the toe, Hz.",
            Role::Resonance => "Filter resonance (Q).",
        }
    }
}

/// Filter shape of an EQ curve band (RBJ cookbook responses)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterShape {
    Lowpass,
    Highpass,
    Bandpass,
    Notch,
    Allpass,
    Peak,
    LowShelf,
    HighShelf,
}

impl FilterShape {
    pub const ALL: [FilterShape; 8] = [
        FilterShape::Lowpass,
        FilterShape::Highpass,
        FilterShape::Bandpass,
        FilterShape::Notch,
        FilterShape::Allpass,
        FilterShape::Peak,
        FilterShape::LowShelf,
        FilterShape::HighShelf,
    ];

    pub const fn local_name(self) -> &'static str {
        match self {
            FilterShape::Lowpass => "Lowpass",
            FilterShape::Highpass => "Highpass",
            FilterShape::Bandpass => "Bandpass",
            FilterShape::Notch => "Notch",
            FilterShape::Allpass => "Allpass",
            FilterShape::Peak => "Peak",
            FilterShape::LowShelf => "LowShelf",
            FilterShape::HighShelf => "HighShelf",
        }
    }
}

/// How a band's Q value reads
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QUnit {
    /// Cookbook Q
    Q,
    /// Bandwidth in octaves
    Octaves,
    /// Shelf slope S (1: steepest without overshoot)
    Slope,
}

impl QUnit {
    pub const fn local_name(self) -> &'static str {
        match self {
            QUnit::Q => "Q",
            QUnit::Octaves => "Octaves",
            QUnit::Slope => "Slope",
        }
    }
}

/// A value read from a port or fixed by the plugin
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Fixed(f32),
    /// Port symbol
    Port(&'static str),
}

/// The shape of a band: fixed, or chosen by a port
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveShape {
    Fixed(FilterShape),
    /// Port symbol and, for each of its values, the shape (None: band off)
    Port(&'static str, &'static [(f32, Option<FilterShape>)]),
}

/// One band of a frequency response the host computes (`openrig:curve`)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub shape: CurveShape,
    /// Centre or corner frequency, Hz
    pub frequency: Value,
    /// Gain, dB (bells and shelves)
    pub gain: Option<Value>,
    pub q: Option<Value>,
    pub q_unit: QUnit,
    /// Toggle port turning the band on
    pub enable: Option<&'static str>,
}

/// LFO waveform families with an exact definition (see the specification)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
    Square,
    SawUp,
    SawDown,
    /// `tanh(k·sin 2πφ) / tanh(k)` with `k = 16·s²`, `s` the position of the
    /// [`Role::LfoShape`] port on its range (0: sine, 1: rounded square)
    SineToSquare,
}

impl Waveform {
    pub const ALL: [Waveform; 6] = [
        Waveform::Sine,
        Waveform::Triangle,
        Waveform::Square,
        Waveform::SawUp,
        Waveform::SawDown,
        Waveform::SineToSquare,
    ];

    pub const fn local_name(self) -> &'static str {
        match self {
            Waveform::Sine => "Sine",
            Waveform::Triangle => "Triangle",
            Waveform::Square => "Square",
            Waveform::SawUp => "SawUp",
            Waveform::SawDown => "SawDown",
            Waveform::SineToSquare => "SineToSquare",
        }
    }

    /// Value at `phase` (turns) for a shape amount `s` in [0, 1]; the
    /// reference the host and the plugin share
    pub fn eval(self, phase: f32, s: f32) -> f32 {
        let p = phase - phase.floor();
        match self {
            Waveform::Sine => orx_dsp::lfo::sine(p),
            Waveform::Triangle => orx_dsp::lfo::triangle(p),
            Waveform::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::SawUp => 2.0 * p - 1.0,
            Waveform::SawDown => 1.0 - 2.0 * p,
            Waveform::SineToSquare => orx_dsp::lfo::sine_to_square(p, orx_dsp::lfo::shape_steepness(s)),
        }
    }
}

/// Kind of dynamics processor for the dynamics view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicsKind {
    Compressor,
    Limiter,
    Gate,
    Expander,
}

impl DynamicsKind {
    pub const fn local_name(self) -> &'static str {
        match self {
            DynamicsKind::Compressor => "Compressor",
            DynamicsKind::Limiter => "Limiter",
            DynamicsKind::Gate => "Gate",
            DynamicsKind::Expander => "Expander",
        }
    }
}

/// What a stream frame holds (`openrig:content`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamContent {
    /// One LFO cycle of the gain actually applied, linear 0 to 1, from
    /// phase 0 to 1, per channel; `position` is the current phase
    ModulationCycle,
    /// Magnitude spectrum, dBFS, on log-spaced bins from 20 Hz to 20 kHz,
    /// per channel
    Spectrum,
    /// Level history, dBFS, oldest first, per channel; `position` is unused
    LevelHistory,
    /// One bar in cells: level, source and speed rows; `position` is the
    /// playhead, the fraction of the bar elapsed
    StepGrid,
}

impl StreamContent {
    pub const ALL: [StreamContent; 4] =
        [StreamContent::ModulationCycle, StreamContent::Spectrum, StreamContent::LevelHistory, StreamContent::StepGrid];

    /// Rows of a [`StreamContent::StepGrid`] frame
    pub const STEP_GRID_ROWS: u32 = 3;

    pub const fn local_name(self) -> &'static str {
        match self {
            StreamContent::ModulationCycle => "ModulationCycle",
            StreamContent::Spectrum => "Spectrum",
            StreamContent::LevelHistory => "LevelHistory",
            StreamContent::StepGrid => "StepGrid",
        }
    }

    pub const fn comment(self) -> &'static str {
        match self {
            StreamContent::ModulationCycle => {
                "One cycle of the gain the modulation applies, linear 0 to 1, from LFO phase 0 to 1, one row per channel. openrig:position is the current phase."
            }
            StreamContent::Spectrum => {
                "Magnitude spectrum in dBFS on openrig:length log-spaced bins from 20 Hz to 20 kHz, one row per channel."
            }
            StreamContent::LevelHistory => {
                "Level history in dBFS, oldest value first, evenly spaced over the frame interval, one row per channel."
            }
            StreamContent::StepGrid => {
                "One bar from the downbeat in openrig:length equal cells, three rows: the level each cell lets through (linear, 0 to 1); its source (0 the live input, 1 the slice being captured, 2 a repeat of it); its playback speed (1 normal, 0 stopped, negative backward). openrig:position is the playhead, the fraction of the bar elapsed."
            }
        }
    }
}

/// View metadata of a plugin
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub kind: ViewKind,
    /// Bands of an EQ view
    pub curves: &'static [Curve],
    /// Waveform of an LFO view
    pub waveform: Option<Waveform>,
    /// Kind of a dynamics view
    pub dynamics: Option<DynamicsKind>,
}

impl View {
    pub const fn new(kind: ViewKind) -> Self {
        Self { kind, curves: &[], waveform: None, dynamics: None }
    }

    pub const fn curves(self, curves: &'static [Curve]) -> Self {
        Self { curves, ..self }
    }

    pub const fn waveform(self, waveform: Waveform) -> Self {
        Self { waveform: Some(waveform), ..self }
    }

    pub const fn dynamics(self, kind: DynamicsKind) -> Self {
        Self { dynamics: Some(kind), ..self }
    }
}

/// Which family a plugin belongs to (`openrig:family`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Simple, versatile tools, drawn in the host's own look
    Basic,
    /// Plugins with a character of their own, drawn as a faceplate
    Signature,
}

impl Family {
    pub const ALL: [Family; 2] = [Family::Basic, Family::Signature];

    pub const fn local_name(self) -> &'static str {
        match self {
            Family::Basic => "Basic",
            Family::Signature => "Signature",
        }
    }

    pub const fn comment(self) -> &'static str {
        match self {
            Family::Basic => {
                "A simple, versatile tool. The host draws it in its own look; openrig:faceColor and openrig:knobStyle are hints at most."
            }
            Family::Signature => {
                "A plugin with a character of its own. The host draws it as a faceplate in its openrig:faceColor with openrig:knobStyle knobs."
            }
        }
    }
}

/// Knob shapes a faceplate draws (`openrig:knobStyle`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnobStyle {
    Plain,
    Skirted,
    Chickenhead,
    Domed,
}

impl KnobStyle {
    pub const ALL: [KnobStyle; 4] = [KnobStyle::Plain, KnobStyle::Skirted, KnobStyle::Chickenhead, KnobStyle::Domed];

    pub const fn local_name(self) -> &'static str {
        match self {
            KnobStyle::Plain => "Plain",
            KnobStyle::Skirted => "Skirted",
            KnobStyle::Chickenhead => "Chickenhead",
            KnobStyle::Domed => "Domed",
        }
    }

    pub const fn comment(self) -> &'static str {
        match self {
            KnobStyle::Plain => "The host's own knob.",
            KnobStyle::Skirted => "A cap over a wider skirt marked with the scale, a line pointer on the cap.",
            KnobStyle::Chickenhead => "A tapered pointer knob: the position reads from the shape itself.",
            KnobStyle::Domed => "A smooth round cap with a dot or short line as the pointer.",
        }
    }
}

/// Surface of a faceplate (`openrig:finish`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Painted,
    Brushed,
}

impl Finish {
    pub const ALL: [Finish; 2] = [Finish::Painted, Finish::Brushed];

    pub const fn local_name(self) -> &'static str {
        match self {
            Finish::Painted => "Painted",
            Finish::Brushed => "Brushed",
        }
    }

    pub const fn comment(self) -> &'static str {
        match self {
            Finish::Painted => "Paint over the enclosure: an even colour with a slight grain.",
            Finish::Brushed => "Bare brushed metal tinted by the colour: fine parallel streaks.",
        }
    }
}

/// How the plate name is lettered (`openrig:plateStyle`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlateStyle {
    Silkscreen,
    Glitch,
}

impl PlateStyle {
    pub const ALL: [PlateStyle; 2] = [PlateStyle::Silkscreen, PlateStyle::Glitch];

    pub const fn local_name(self) -> &'static str {
        match self {
            PlateStyle::Silkscreen => "Silkscreen",
            PlateStyle::Glitch => "Glitch",
        }
    }

    pub const fn comment(self) -> &'static str {
        match self {
            PlateStyle::Silkscreen => "The plate name printed plainly in the host's lettering. The default.",
            PlateStyle::Glitch => {
                "The plate name cut by two horizontal bands shifted sideways, as a corrupted picture: the band from 20 to 30 % of the cap height (from the baseline) moves right by 8 % of the cap height and takes the host's accent colour, the band from 62 to 70 % moves left by 4 %. Fixed, never animated."
            }
        }
    }
}

/// How a host presents the plugin: its family and, for a faceplate, its
/// colour, surface, knobs, control order and names
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub family: Family,
    /// Faceplate colour, `#RRGGBB` (sRGB); None: the host's panel colour
    pub color: Option<&'static str>,
    pub knob: KnobStyle,
    /// Control symbols in reading order on the faceplate
    /// (`openrig:order` 1, 2…); controls not listed are left to the
    /// host's full control list
    pub layout: &'static [&'static str],
    /// A few words under the name (at most [`Face::TAGLINE_MAX`]
    /// characters)
    pub tagline: &'static str,
    /// Surface of the faceplate
    pub finish: Option<Finish>,
    /// How worn the faceplate looks, 0 (new) to 1, fixed per plugin
    pub wear: Option<f32>,
    /// Short name printed on the faceplate (at most
    /// [`Face::PLATE_NAME_MAX`] characters)
    pub plate_name: Option<&'static str>,
    /// Lettering of the plate name; None: [`PlateStyle::Silkscreen`]
    pub plate_style: Option<PlateStyle>,
}

impl Face {
    pub const TAGLINE_MAX: usize = 48;
    pub const PLATE_NAME_MAX: usize = 12;

    /// A basic plugin: the host's look, plain knobs
    pub const fn basic(tagline: &'static str, layout: &'static [&'static str]) -> Self {
        Self {
            family: Family::Basic,
            color: None,
            knob: KnobStyle::Plain,
            layout,
            tagline,
            finish: None,
            wear: None,
            plate_name: None,
            plate_style: None,
        }
    }

    /// A signature plugin: its own faceplate, printed with `plate_name`
    pub const fn signature(
        plate_name: &'static str,
        tagline: &'static str,
        color: &'static str,
        knob: KnobStyle,
        layout: &'static [&'static str],
    ) -> Self {
        Self {
            family: Family::Signature,
            color: Some(color),
            knob,
            layout,
            tagline,
            finish: Some(Finish::Painted),
            wear: Some(0.0),
            plate_name: Some(plate_name),
            plate_style: None,
        }
    }

    pub const fn finish(self, finish: Finish) -> Self {
        Self { finish: Some(finish), ..self }
    }

    pub const fn wear(self, wear: f32) -> Self {
        Self { wear: Some(wear), ..self }
    }

    pub const fn plate_style(self, style: PlateStyle) -> Self {
        Self { plate_style: Some(style), ..self }
    }

    /// Faceplate rank of a control, 1 first
    pub fn order(&self, symbol: &str) -> Option<usize> {
        self.layout.iter().position(|s| *s == symbol).map(|i| i + 1)
    }
}

/// Whether `s` is a `#RRGGBB` colour
pub fn is_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
