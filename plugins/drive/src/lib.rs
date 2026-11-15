//! OR Drive: boost, overdrive, distortion and fuzz in one plugin.
//!
//! Each mode follows how its circuit behaves, with generic names:
//!
//! - Boost: a clean gain stage (0 to +20 dB) that rounds off softly
//!   (`tanh`) only near full scale; treble shelf as the tone.
//! - Overdrive: an op-amp with diodes in its feedback. The output is the
//!   clean signal plus a clipped copy of a high-passed (720 Hz) gain path,
//!   whose gain and low-pass corner follow a drive pot in series with
//!   51 kΩ and 51 pF: the mid-focused, never fully clipped sound; treble
//!   cut as the tone.
//! - Distortion: a high-gain op-amp (6 to 66 dB) whose gain-bandwidth
//!   product darkens it as the gain rises, then hard diodes to ground
//!   (cubic knee); a low-pass filter as the tone.
//! - Fuzz: a two-transistor stage (20 to 60 dB) clipping hard on one side
//!   and softly on the other around a bias: even harmonics and a thick
//!   compression; the tone blends a low pass into a high pass through a
//!   mid scoop.
//!
//! A shared low shelf before the clipping sets how tight or thick the
//! drive is (Bass), and the output level is matched across modes at the
//! middle of Drive.
//!
//! Aliasing: the clipping runs at four times the sample rate (IIR
//! half-band filters) through a first-order antiderivative waveshaper
//! (`orx_dsp::adaa`). The pair costs far less than 16× oversampling and
//! keeps folded harmonics far below the real ones (see the tests).
//!
//! Mode changes crossfade over 30 ms between the two circuits. Zero
//! latency: the half-band filters are minimum-phase, a few samples of
//! group delay at the top of the band only.

mod circuit;
mod drive;
mod info;

pub use circuit::Fuzz;
pub use drive::Drive;
pub use info::{DRIVE, Mode};
