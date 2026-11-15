//! OR Splinter: tempo-synced glitch for live playing.
//!
//! - **HOLD** (a footswitch) plays the action: capture a slice (1/4 to
//!   1/32, straight or triplet) and repeat it, forward or backward, with
//!   a pitch step and a decay per repeat; or stop the tape over a number
//!   of beats, and spin it back up on release.
//! - **Quantize**: the press starts on the slice grid, from the host bar
//!   (or from a running chop). A press up to 40 ms late still starts on
//!   the line just passed, so a stomp on the beat is on the beat.
//! - **Chop**: a gate on a one-bar pattern (straight notes, gallop,
//!   offbeats, a breakdown) and **Crush** (sample-and-hold rate and bit
//!   reduction), while HOLD is held or always.
//! - **Chaos**: random reversed, octave, rolled or dropped repeats, and
//!   flipped chop steps, from a seed per capture.
//!
//! Every slice edge is crossfaded and the tape starts from the input
//! itself, so nothing clicks. No latency; with nothing engaged and Mix
//! at 100 % the output is the input, bit for bit. See `docs/DESIGN.md`.

pub mod clock;
pub mod crush;
mod info;
pub mod pattern;
pub mod ramp;
pub mod slicer;
mod splinter;
pub mod tape;

pub use info::{Action, BPM_MAX, BPM_MIN, GRID_CELLS, SPLINTER, Scope};
pub use splinter::Splinter;
