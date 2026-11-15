//! Test kit of OpenRig FX.
//!
//! - [`signal`]: deterministic test signals (sines, sweeps, impulses,
//!   noise, a plucked string);
//! - [`wav`] and [`golden`]: 32-bit float WAV files and golden-file
//!   comparison (`ORX_BLESS=1` rewrites the references);
//! - [`metrics`]: levels, differences, single-bin DFT;
//! - [`rt`]: an allocator that fails the test when the audio path
//!   allocates or frees memory;
//! - [`conformance`]: the suite every plugin passes (validation, RT
//!   safety under random automation, block-size independence, reset
//!   determinism, hostile inputs);
//! - [`ports_lock`]: the append-only check of port symbols and indices;
//! - [`bench`]: a small CPU benchmark harness.

pub mod bench;
pub mod conformance;
pub mod golden;
pub mod metrics;
pub mod ports_lock;
pub mod rt;
pub mod signal;
pub mod wav;

/// `ORX_BLESS` is set: rewrite golden files and port locks
pub fn bless() -> bool {
    std::env::var_os("ORX_BLESS").is_some_and(|v| !v.is_empty() && v != "0")
}
