//! OR Black Dist: a circuit model of the classic op-amp hard-clipping
//! distortion, the black box with one big gain knob and a filter.
//!
//! A non-inverting op-amp stage with a 100 kΩ gain pot and two RC legs
//! to ground (47 Ω + 2.2 µF, 560 Ω + 4.7 µF: the gain rises above 60 Hz
//! and again above 1.5 kHz), modelled with its single-pole open loop
//! (1 MHz gain-bandwidth), its slew-rate limit (0.3 V/µs) and its rails:
//! at high gain the bandwidth closes and the edges round off, which is
//! much of the sound. Then 1 kΩ into two diodes to ground (hard
//! clipping, solved per sample by Newton's method), the passive filter
//! (1.5 kΩ plus a 100 kΩ pot into 3.3 nF) and the output buffer. See
//! `circuit` for the equations.
//!
//! Voicings: Silicon (the original diodes) and LED (the common variant
//! with LEDs: clipping at about 1.7 V, louder and more open).
//!
//! Aliasing: the circuit runs at eight times the sample rate (IIR
//! half-band filters). The diodes turn a loud note into a square wave
//! whose edges are shorter than a sample even there, and whose harmonics
//! fold back inside the oversampled rate already: at four times the
//! rate, a 2 kHz note at full distortion folds to −45 dB, at eight times
//! −58 to −70 dB. No antiderivative form applies to an implicit
//! equation, so the rate is the remedy. Zero latency.

mod black;
pub mod circuit;
mod info;

pub use black::{BlackDist, MAKEUP_DB};
pub use info::{BLACK_DIST, Voicing};
