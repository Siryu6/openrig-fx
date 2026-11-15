//! OR Plate: a plate reverb for guitar.
//!
//! The reverberator is Dattorro's figure-eight tank (see [`tank`]), the
//! classic structure of studio plate emulations: dense from the first
//! milliseconds, smooth, bright without metallic ringing thanks to two
//! slowly modulated all-passes inside the loop. It is set the way plate
//! units and modern pedals are (Decay, Pre-delay, Mix, Damping as a high
//! decay, Low end as a low decay, Mod, filters on the reverb), with the
//! decay in seconds for three bands, which is also what a host draws as
//! the decay curve view.
//!
//! - Pre-delay: a line per input before the diffusers, read with a cubic
//!   interpolator at a smoothed time (a change glides).
//! - Freeze: the input fades out and every loop gain fades to unity
//!   (60 ms), so the reverb holds; released, the decay takes over again.
//! - The filters on the reverb are state variable filters (12 dB/oct),
//!   crossfaded in and out so their "off" ends are exactly transparent.
//! - Mix: dry at unity and reverb rising up to 50 %, dry falling above.
//! - Trails: bypassed, the input fades out and the reverb rings out over
//!   the dry signal; the plugin stops once the tank holds less than
//!   −100 dBFS for a whole turn (a frozen tail is cut after 30 s).
//!
//! Cost: about 30 delay-line accesses a sample, no oversampling (the loop
//! has no non-linearity). Zero latency.

mod info;
mod plate;
mod tank;

pub use info::{MAX_DECAY_S, MAX_PREDELAY_MS, MIN_DECAY_S, PLATE};
pub use plate::Plate;
