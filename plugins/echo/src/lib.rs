//! OR Echo: a delay with three voicings, built for a pedalboard.
//!
//! - **Digital**: clean repeats. With the loop filters off and the
//!   modulation at 0, echo `n` of an input is exactly `level·feedbackⁿ⁻¹`
//!   times it, `n` delay times later.
//! - **Tape**: each pass through the loop goes through the heads' band
//!   (a 60 Hz low cut and a high cut that follows the tape speed: wider
//!   for short delays) and the tape's `tanh` saturation; the transport
//!   adds wow, flutter and a slow random drift.
//! - **Analog**: a bucket-brigade line. Its filters follow its clock, so
//!   the repeats darken as the time grows (from 9 kHz down to 1.2 kHz
//!   over the range of a 4096-bucket line), with the softer
//!   `x / (1 + |x|)` clipping and a chorus-like sweep.
//!
//! Saturation uses first-order antiderivative anti-aliasing
//! (`orx_dsp::adaa`), whose half-sample lag lengthens each tape or analog
//! repeat by half a sample (10 µs at 48 kHz). A peak limiter with an
//! immediate attack keeps the loop at or below 0 dBFS, so feedback above
//! 100 % builds a runaway that stays bounded; below that level it does
//! nothing and digital repeats stay exact.
//!
//! **Time changes** never click. Digital crossfades in 40 ms to a second
//! read head at the new time (changes during a crossfade wait for it to
//! end). Tape and analog glide: a critically damped motion of the read
//! head, its rate limited to ±50 %, bends the repeats in pitch like a tape
//! motor or a BBD clock settling. The time is the knob, or a note value
//! at the host tempo (up to 4 s); the `delay_ms` meter shows the time
//! playing now.
//!
//! **Filters in the loop**: a low cut and a high cut (Butterworth, 12
//! dB/octave) shape every repeat a little more, from the first one.
//!
//! **Ducking** lowers the echoes (not the loop) while the input is above
//! a threshold, with a 6 dB soft knee: they swell back when you stop.
//!
//! **Trails**: the dry signal is never processed, so with the runner's
//! trails bypass (`Plugin::TRAILS`) the echoes ring out over the dry
//! signal when the plugin is switched off, and the plugin stops once its
//! line holds only silence (below −100 dBFS for a whole delay time).
//!
//! The stereo variant runs a line per channel, or ping-pong: both inputs
//! summed into the left line, each line feeding the other. Every switch
//! (mode, ping-pong, filter on/off, dry) crossfades in 20 ms; every
//! parameter is smoothed per sample, and the output does not depend on
//! the host's block size.

mod echo;
mod info;
mod modulation;

pub use echo::Echo;
pub use info::{MAX_MS, MAX_SYNC_MS, MIN_MS, MONO, Mode, STEREO};

/// OR Echo, mono in and out
pub type Mono = Echo<1>;
/// OR Echo Stereo: stereo in and out, ping-pong
pub type Stereo = Echo<2>;
