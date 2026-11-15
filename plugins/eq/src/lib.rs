//! OR EQ and OR Graphic EQ: zero-latency equalisers for guitar.
//!
//! - OR EQ is the parametric equaliser of modern multi-effects units: low
//!   and high cut (12 dB/oct, switchable), low and high shelves, three
//!   bells with their own Q, and an output level.
//! - OR Graphic EQ is the classic seven-band guitar graphic, one band per
//!   octave from 100 Hz to 6.4 kHz, with an output level.
//!
//! Each band is described once ([`design`]): the processor builds its
//! filters from that description and the `openrig:curve` descriptions a
//! host draws are generated from it, so the drawn curve is the response.
//!
//! - Filters: Simper's state variable filters (topology-preserving
//!   transform), whose magnitude is the cookbook biquad's and whose
//!   coefficients can move every sample without zipper noise or blowing
//!   up. While a control moves, its band's coefficients follow the
//!   smoothed value sample by sample; settled, they are left alone.
//! - A bell or shelf at 0 dB is exactly transparent; the cut filters fade
//!   in and out over 20 ms, so off is exactly transparent too.
//! - Mono: a host runs it dual-mono on a stereo chain.
//!
//! Cost: one state variable filter per band, nothing else. Zero latency.

pub mod design;
mod eq;
mod info;

pub use eq::{Design, Equalizer, GraphicDesign, ParametricDesign};
pub use info::{GAIN_DB, GRAPHIC, GRAPHIC_HZ, GRAPHIC_Q, PARAMETRIC};

/// OR EQ
pub type Parametric = Equalizer<ParametricDesign>;
/// OR Graphic EQ
pub type Graphic = Equalizer<GraphicDesign>;
