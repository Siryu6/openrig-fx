//! Filters.
//!
//! - [`svf::Svf`]: the workhorse. Topology-preserving (trapezoidal) state
//!   variable filter after Zavalishin and Simper: every RBJ response
//!   (low/high/band pass, notch, all-pass, bell, shelves) with stable,
//!   artefact-free modulation of frequency and Q at audio rate.
//! - [`biquad::Biquad`]: transposed direct form II, for fixed filters and
//!   for computing responses ([`biquad::Coeffs::response`]) that views can
//!   draw.
//! - [`onepole::OnePole`]: TPT one-pole low/high pass, DC blocker.
//! - [`bank::SvfBank`]: many SVFs side by side, vectorised (filter banks).

pub mod bank;
pub mod biquad;
pub mod onepole;
pub mod svf;

/// Response shapes shared by the biquad and the SVF
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Lowpass,
    Highpass,
    /// Band pass with 0 dB at the centre
    Bandpass,
    Notch,
    Allpass,
    /// Bell: `gain_db` at the centre
    Peak,
    LowShelf,
    HighShelf,
}

/// Warped analog frequency `tan(π·f/fs)` of the bilinear transform, with
/// `f` kept below Nyquist so the filter stays stable
#[inline]
pub(crate) fn prewarp(freq: f64, sample_rate: f64) -> f64 {
    let nyquist_guard = 0.4999 * sample_rate;
    let f = if freq.is_finite() { freq.clamp(1e-3, nyquist_guard) } else { 1000.0f64.min(nyquist_guard) };
    (core::f64::consts::PI * f / sample_rate).tan()
}

/// Q kept in a sane range (NaN → Butterworth)
#[inline]
pub(crate) fn sane_q(q: f64) -> f64 {
    if q.is_finite() { q.clamp(0.025, 100.0) } else { core::f64::consts::FRAC_1_SQRT_2 }
}

/// Gain in dB kept in a sane range (NaN → 0)
#[inline]
pub(crate) fn sane_db(db: f64) -> f64 {
    if db.is_finite() { db.clamp(-60.0, 60.0) } else { 0.0 }
}
