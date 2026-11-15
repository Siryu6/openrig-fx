//! OR Green Drive: a circuit model of the classic mid-hump op-amp
//! overdrive, the green pedal behind countless lead tones.
//!
//! Where OR Drive's overdrive mode follows how that kind of circuit
//! behaves (a clean path plus a clipped, filtered one), this plugin
//! solves the circuit itself: the diodes in the op-amp's feedback are
//! an implicit equation solved by Newton's method at every sample, with
//! the feedback capacitor, the drive pot and the 720 Hz gain leg around
//! them, so the clipping softens, compresses and darkens with the drive
//! exactly as the parts make it. The tone network and its op-amp follow
//! as a linear circuit. See `circuit` for the topology and values.
//!
//! Voicings: Classic (the original values), Fat (100 nF in the gain leg
//! instead of 47 nF: the clipped band starts at 340 Hz instead of
//! 720 Hz) and Asymmetric (two diodes in series on one side: more
//! output, even harmonics), three common variants of the pedal.
//!
//! Aliasing: the circuit runs at four times the sample rate (IIR
//! half-band filters); the feedback capacitor and the tone stage's low
//! pass roll the harmonics off before the decimation. Zero latency: the
//! half-band filters are minimum-phase.

pub mod circuit;
mod green;
mod info;

pub use green::{GreenDrive, MAKEUP_DB};
pub use info::{GREEN_DRIVE, Voicing};
