//! OR Comp: a guitar compressor.
//!
//! Set like the studio-style compressors of modern multi-effects
//! (Threshold, Ratio, Attack, Release, Level, Blend) rather than a
//! single Sustain knob: a sustainer is a low threshold and a high ratio
//! (see the presets), and Blend gives the parallel compression of the
//! pedals that keep the pick attack in the dry signal.
//!
//! - Gain computer in decibels, feed-forward, with a quadratic soft knee
//!   ([`curve`], the static curve a host draws in its dynamics view).
//! - Detector: the peak of the input, in decibels; the gain reduction is
//!   smoothed with separate attack and release poles (smooth branching),
//!   so the release does not depend on the attack.
//! - Below the threshold and the knee, with no makeup, the signal passes
//!   bit for bit; a gain reduction meter feeds the dynamics view.
//!
//! Mono, zero latency.

mod comp;
mod info;

pub use comp::{Comp, curve};
pub use info::{COMP, METER_DB};
