//! Real-time-safe DSP building blocks of OpenRig FX.
//!
//! Every type here follows the same contract:
//! - memory is allocated by constructors only (`new`, `with_*`), through
//!   [`alloc`], with the largest size the processor will ever need;
//! - per-sample and per-block methods never allocate, lock, or make system
//!   calls, and never panic on any input (out-of-range parameters are
//!   clamped, non-finite input is tolerated without poisoning the state
//!   forever: callers reset on non-finite output);
//! - coefficients are designed in `f64`, signals are processed in `f32`.

pub mod adaa;
pub mod alloc;
pub mod circuit;
pub mod delay;
pub mod denormal;
pub mod env;
pub mod filter;
pub mod lfo;
pub mod math;
pub mod noise;
pub mod oversample;
pub mod reader;
pub mod smooth;
pub mod xfade;

pub use denormal::FtzGuard;
pub use smooth::{Smoother, Smoothing};
