//! OR Drop: down-tuning, octaves and momentary dives for guitar, in the
//! time domain.
//!
//! The input goes into a delay line read at the pitch ratio; the read
//! head is kept around a fixed delay (the latency, reported to the host)
//! by jumps of one detected period, crossfaded. The output is always the
//! input's own waveform, resampled and spliced: no analysis filterbank,
//! no oscillators, no resynthesis. See `shifter` for the method and
//! `docs/DESIGN.md` for why it was chosen.
//!
//! - Shift: −24 to +12 semitones, with a short glide while turning it.
//! - Dive: a toggle (a momentary footswitch) gliding Dive To semitones
//!   away from Shift over Dive Time, and back.
//! - Mode: Tight (6 ms) or Quality (14 ms, whole-period splices down to
//!   36 Hz, for extended-range guitars and octaves down).
//! - Mix: dry and shifted, the dry delayed by the same latency so they
//!   stay in phase.

mod info;
mod pedal;
pub mod shifter;

pub use info::{DROP, Mode, SHIFT_MAX, SHIFT_MIN};
pub use pedal::{DropPedal, SHIFT_GLIDE_S};
