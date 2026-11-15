//! OR Tremolo: the reference plugin of OpenRig FX.
//!
//! Three tremolo voicings, each a well-defined model rather than a circuit
//! simulation:
//!
//! - **Classic**: the amplitude follows the LFO linearly, `g = 1 − d·m`,
//!   with `m` the LFO drive (0 at its peak, 1 at its trough) and `d` the
//!   depth. Depth 0 is exactly transparent.
//! - **Opto**: the drive lights a lamp seen by a photoresistor, modelled as
//!   an asymmetric one-pole (4 ms when the light rises, 40 ms when it
//!   falls) followed by the cell's softer-than-linear curve,
//!   `g = 1 − d·ℓ^1.5`. Edges round off the way vintage amps' optical
//!   tremolos do, even with a square LFO.
//! - **Harmonic**: the signal is split by a Linkwitz-Riley crossover (two
//!   cascaded Butterworth TPT state variable filters per band) and the two
//!   bands are modulated in antiphase, the "harmonic vibrato" of early
//!   1960s amps. The bands sum to a second-order allpass, so depth 0 has a
//!   flat magnitude response.
//! - **Pan** (stereo only): equal-power auto-pan, unity at the centre.
//!
//! The LFO is `tanh(k·sin 2πφ)/tanh(k)` with `k = 16·s²` for the shape
//! amount `s`: a sine morphing to a rounded square, never a hard edge.
//! Its phase is kept in `f64`. It follows the host tempo when a note
//! value is chosen (the phase is free-running: hosts do not all send a
//! reliable bar position).
//!
//! Every parameter is smoothed per sample, mode changes crossfade in
//! 10 ms, and the output is identical whatever the host's block size.

mod info;
mod tremolo;

pub use info::{MONO, STEREO};
pub use tremolo::Tremolo;

/// OR Tremolo, mono in and out
pub type Mono = Tremolo<1>;
/// OR Tremolo Stereo: stereo in and out, LFO spread and auto-pan
pub type Stereo = Tremolo<2>;
