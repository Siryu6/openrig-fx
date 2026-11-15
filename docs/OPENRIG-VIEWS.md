# OpenRig native views: the `openrig:` vocabulary

Version 1. Namespace `https://openrig.eu/ns/view#`, prefix `openrig:`.
Machine-readable schema: [`ns/openrig-view.ttl`](../ns/openrig-view.ttl),
generated from `orx_plugin::view` by `cargo xtask vocab` (a test fails when
it is stale). This document is the human specification; a test checks that
it covers every term of the schema.

## Purpose

A pedalboard has no room, and often no input device, for a plugin's own
GUI. OpenRig draws its own **native view** above a plugin's knobs: an EQ
curve, a compressor's level curve, an LFO cycle, echoes on a time line.
Today it infers them from port names and units (`view_infer.rs` in
OpenRig), which works for well-named plugins and fails quietly for the
others. The `openrig:` vocabulary lets a plugin *state* what OpenRig
otherwise guesses:

- which view fits the plugin (`openrig:view`);
- what each port means to that view (`openrig:role`), with a value
  convention per role, so the host never interprets a number wrongly;
- exact curves the host computes itself from port values (filter bands
  with the RBJ cookbook formulas, LFO waveforms with a closed form), so
  the drawing is the response the plugin plays, without any traffic;
- data only the plugin knows (the gain it applies over a cycle, a
  spectrum, a level history), as rate-limited frames on an atom output
  (`openrig:stream`);
- which few controls matter most (`openrig:primary`), for a knob strip
  or footswitch screen that shows four of them;
- how to present the plugin (`openrig:family`): in the host's own look,
  or as a faceplate with its colour, knob style, control order and
  tagline (see [Faceplates](#faceplates)).

Everything is plain LV2 Turtle beside the standard properties. A host
that ignores the vocabulary loses nothing: the plugin stays a correct
LV2 plugin with ordinary control ports.

## Rules for hosts

1. **Optional and additive.** Every statement is a hint for drawing;
   none changes how the plugin is run. A host must keep working with any
   subset it does not understand.
2. **Unknown terms are ignored**, not errors: a later version may add
   views, roles, waveforms, shapes or stream contents. A view the host
   cannot draw (missing role, unknown waveform) falls back to the host's
   own inference, then to knobs only.
3. **Ports are named by `lv2:symbol`** in every `…Port` property (as
   presets do), never by index.
4. **Values are in the port's own unit.** A role states the convention
   (dB, Hz, ms, 0 to 1…); where it allows several, `units:unit` of the
   port decides.
5. **The plugin's statements win** over inferred ones for the same port.

## Rules for plugins

- State `openrig:view` only when the view's roles are present.
- Give a role to at most one port per meaning (one `openrig:lfoRate`…).
- Keep `openrig:primary` ranks unique, from 1; four or fewer is typical.
- Keep `openrig:order` places unique, from 1, on control inputs only.
- A stream port is an `atom:AtomPort` output with `atom:bufferType
  atom:Sequence`, `atom:supports openrig:Frame`, an `rsz:minimumSize` that
  fits one frame, and `lv2:portProperty pprops:notOnGUI`.
- Never send more than `openrig:rate` frames per second; a host may drop
  frames it has no time to draw.

## Example

OR Tremolo Stereo (abridged from the generated `tremolo-stereo.ttl`):

```turtle
<https://openrig.eu/plugins/tremolo-stereo>
    a lv2:Plugin, lv2:ModulatorPlugin, doap:Project ;
    openrig:view openrig:LfoView ;
    openrig:waveform openrig:SineToSquare ;
    lv2:port [
        a lv2:InputPort , lv2:ControlPort ;
        lv2:symbol "rate" ;
        lv2:default 5.0 ; lv2:minimum 0.1 ; lv2:maximum 20.0 ;
        units:unit units:hz ;
        lv2:portProperty pprops:logarithmic ;
        openrig:role openrig:lfoRate ;
        openrig:primary 1
    ] , [
        a lv2:OutputPort , lv2:ControlPort ;
        lv2:symbol "lfo_phase" ;
        openrig:role openrig:lfoPhase
    ] , [
        a lv2:OutputPort , atom:AtomPort ;
        lv2:symbol "cycle" ;
        atom:bufferType atom:Sequence ;
        atom:supports openrig:Frame ;
        rsz:minimumSize 1232 ;
        lv2:portProperty pprops:notOnGUI ;
        openrig:stream [
            openrig:content openrig:ModulationCycle ;
            openrig:length 64 ;
            openrig:channels 2 ;
            openrig:rate 30.0
        ]
    ] .
```

A host draws one LFO cycle: the shape from `openrig:SineToSquare` and the
port holding `openrig:lfoShape`, the speed from `openrig:lfoRate`, a
moving cursor from the `openrig:lfoPhase` meter. When it reads the
`cycle` stream instead, it draws the exact gain the plugin applies on
each channel, the cursor at the frame's `openrig:position`.

An EQ band, as a plugin with a band-type selector would state it:

```turtle
<urn:example:eq>
    openrig:view openrig:EqView ;
    openrig:curve [
        openrig:shapePort "type1" ;
        openrig:shapeChoice [ rdf:value 0 ; openrig:shape openrig:Off ] ,
                            [ rdf:value 1 ; openrig:shape openrig:Peak ] ,
                            [ rdf:value 2 ; openrig:shape openrig:LowShelf ] ;
        openrig:frequencyPort "freq1" ;
        openrig:gainPort "gain1" ;
        openrig:qPort "q1" ;
        openrig:qUnit openrig:Q ;
        openrig:enablePort "on1"
    ] , [
        openrig:shape openrig:Highpass ;
        openrig:frequency 30.0 ;
        openrig:q 0.707 ;
        openrig:qUnit openrig:Q
    ] .
```

The response is the product of the bands, each computed at the
plugin's sample rate with the cookbook formulas; a band is flat while its
`openrig:enablePort` is 0 or its shape is `openrig:Off`.

## Plugin properties

| Property | Domain | Range | Meaning |
|---|---|---|---|
| `openrig:view` | `lv2:Plugin` | `openrig:View` | The view a host should draw above the plugin's controls. |
| `openrig:waveform` | `lv2:Plugin` | `openrig:Waveform` | Waveform of an LfoView; openrig:lfoShape morphs it where the waveform says so. |
| `openrig:dynamics` | `lv2:Plugin` | `openrig:DynamicsKind` | Kind of processor of a DynamicsView. |
| `openrig:curve` | `lv2:Plugin` | `openrig:Curve` | A band of the frequency response of an EqView; the response is the product of all bands. |
| `openrig:family` | `lv2:Plugin` | `openrig:Family` | The plugin's family; a host without faceplates draws every family alike. |
| `openrig:faceColor` | `lv2:Plugin` | `xsd:string` | Faceplate colour, "#RRGGBB" in sRGB. The host keeps its own accent, text and texture over it, and may darken or lighten it to keep labels readable. |
| `openrig:knobStyle` | `lv2:Plugin` | `openrig:KnobStyle` | The knob shape of the faceplate. |
| `openrig:finish` | `lv2:Plugin` | `openrig:Finish` | The surface of the faceplate. |
| `openrig:wear` | `lv2:Plugin` | `xsd:decimal` | How worn the faceplate looks, 0 (new) to 1, fixed for the plugin. The host draws the wear from a seed it derives from the plugin URI, so a plugin always looks the same. |
| `openrig:plateName` | `lv2:Plugin` | `xsd:string` | The short name printed on the faceplate, 12 characters at most. |
| `openrig:plateStyle` | `lv2:Plugin` | `openrig:PlateStyle` | The lettering of the plate name, openrig:Silkscreen when absent. Signature plugins only. |
| `openrig:tagline` | `lv2:Plugin` | `xsd:string` | A few words under the plugin's name, 48 characters at most. |
| `openrig:shape` | `openrig:Curve` | `openrig:FilterShape` | Fixed shape of a band, or the shape an openrig:ShapeChoice selects. |
| `openrig:shapePort` | `openrig:Curve` | `xsd:string` | Symbol of the port choosing the band shape, with one openrig:shapeChoice per value. |
| `openrig:shapeChoice` | `openrig:Curve` | `openrig:ShapeChoice` | Maps one rdf:value of the openrig:shapePort to an openrig:shape. |
| `openrig:frequency` | `openrig:Curve` | `xsd:decimal` | Fixed band frequency, Hz. |
| `openrig:frequencyPort` | `openrig:Curve` | `xsd:string` | Symbol of the port holding the band frequency, Hz. |
| `openrig:gain` | `openrig:Curve` | `xsd:decimal` | Fixed band gain, dB. |
| `openrig:gainPort` | `openrig:Curve` | `xsd:string` | Symbol of the port holding the band gain, dB. |
| `openrig:q` | `openrig:Curve` | `xsd:decimal` | Fixed band Q, read per openrig:qUnit. |
| `openrig:qPort` | `openrig:Curve` | `xsd:string` | Symbol of the port holding the band Q, read per openrig:qUnit. |
| `openrig:qUnit` | `openrig:Curve` | `openrig:QUnit` | How the band Q reads. |
| `openrig:enablePort` | `openrig:Curve` | `xsd:string` | Symbol of a toggle port; the band is flat while it is 0. |
| `openrig:role` | `lv2:Port` | `openrig:Role` | What the port means to the view. |
| `openrig:primary` | `lv2:Port` | `xsd:positiveInteger` | Rank (1 first) of the port among the controls a compact host shows first, such as the knob strip of a footswitch screen. |
| `openrig:order` | `lv2:Port` | `xsd:positiveInteger` | Place (1 first) of the control on the faceplate, in reading order. Controls without one stay in the host's full control list. |
| `openrig:stream` | `lv2:Port` | `openrig:Stream` | The frames this atom output sends. |
| `openrig:content` | `openrig:Stream` | `openrig:StreamContent` | What the values of each frame are. |
| `openrig:length` | `openrig:Stream` | `xsd:positiveInteger` | Values per channel in each frame. |
| `openrig:channels` | `openrig:Stream` | `xsd:positiveInteger` | Channels in each frame. Also the key of the channel count in an openrig:Frame. |
| `openrig:rate` | `openrig:Stream` | `xsd:decimal` | Most frames per second the plugin sends, Hz. |
| `openrig:position` | `openrig:Frame` | `xsd:float` | A position in the frame (the current LFO phase of a ModulationCycle), per openrig:content. |
| `openrig:values` | `openrig:Frame` | `atom:Vector` | The values of the frame, channel after channel. |

Every `…Port` property is the alternative to the fixed property of the
same name (`openrig:frequency` or `openrig:frequencyPort`): a band takes
one or the other for each value.

## Classes

| Term | Meaning |
|---|---|
| `openrig:View` | A kind of native view a host draws for a plugin. Instances are the views below. |
| `openrig:Role` | What a port means to the view. Instances are lower camel case individuals; each states its value convention. |
| `openrig:Waveform` | An LFO waveform with an exact definition, so the host draws the curve the plugin plays. |
| `openrig:FilterShape` | A filter response with the RBJ Audio EQ Cookbook definition, for openrig:curve bands. openrig:Off is a band that contributes nothing. |
| `openrig:QUnit` | How the Q value of an openrig:curve band reads. |
| `openrig:DynamicsKind` | The kind of processor a DynamicsView draws. |
| `openrig:Curve` | One band of a frequency response the host computes from port values. |
| `openrig:ShapeChoice` | One value of an openrig:shapePort and the band shape it selects. |
| `openrig:Stream` | Description of the frames an atom output port sends. |
| `openrig:StreamContent` | What the values of a stream frame are. |
| `openrig:Frame` | Atom object sent on a stream port: openrig:channels (Int), openrig:position (Float) and openrig:values (Vector of Float, channel after channel). |
| `openrig:Family` | How a host presents a plugin: in its own look, or as a faceplate of its own. |
| `openrig:KnobStyle` | The knob shape a faceplate draws. |
| `openrig:Finish` | The surface of a faceplate. |
| `openrig:PlateStyle` | How the plate name of a faceplate is lettered. |

## Views

Instances of `openrig:View`, the values of `openrig:view`. Each view lists
the roles it draws from; a view whose roles are missing is not drawn.

| Term | Meaning |
|---|---|
| `openrig:EqView` | Frequency response drawn from the plugin's openrig:curve descriptions. |
| `openrig:DynamicsView` | Static input/output level curve drawn from the threshold, ratio, knee and makeup roles, with the gain reduction meter. |
| `openrig:EchoTimelineView` | Echoes on a time axis drawn from the delay time, feedback and mix or wet level roles, over the host's tempo grid; the first echo can be dragged to set the delay time. |
| `openrig:DecayCurveView` | Decay time against frequency drawn from the decay and crossover roles. |
| `openrig:LfoView` | One cycle of the modulation drawn from the LFO roles and openrig:waveform, or from a ModulationCycle stream. |
| `openrig:PitchView` | Pitch shift on a neck or keyboard drawn from the pitch roles and note meters. |
| `openrig:SweepView` | Filter response along its sweep drawn from the sweep roles. |
| `openrig:StepGridView` | One bar in steps drawn from a StepGrid stream: the level each step lets through, the slice captured and its repeats, the playback speed and the playhead. |

### The echo timeline

`openrig:EchoTimelineView` is the view of a delay on a stage: it shows
where the echoes fall against the beat, and it is the place to set the
time with a finger or a tap. A host draws it from these ports:

| Role | Needed | Use |
|---|---|---|
| `openrig:delayTime` | yes | Spacing of the echoes when not synced, and what a drag or a tap sets |
| `openrig:feedback` | yes | Height of each echo after the first |
| `openrig:mix` or `openrig:wetLevel` | yes | Height of the first echo |
| `openrig:delayTempoDivision` | no | Note value of the spacing when synced; the snap grid of a drag |
| `openrig:currentDelayTime` | no | The spacing actually playing (sync resolved, glide in progress) |
| `openrig:pingPong` | no | Echoes drawn alternately on a left and a right lane |

**Drawing.** Time runs from 0 (the note played) to the right. The echo
`n` (from 1) sits at `n·T` with a height of `level·feedbackⁿ⁻¹`, where
`T` is the delay time (from `openrig:currentDelayTime` when present,
else from the division at the host tempo when synced, else from
`openrig:delayTime`), `feedback` the feedback role on its range mapped
to 0 to 1 (a range above 1, for runaway settings, draws heights capped
at 1), and `level` the wet level (or the mix share). Echoes below
−60 dB relative to the first are not drawn. With `openrig:pingPong` on,
odd echoes go to the left lane and even echoes to the right.

**Tempo grid.** When the host has a tempo, it draws a vertical line on
every beat (a quarter note at the host tempo) and a lighter one on each
subdivision of the chosen note value, from 0 to the last drawn echo.
Without a tempo, it draws a grid in milliseconds (100 ms lines).

**Dragging.** The first echo is a handle. Dragging it horizontally sets
`openrig:delayTime` to the time under the finger, within the port's
range. When `openrig:delayTempoDivision` is non-zero, the drag snaps
to the note values of that port's scale points instead and sets the
division; dragging past the longest or shortest note value turns sync
off (division 0) and sets the time directly. Dragging the second echo
vertically sets `openrig:feedback` from its height relative to the
first.

**Tapping.** A host's tap-tempo input (footswitch, screen button) is
the beat. When `openrig:delayTempoDivision` is non-zero, the host sets
its own tempo from the taps and the plugin follows it through the
transport (the echo lands on the chosen note value of the tapped beat).
When it is 0, the host sets `openrig:delayTime` to the average interval
of the last taps. Either way the plugin is expected to change its time
without clicks (a crossfade or a glide), which `openrig:currentDelayTime`
shows as it happens.

### The step grid

`openrig:StepGridView` is the view of a stutter, a slicer or a
rhythmic gate: one bar seen as a row of steps. It is drawn from the
plugin's `openrig:StepGrid` stream alone (a plugin stating the view
without such a stream is invalid), so the host shows what plays, not
what the controls suggest.

**Drawing.** The bar runs from the downbeat on the left to the next
downbeat on the right, cut in `openrig:length` equal cells. For each
cell:

- the **level** row (0 to 1) is the height of the cell's bar: a chop
  pattern reads as a row of tall and short bars;
- the **source** row tells what the cell plays: 0 the live input (the
  host's plain colour), 1 the slice being captured (outlined in the
  accent), 2 a repeat of that slice (filled in the accent). A run of
  cells with the same non-zero source and no 1 between them is one
  slice and its repeats;
- the **speed** row is drawn as a line across the cells at the height
  of the speed between 0 and 1, and is omitted where the speed is 1: a
  tape stop shows as a line falling to 0, a pitched repeat as a step.
  A negative speed (played backward) is drawn as a reversed arrow on
  the cell.

`openrig:position` is the playhead, a vertical line at that fraction
of the bar. A host draws a line on each beat (a quarter of the bar in
4/4) under the cells. The plugin chooses `openrig:length` so that its
patterns fall on whole cells (48 for steps of 1/16 and triplets of
1/8; 96 to add 1/32); the host may merge neighbouring cells to fit its
width, keeping the lowest level, the highest source and the speed of
the first cell.

## Roles

Instances of `openrig:Role`, the values of `openrig:role`. A role says
what the port means and its value convention. "Meter" roles belong on
control outputs; the others on control inputs.

| Term | Meaning |
|---|---|
| `openrig:mix` | Wet share. 0 is dry only, the maximum of the range is wet only. |
| `openrig:wetLevel` | Level of the effect added to the dry signal, which stays at unity: 0 adds nothing; linear 0 to 1 (or 100 %), or dB when the unit says. |
| `openrig:inputGain` | Gain applied before the effect, dB. |
| `openrig:outputGain` | Gain applied after the effect, dB. |
| `openrig:inputLevel` | Meter: input level, dBFS. |
| `openrig:outputLevel` | Meter: output level, dBFS. |
| `openrig:lfoRate` | LFO frequency, Hz. |
| `openrig:lfoDepth` | Modulation depth. The minimum of the range is no modulation, the maximum is full modulation. |
| `openrig:lfoShape` | Continuous waveform morph of openrig:waveform, from the minimum to the maximum of the range. |
| `openrig:lfoWaveform` | Choice of waveform; scale point labels name the waveforms of openrig:Waveform (Sine, Triangle, Square, Saw Up, Saw Down). |
| `openrig:lfoTempoDivision` | Choice of note value the LFO follows when synced to the tempo; scale point labels are note values (1/4, 1/8., 1/8T). A value of 0 means not synced. |
| `openrig:lfoPhase` | Meter: current LFO phase in turns, 0 to 1. |
| `openrig:stereoPhase` | Phase offset of the right channel's LFO, degrees, or spread (0 to 100 %). |
| `openrig:modulationGain` | Meter: gain the modulation applies now, linear 0 to 1. |
| `openrig:crossoverFrequency` | Split frequency between bands modulated differently, Hz. |
| `openrig:threshold` | Level where the dynamics curve bends, dBFS. |
| `openrig:ratio` | Compression ratio (input dB over output dB above the threshold). |
| `openrig:knee` | Width of the soft knee, dB. |
| `openrig:attack` | Attack time, ms. |
| `openrig:release` | Release time, ms. |
| `openrig:makeup` | Gain added after the reduction, dB. |
| `openrig:range` | How far a gate or expander lowers the signal, dB (negative). |
| `openrig:gainReduction` | Meter: current gain reduction, dB (positive or negative, units say). |
| `openrig:delayTime` | Time between echoes, in the port's unit (s or ms). |
| `openrig:feedback` | Share of each echo fed into the next, 0 to the maximum of the range (1 or 100 %). |
| `openrig:delayTempoDivision` | Choice of note value the delay follows when synced to the tempo; a value of 0 means not synced. |
| `openrig:currentDelayTime` | Meter: delay time playing now, after tempo sync and any glide, in the port's unit (s or ms). |
| `openrig:pingPong` | Toggle: echoes alternate between the left and right outputs. |
| `openrig:preDelay` | Delay before the reverb, ms. |
| `openrig:decay` | Decay time (RT60) at middle frequencies, s. |
| `openrig:decayLow` | Decay time of the low band, s, or a multiplier of the mid decay when unitless. |
| `openrig:decayHigh` | Decay time of the high band, s, or a multiplier of the mid decay when unitless. |
| `openrig:crossoverLow` | Frequency between the low and mid decay bands, Hz. |
| `openrig:crossoverHigh` | Frequency between the mid and high decay bands, Hz. |
| `openrig:size` | Room size, unitless or m. |
| `openrig:damping` | High frequency damping of the tail. |
| `openrig:pitchShift` | Pitch shift, semitones (or cents per the unit). |
| `openrig:detectedNote` | Meter: detected input note, MIDI note number (0 when none). |
| `openrig:detectedCents` | Meter: deviation of the detected note, cents. |
| `openrig:trackingQuality` | Meter: confidence of the pitch tracking, 0 to 1. |
| `openrig:pedalPosition` | Expression pedal position, minimum heel down to maximum toe down. |
| `openrig:sweepLow` | Filter frequency at the heel, Hz. |
| `openrig:sweepHigh` | Filter frequency at the toe, Hz. |
| `openrig:resonance` | Filter resonance (Q). |

## Waveforms

Instances of `openrig:Waveform`, the values of `openrig:waveform`. Phase
runs from 0 to 1 over one cycle; values run from -1 to 1. The host maps
them to the gain the modulation applies with the depth role.

| Term | Meaning |
|---|---|
| `openrig:Sine` | sin(2 pi phase). |
| `openrig:Triangle` | Triangle, 0 at phase 0, 1 at 1/4, -1 at 3/4. |
| `openrig:Square` | 1 for phase below 1/2, -1 above. |
| `openrig:SawUp` | 2 phase - 1. |
| `openrig:SawDown` | 1 - 2 phase. |
| `openrig:SineToSquare` | tanh(k sin(2 pi phase)) / tanh(k) with k = 16 s^2, s the openrig:lfoShape position on its range from 0 to 1. A pure sine at s = 0. |

## Filter shapes

Instances of `openrig:FilterShape`, the values of `openrig:shape`: the
responses of Robert Bristow-Johnson's *Audio EQ Cookbook* with its
definitions of Q, BW and S.

| Term | Meaning |
|---|---|
| `openrig:Lowpass` | RBJ Audio EQ Cookbook response. |
| `openrig:Highpass` | RBJ Audio EQ Cookbook response. |
| `openrig:Bandpass` | RBJ Audio EQ Cookbook response. |
| `openrig:Notch` | RBJ Audio EQ Cookbook response. |
| `openrig:Allpass` | RBJ Audio EQ Cookbook response. |
| `openrig:Peak` | RBJ Audio EQ Cookbook response. |
| `openrig:LowShelf` | RBJ Audio EQ Cookbook response. |
| `openrig:HighShelf` | RBJ Audio EQ Cookbook response. |
| `openrig:Off` | No band: a flat response of 0 dB. |

## Q units

Instances of `openrig:QUnit`, the values of `openrig:qUnit`.

| Term | Meaning |
|---|---|
| `openrig:Q` | Cookbook Q. |
| `openrig:Octaves` | Bandwidth in octaves (cookbook BW). |
| `openrig:Slope` | Shelf slope S (cookbook); 1 is the steepest without overshoot. |

## Dynamics kinds

Instances of `openrig:DynamicsKind`, the values of `openrig:dynamics`.

| Term | Meaning |
|---|---|
| `openrig:Compressor` | Kind of dynamics processor. |
| `openrig:Limiter` | Kind of dynamics processor. |
| `openrig:Gate` | Kind of dynamics processor. |
| `openrig:Expander` | Kind of dynamics processor. |

## Streams

A stream port sends `openrig:Frame` atom objects in its output sequence,
at most `openrig:rate` per second (the plugin counts frames of audio,
so the rate holds at any block size):

| Key | Atom type | Value |
|---|---|---|
| `openrig:channels` | `atom:Int` | Rows in `openrig:values` |
| `openrig:position` | `atom:Float` | Position per `openrig:content` (0 when unused) |
| `openrig:values` | `atom:Vector` of `atom:Float` | `openrig:length` values per channel, channel after channel |

Contents, the values of `openrig:content`:

| Term | Meaning |
|---|---|
| `openrig:ModulationCycle` | One cycle of the gain the modulation applies, linear 0 to 1, from LFO phase 0 to 1, one row per channel. openrig:position is the current phase. |
| `openrig:Spectrum` | Magnitude spectrum in dBFS on openrig:length log-spaced bins from 20 Hz to 20 kHz, one row per channel. |
| `openrig:LevelHistory` | Level history in dBFS, oldest value first, evenly spaced over the frame interval, one row per channel. |
| `openrig:StepGrid` | One bar from the downbeat in openrig:length equal cells, three rows: the level each cell lets through (linear, 0 to 1); its source (0 the live input, 1 the slice being captured, 2 a repeat of it); its playback speed (1 normal, 0 stopped, negative backward). openrig:position is the playhead, the fraction of the bar elapsed. |

A StepGrid stream always has `openrig:channels` 3: the rows are those
three, in that order, rather than audio channels.

## Faceplates

OpenRig plugins come in two families. The basic ones (echo, reverb, EQ,
drive, gate, compressor…) are versatile tools and look like the rest of
the host. The signature ones are circuit models with a character of
their own; a host draws each as a faceplate, as a pedal has one: its own
colour and knobs, its controls in a fixed order, a tagline under its
name. The art direction stays the host's: the faceplate colour is the
only colour a plugin gives, and the host draws its accent, labels,
texture and corners over it.

```turtle
<https://openrig.eu/plugins/green-drive>
    openrig:family openrig:Signature ;
    openrig:faceColor "#2E5A3A" ;
    openrig:knobStyle openrig:Skirted ;
    openrig:finish openrig:Painted ;
    openrig:wear 0.35 ;
    openrig:plateName "GREEN" ;
    openrig:tagline "The mid-hump overdrive, in circuit" ;
    lv2:port [
        lv2:symbol "drive" ;
        openrig:primary 1 ;
        openrig:order 1
    ] , …
```

Rules:

- `openrig:family` is required for the other faceplate terms to apply;
  a plugin without it is drawn as the host draws any plugin.
- A signature plugin states `openrig:faceColor` and `openrig:knobStyle`.
  A basic one may state them; the host may ignore them.
- `openrig:order` places controls in reading order (left to right, then
  the next row); the host chooses the rows. Toggles and choices take a
  switch, the other controls a knob. Controls without an order are not
  on the faceplate, only in the full control list.
- `openrig:primary` still picks the knob strip; it is independent of
  the order.
- `openrig:tagline` is plain text, 48 characters at most, no final
  full stop.
- A signature plugin also states `openrig:finish`, `openrig:wear` and
  `openrig:plateName`. The plate name is the silkscreen on the faceplate
  (upper case reads best, "GREEN"); the plugin's `doap:name` stays its
  name everywhere else.
- `openrig:wear` is fixed per plugin, not per instance. The host derives
  the texture's random seed from the plugin URI, so scratches and worn
  edges sit at the same place on every board and every screen.
- `openrig:plateStyle` (signature plugins only, optional) sets the
  lettering of the plate name. The host keeps its own typeface and
  draws the style over it; without the property the name is printed
  plainly (`openrig:Silkscreen`). The offsets of `openrig:Glitch` are
  fractions of the cap height so it reads the same at every size, and
  a host shows it still: a faceplate does not move.
- The "on" LED is the host's, always in the app's green whatever the
  faceplate colour; a plugin does not choose it.
- Trust: a host draws a faceplate only for plugins whose URI starts with
  `https://openrig.eu/plugins/` (the OpenRig catalogue, whose owner runs
  the marketplace); other plugins stating these terms are drawn in the
  host's own look. No signature or receipt check is involved: the prefix
  is the whole rule.

Families, the values of `openrig:family`:

| Term | Meaning |
|---|---|
| `openrig:Basic` | A simple, versatile tool. The host draws it in its own look; openrig:faceColor and openrig:knobStyle are hints at most. |
| `openrig:Signature` | A plugin with a character of its own. The host draws it as a faceplate in its openrig:faceColor with openrig:knobStyle knobs. |

Finishes, the values of `openrig:finish`:

| Term | Meaning |
|---|---|
| `openrig:Painted` | Paint over the enclosure: an even colour with a slight grain. |
| `openrig:Brushed` | Bare brushed metal tinted by the colour: fine parallel streaks. |

Knob styles, the values of `openrig:knobStyle`:

| Term | Meaning |
|---|---|
| `openrig:Plain` | The host's own knob. |
| `openrig:Skirted` | A cap over a wider skirt marked with the scale, a line pointer on the cap. |
| `openrig:Chickenhead` | A tapered pointer knob: the position reads from the shape itself. |
| `openrig:Domed` | A smooth round cap with a dot or short line as the pointer. |

Plate styles, the values of `openrig:plateStyle`:

| Term | Meaning |
|---|---|
| `openrig:Silkscreen` | The plate name printed plainly in the host's lettering. The default. |
| `openrig:Glitch` | The plate name cut by two horizontal bands shifted sideways, as a corrupted picture: the band from 20 to 30 % of the cap height (from the baseline) moves right by 8 % of the cap height and takes the host's accent colour, the band from 62 to 70 % moves left by 4 %. Fixed, never animated. |

## Versioning

`owl:versionInfo` of the ontology is the vocabulary version. Additions
(terms, roles, views…) keep the version's meaning of every existing term;
a change of meaning would get a new term, never a new meaning for an old
one. Terms are not removed.
