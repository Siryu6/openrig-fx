//! OR Gate: a noise gate for high-gain rigs.
//!
//! Set like the gates of modern multi-effects (Threshold, Decay, Hold,
//! Range):
//!
//! - Detection: the peak level of the input (instant attack, 10 ms
//!   release, which bridges the zero crossings of low notes).
//! - Hysteresis: the gate opens above the threshold and closes 6 dB
//!   below it, after the hold time, so a fading note does not chatter.
//! - Gain: opens in 1 ms, linearly in amplitude (keeps the pick attack
//!   without a click); closes over the decay time, linearly in decibels
//!   (a natural fade). Range limits how far it closes: −90 dB mutes, a
//!   few dB make a gentle expander.
//! - Open, the gain is exactly 1: the signal passes bit for bit.
//! - A gain reduction meter feeds the host's dynamics view.
//!
//! Mono, zero latency, a few operations per sample.

mod gate;
mod info;

pub use gate::Gate;
pub use info::{FLOOR_DB, GATE};
