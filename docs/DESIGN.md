# Design of OpenRig FX

What the suite is made of, the choices behind it, and how a bundle is
checked before it ships.

## One description, every format

A plugin is a Rust type implementing `orx_plugin::Plugin`: a static
`PluginInfo` (ports, ranges, units, choices, smoothing, presets, view
metadata) and a real-time `process`. Nothing else describes it:

- the LV2 Turtle files (manifest, plugin, presets) are generated from
  `PluginInfo` by `orx_lv2::ttl`, so indices, symbols, ranges and presets
  cannot drift between code and metadata;
- the JACK standalone names its ports and parses `--set`, `--preset` and
  `--cc` from the same description;
- port indices and symbols are a public contract (presets and rigs refer
  to them): each plugin keeps a `ports.lock` file its tests compare
  against, and ports are only ever appended.

`orx_plugin::Runner` is the part of a host every format would otherwise
reimplement, so the LV2 wrapper, the JACK client and the tests all drive
plugins through it: block splitting with events moved to the right piece,
parameter sanitising (NaN, range, integer, choice), flush-to-zero, a
click-free 10 ms bypass on the `lv2:enabled` port with the dry signal
delayed by the reported latency, recovery from non-finite output (silence
and a reset, counted), latency reporting and rate-limited view streams.
Because the wrappers are thin, "the LV2 plugin sounds like the tests" is
checked rather than hoped: the tremolo's tests compare the C ABI output
with the runner's, and the OpenRig host check compares OpenRig's effect
chain with it, both bit-exact.

## A hand-written LV2 binding

`orx-lv2` is written against the LV2 C headers, without binding crates:

- the C surface is small (descriptor, instantiate, connect, run,
  activate, extension data, state, URID, atoms), and owning it keeps
  every `unsafe` block next to the reason it is sound;
- panics are caught at the C boundary (`panic = "unwind"` in release):
  a plugin that panics passes its input through from then on instead of
  taking the host down;
- `export_lv2!` turns a list of plugin types into the descriptor table of
  one cdylib, so a plugin's LV2 crate is one line.

`urid:map` is required only by plugins with atom ports or state, and
`lv2:hardRTCapable` is declared only when the plugin sets `hard_rt`, which
its conformance test enforces against the RT suite below. `cargo xtask
bundle` runs the plugin tests before building, so no bundle is assembled
from a plugin whose claim is untested.

## Real-time safety

The contract of `process` is no allocation, lock, system call, unbounded
loop or panic. `orx-dsp` types allocate in constructors only, for the
largest size they will ever need. `orx-test` enforces it:

- an allocator that fails the test when the audio path allocates or frees;
- random parameter automation, every preset, hostile inputs (full-scale
  DC and noise, denormals, a sub-sonic sine, NaN and infinity with
  recovery), reset determinism, and the same output whether the host
  sends blocks of 1, 7, 64 or 1000 frames.

### Bypass and trails

The `Runner` crossfades bypass changes. A plugin with `TRAILS` (the echo)
is not cut on bypass: its input fades out with the same gains as the
crossfade while its output keeps playing over the dry signal, so the
repeats ring out. It keeps running while `is_ringing()` says its loop
still holds sound, at most `TRAIL_LIMIT_S` (30 s, for a runaway
feedback), after which a 10 ms fade silences it and the runner goes idle.
Trails need a plugin without latency. The plate uses the same model. The conformance suite checks that
every preset of a trails plugin goes idle and silent after a bypass.

OR Splinter has no trails on purpose: a glitch replaces the signal, and a
stutter that kept playing after the footswitch turned it off would be a
fault on a stage. The runner crossfades its output instead (10 ms), and
the reset after bypass drops any capture.

## DSP choices

Coefficients are designed in `f64` and signals processed in `f32`; LFO
phases are kept in `f64`. Smoothing is per sample. Filters are TPT state
variable filters (stable under fast modulation). Models are stated as
equations in each plugin's documentation, and tests check them
analytically (exact nulls, an LFO against its closed form, an allpass sum
below -110 dB) as well as against golden files.

Saturation inside a feedback loop is anti-aliased with first-order
antiderivative anti-aliasing (`orx_dsp::adaa`): a curve supplies its
value and its integral, and the waveshaper outputs the divided difference
of the integral, computed in `f64` with a midpoint fallback for tiny
steps. It costs half a sample of delay, which the echo does not
compensate (it shifts the repeats by 10 µs at 48 kHz).

High-gain clipping (OR Drive) combines both remedies: the waveshaper
runs at four times the rate (two IIR half-band stages) through the same
first-order antiderivative form. On a 2 kHz tone at full gain the folded
power drops from −17 dB (plain curve at the sample rate) to −51 to −61 dB
of the output. A clean signal summed with a clipped one (overdrive) goes
through the same rate changes and the same half-sample mean, so the two
stay aligned.

Circuit models (OR Green Drive, OR Black Dist) solve the schematic instead of a curve.
Each capacitor is a trapezoidal companion (conductance `2C/T` and a
current source) whose voltage and current are the state, so the whole
circuit is its bilinear transform, and a control that changes a part
moves on from where the capacitors are. A diode pair in an op-amp's
feedback leaves one implicit equation per sample, `g·v + I(v) = i`,
monotonic in `v`: Newton's method from the extrapolated last two
samples, kept inside a bracket of the root (bisecting when a step leaves
it, and tightening it to where the diodes alone would carry the current
before any exponential could overflow), converges in about two
evaluations (`orx_dsp::circuit`). Linear parts (tone networks) solve in
closed form; an op-amp with its own limits (gain-bandwidth as a single
pole, slew rate, rails) is a trapezoidal step in closed form too. No
antiderivative form applies to an implicit equation, so the rate is the
remedy for aliasing: four times when the circuit's own capacitors roll
the harmonics off before the decimation (diodes in the feedback), eight
times when the diodes clip to ground into a square whose edges are
shorter than a sample. A small-signal test checks the
whole model against the circuit's nodal equations evaluated in the
frequency domain.

Pitch shifting (OR Drop) stays in the time domain, for two reasons.
Latency: a guitarist feels a down-tuning above about 10 ms, and a
frequency-domain shifter needs windows of several periods of the lowest
note (100 ms and more for a 41 Hz low E) to resolve it. And patents:
US 20250193632 / EP 4567785 (and the granted US 12689866), "filterbank
pitch shifter", cover splitting the input into a bank of band-pass
filters and resynthesising each band with oscillators at the shifted
frequencies, the approach the earlier OR Capo plan had chosen (sub-band
phase scaling). OR Drop has no filterbank, no analysis into bands, no
oscillator and no resynthesis: it reads the input from one delay line
at the pitch ratio with a four-point interpolator, and keeps the read
head around a fixed delay by jumping one detected period at a time,
crossfaded (pitch-synchronous overlap, the family of methods published
since the 1980s as PSOLA and WSOLA). Every output sample is input audio,
resampled and spliced. This is an engineering reading of the published
titles and abstracts, not a legal opinion; the claims should be read by
someone qualified before any commercial release.

The method in brief (`plugins/drop/src/shifter.rs`):

- The period is the shortest lag of the normalised autocorrelation, on
  a copy decimated to about 12 kHz, within 90 % of the best one, up to
  33 ms (a single note down to 30 Hz, or a power chord on B1, whose
  common period is two periods of the root). Every 256 samples, on the
  newest input; a lag at the full rate is refined at each splice where
  the head reads, made fractional with a parabola through the
  correlations around it (a whole-sample jump on a period of 145.6
  samples would raise the pitch by about 4 cents).
- The head drifts by `1 − r` samples per sample. Downwards it jumps one
  period towards the newest input when it is half a period past the
  centre, upwards one period back early enough that the fading-out head
  never overtakes the input. Crossfades are raised cosines as long as
  the period, at most 6 ms (Tight) or 12 ms (Quality).
- The centre is the latency reported on `lv2:latency`: 6 ms in Tight,
  14 ms in Quality. A downward jump needs a period of delay behind the
  newest input, so a period longer than twice the centre swings from
  the input to one period behind it: in Tight the delay follows the
  note below about 85 Hz (half a period on average: 7 ms on D2, 8 ms on
  B1, 12 ms on E1); in Quality it stays at 14 ms down to 36 Hz.
- The dry signal of the Mix is the input delayed by the same latency,
  so a blend stays in phase, and shift 0 is the plain delay, bit for
  bit.

Measured on steady synthetic tones (inharmonic power after the shift,
the residue of a least-squares fit of the expected harmonics): single
notes from F#1 to E4 and just-intoned power chords from B1 come out
below -54 dB in both modes, for -2, -5 and -12 semitones. Equal-tempered
chords are not periodic (the fifth beats against the root), so each
splice leaves a small seam: power chords at -36 dB (-2), -28 dB (-5)
and -23 dB (-12), a four-note major chord at -15, -9 and -4 dB. That is
the limit of any single-head time-domain shifter: Drop is for riffs,
single notes and power chords; full chords an octave down warble. Upward
shifts alias above `fs / (2r)` (the interpolator does not low-pass).

Reverb decay is set as a reverberation time and applied to every delay
element of a feedback loop, all-passes included (`z⁻¹ → λ·z⁻¹`), so
every mode decays at the set rate; the frequency-dependent part is a
pair of second-order shelves per loop, centred on the crossovers, so
the decay-curve view a host draws from the roles is the plate's.

Equaliser bands are described once (`orx_eq::design::BandDef`): the
processor builds its state variable filters from the description and the
`openrig:curve` statements are generated from it, so the curve a host
draws (cookbook responses at the declared values) is the response; a
test compares the plugin's measured impulse response with that product
within 0.02 dB. Smoothed controls move the coefficients sample by
sample, which the TPT structure tolerates without zipper noise.

Tempo divisions are one shared table (`orx_plugin::tempo`): the choice
values, labels and lengths in quarter notes are the same for every
tempo-synced plugin, so a host or a preset sees one meaning per value.

Glitch effects (OR Splinter) follow the tempo through
`orx_plugin::tempo`, which gained 1/32 and 1/32T for the slice lengths
(the echo's list keeps its twelve entries: `tempo::CHOICES` is the
delay-time list, `tempo::choice` builds any other). The clock counts
quarter notes in the bar. It locks to the host's bar only when the bar
position moves between two rolling transport events: a host that sends
a tempo with a fixed bar position (OpenRig sends bar beat 0 in every
block) gives the tempo but no grid, and the press of HOLD is then beat
one, as on a hardware pedal without clock. With a grid, a press waits
for the next line of the slice grid, or starts on the line just passed
when it comes at most 40 ms (and a quarter of the slice) late: the
capture is written forward from that line, the ring of the input
supplying the samples already gone, so a late foot still cuts on the
beat.

The engines are built not to click and to cost nothing when idle:

- *Repeats* read a captured slice with Hermite interpolation; every seam
  is a raised-cosine crossfade of up to 3 ms (shorter for short slices)
  between voices (three at most, the quietest stolen), and repeat `n`
  starts exactly at `n` slice lengths, so a roll does not drift from the
  grid however long it is held. The decay (dB per repeat) and the pitch
  step (semitones per repeat, speed clamped to a quarter and four times)
  apply per repeat; Repeats 0 is endless.
- *Tape stop* reads the ring at a speed falling linearly to 0 over
  Stop Time beats; the head starts on the current sample, so the stop
  starts without any change in the sound, and the level follows the
  square root of the speed, so the end fades out instead of holding a
  DC step. Tape Start spins back up in half the stop time and joins the
  input with a 10 ms crossfade.
- *Chop* is a gate over a one-bar pattern (1/8, 1/16, triplets, gallop,
  offbeat, breakdown 3-3-2) with a duty per step and 2 ms edges.
- *Crush* holds samples at the rate set (no interpolation, the aliasing
  is the point) and rounds to the bit depth; at 16 bits and 48 kHz it
  is skipped.
- *Chaos* is drawn from a generator seeded anew for each capture (from
  the number of captures since activation): each repeat may turn
  backward, jump an octave, roll at twice or four times the rate or drop
  out, and each chop step may flip. Renders are repeatable, which keeps
  the golden files and the conformance checks deterministic.

At rest, mix 100, the output is the input bit for bit (the blends are
written `a·g + b·(1 − g)` so that both ends are exact); Splinter has no
latency.

SIMD: plain loops over structure-of-arrays chunks, which LLVM vectorises.
That covers aarch64 (NEON is baseline) and the OpenRig x86_64 image
(x86-64-v3). Generic x86_64 builds pick AVX2/FMA kernels at run time for
the few loops where it matters (`orx_simd::kernel!`); without `mul_add`
both paths give the same per-lane results.

## Two families: basic and signature

The basic plugins (Echo, Plate, EQ, Drive, Gate, Comp, Tremolo) are
versatile and neutral: they cover a wide range and sit in the host's
own look. The signature plugins are circuit models with a character:
the faithful circuit is the core, and one or two OpenRig touches give
it an identity, voiced with modern heavy music in mind (low tunings,
tight high gain, big clean and ambient parts). Each signature plugin is
drawn as a faceplate of its own (`openrig:family`, `openrig:faceColor`,
`openrig:knobStyle`, `openrig:order`, `openrig:tagline`; see
[OPENRIG-VIEWS.md](OPENRIG-VIEWS.md#faceplates)).

Voicing choices so far:

- **OR Green Drive.** In heavy music the mid-hump overdrive is mostly a
  boost in front of a high-gain amp: drive near 0, level up, the low end
  kept out of the amp's clipping. The circuit already boosts the mids
  and passes the lows at unity; the OpenRig touch, **Tight**, takes the
  lows out of that unity path too: two coupling sections ganged on one
  pot before the clipping stage, a 12 dB/octave high pass swept from 5 Hz
  (out of the way, even for a 41 Hz low E an octave down) to 700 Hz. The
  sweet spot is the "Tight Boost" preset: drive 0, tone 55 (bright
  enough to cut, short of fizz), level +9 dB, Tight 250 Hz, where a low E
  comes out about 20 dB lower and 1 kHz under 1 dB lower. Fat and
  Asymmetric stay as the historic variants.
- **OR Black Dist.** The OpenRig touch is the diode selector, each
  choice solved by the same circuit (the diode pair's Newton solve, or a
  plain divider without diodes), level-matched at the middle of
  Distortion so that switching compares characters, not loudness:
  - *Silicon*, the original: clipping at about 0.6 V, the tightest and
    most compressed, for fast low riffs;
  - *LED*: clipping at about 1.7 V (-7.3 dB of compensation), more open;
    a quiet note comes out 9.9 dB under a loud one against 3.4 dB through
    silicon, so picking dynamics survive;
  - *None*: no diodes, the op-amp clips on its ±3.5 V rails with its
    0.3 V/µs slew rate rounding the edges (-11.4 dB of compensation);
    rawer and fuller, the closest to a fuzzy op-amp crunch.
  At eight times the rate, a 2 kHz note at full distortion aliases at
  -58 to -64 dB (silicon), -64 to -71 dB (LED) and -73 to -80 dB (none:
  the slew rate softens the edges).
- **OR Drop.** The signature pitch pedal for low tunings: Drop D to
  Drop A and an octave down from a standard-tuned guitar, and dives.
  The OpenRig touches are the explicit latency choice (Tight for the
  feel, Quality for 7- and 8-string guitars and octaves), reported to
  the host so a parallel path stays aligned, and the momentary Dive:
  a toggle meant for a momentary footswitch, gliding to its own shift
  over Dive Time and back (a dive bomb at -24, a one-octave
  rise at +12). Faceplate: oxblood (`#4A1418`), painted,
  heavily worn (0.6), chickenhead knobs.
- **OR Splinter.** The tempo-synced glitch, for the stutters, tape stops
  and chopped breakdowns of metalcore, deathcore and breakcore. What
  guitarists use live, from hardware stutter and slicer pedals and from
  the stutter and gross-beat style effects of DAWs: one footswitch that
  does one thing on the beat (repeat, reverse or tape stop while held,
  momentary or latched by the host), the slice length from the tempo
  (1/4 to 1/32, triplets included), a repeat count with a decay and a
  pitch step per repeat for falling stutters and rising rolls, a gate
  pattern and a crusher. Left out: sequencers of many steps, per-step
  editing and scene memory, which belong to a DAW and not to a foot.
  The OpenRig touches are the Breakdown pattern (3-3-2 groups of
  sixteenths, the chug of a breakdown), Chaos (repeatable random
  reverses, octaves, rolls and dropouts), the capture that forgives a
  late foot (above), and Chop and Crush scoped to HOLD or always on.
  Faceplate: brushed gunmetal (`#3A3D42`), heavily worn (0.7), steel
  skirted knobs, the plate name drawn with the Glitch plate style, and
  the step grid view: the current bar, the slice, its repeats and their
  levels.

## Native views

The `openrig:` vocabulary ([OPENRIG-VIEWS.md](OPENRIG-VIEWS.md)) lets a
plugin state the view a host should draw, the role of each port, exact
curves and data streams. Its schema is generated from `orx_plugin::view`
(`cargo xtask vocab`); tests check that the committed schema is current,
that the generated plugin files use only defined terms, and that the
specification covers every term.

## Binary compatibility with OpenRig

The OpenRig OS image ships glibc 2.41. A bundle may only need `libc`,
`libm`, `libgcc_s`, `libdl`, `libpthread` and the dynamic loader, and no
symbol newer than the image's glibc; `xtask` reads both from the ELF
file. The tremolo and the echo need GLIBC_2.33 at most. `log10f` is
avoided: glibc 2.43 versioned it anew, so `gain_to_db` uses `logf`.
Cross builds for the Raspberry Pi 5 link against Debian bookworm's arm64
glibc (2.36) (`scripts/aarch64-sysroot.sh`), so they can never require
more than the image has.

## Validation pipeline

`cargo xtask bundle [--target T] [--strict]`:

| Step | Tool | Checks |
|---|---|---|
| tests | cargo | every test of the bundle's plugins (conformance, RT, golden, nulls) |
| build | cargo | release cdylib (fat LTO, one codegen unit, stripped) |
| turtle | generator | the files in the bundle are what the registry generates today |
| vocab | `orx_lv2::vocab` | no undefined `openrig:` term |
| syntax | serdi | every Turtle file parses |
| schema | lv2_validate | LV2, doap, foaf and `openrig:` schemas: no error, no warning |
| abi, glibc | readelf | allowed libraries only, newest GLIBC symbol within the image's |
| load | dlopen | descriptors in registry order; every plugin at 44.1, 48 and 96 kHz, in place and not, every preset: finite, bounded output |
| lv2lint | lv2lint | `-M pack`, warnings are errors |
| jalv | jalv, jackd | on a private `jackd -d dummy`: audio through the plugin; with `enabled=0`, the output is the input delayed by whole periods, bit-exact |

The last three need a binary of the build machine; for a foreign target
they are reported as skipped, and `--strict` turns any skip into a
failure.

`cargo xtask openrig-host OPENRIG_DIR` adds OpenRig's own host: what its
discovery reads (ports, ranges, properties, units, choices, presets,
transport input) against the description, then the audio through its
`EffectChain` against the runner, bit-exact, with tempo, parameter
changes, the plugin's own bypass, every preset, and a mono plugin run
dual mono after a stereo one.

### Notes on jalv

jalv 1.10 is driven with `-i` (no console, so SIGINT stops it) and `-x`
(fixed client name). Its console view of control values is not used: in
our runs it showed values that did not match the ports being set, while
audio and `-c SYMBOL=VALUE` behaved correctly. The jalv check therefore
judges the audio only.

## The JACK standalone

`orx-standalone` runs one plugin as a JACK client through the runner.
libjack is loaded at run time (the `jack` crate with dynamic loading, as
OpenRig does), so builds need no JACK files. Control changes reach the
audio thread through a wait-free SPSC queue; meters come back through
atomics. MIDI CCs map to parameters over their range, MIDI events are
forwarded to plugins that take them, and the JACK transport (or
`--bpm`) becomes `time:Position` events, sent when it changes. Its
end-to-end test runs `orx-run` on a dummy server and checks the
modulation envelope and a live parameter change from the recorded audio.
