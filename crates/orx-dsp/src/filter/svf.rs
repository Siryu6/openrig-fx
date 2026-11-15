//! Topology-preserving state variable filter (trapezoidal integration),
//! after V. Zavalishin, "The Art of VA Filter Design" (2012) and A. Simper,
//! "Linear trapezoidal integrated SVF" (Cytomic, 2013).
//!
//! It has the same transfer functions as the RBJ biquads (both are the
//! bilinear transform of the same analog prototypes, prewarped at the
//! centre frequency) but keeps its state meaningful when the coefficients
//! change, so frequency and Q can move every sample without zipper noise
//! or blow-ups.

use super::{Shape, prewarp, sane_db, sane_q};
use crate::math::flush_tiny;

/// Coefficients of an [`Svf`]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvfCoeffs {
    pub k: f32,
    pub a1: f32,
    pub a2: f32,
    pub a3: f32,
    /// Output mix of input, band and low pass
    pub m0: f32,
    pub m1: f32,
    pub m2: f32,
}

impl SvfCoeffs {
    /// Design a response. `q` is the cookbook Q (0.7071: Butterworth);
    /// `gain_db` is used by the bell and shelves only.
    pub fn new(shape: Shape, sample_rate: f64, freq: f64, q: f64, gain_db: f64) -> Self {
        let q = sane_q(q);
        let mut g = prewarp(freq, sample_rate);
        let a = 10f64.powf(sane_db(gain_db) / 40.0);
        let mut k = 1.0 / q;
        let (m0, m1, m2) = match shape {
            Shape::Lowpass => (0.0, 0.0, 1.0),
            Shape::Highpass => (1.0, -k, -1.0),
            Shape::Bandpass => (0.0, k, 0.0),
            Shape::Notch => (1.0, -k, 0.0),
            Shape::Allpass => (1.0, -2.0 * k, 0.0),
            Shape::Peak => {
                k = 1.0 / (q * a);
                (1.0, k * (a * a - 1.0), 0.0)
            }
            Shape::LowShelf => {
                g /= a.sqrt();
                (1.0, k * (a - 1.0), a * a - 1.0)
            }
            Shape::HighShelf => {
                g *= a.sqrt();
                (a * a, k * (1.0 - a) * a, 1.0 - a * a)
            }
        };
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        Self { k: k as f32, a1: a1 as f32, a2: a2 as f32, a3: a3 as f32, m0: m0 as f32, m1: m1 as f32, m2: m2 as f32 }
    }

    /// Butterworth low pass
    pub fn lowpass(sample_rate: f64, freq: f64) -> Self {
        Self::new(Shape::Lowpass, sample_rate, freq, core::f64::consts::FRAC_1_SQRT_2, 0.0)
    }
}

/// The three simultaneous outputs of the filter
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bands {
    pub low: f32,
    /// Band output (peak gain Q; multiply by `k` for 0 dB)
    pub band: f32,
    pub high: f32,
}

/// State variable filter
#[derive(Debug, Clone, Copy)]
pub struct Svf {
    c: SvfCoeffs,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    pub fn new(c: SvfCoeffs) -> Self {
        Self { c, ic1: 0.0, ic2: 0.0 }
    }

    #[inline]
    pub fn set(&mut self, c: SvfCoeffs) {
        self.c = c;
    }

    #[inline]
    pub fn coeffs(&self) -> &SvfCoeffs {
        &self.c
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    /// Low, band and high outputs for one input sample
    #[inline]
    pub fn bands(&mut self, v0: f32) -> Bands {
        let c = &self.c;
        let v3 = v0 - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = flush_tiny(2.0 * v1 - self.ic1);
        self.ic2 = flush_tiny(2.0 * v2 - self.ic2);
        Bands { low: v2, band: v1, high: v0 - c.k * v1 - v2 }
    }

    /// The designed response for one input sample
    #[inline]
    pub fn process(&mut self, v0: f32) -> f32 {
        let c = &self.c;
        let v3 = v0 - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a2 * v3;
        let v2 = self.ic2 + c.a2 * self.ic1 + c.a3 * v3;
        self.ic1 = flush_tiny(2.0 * v1 - self.ic1);
        self.ic2 = flush_tiny(2.0 * v2 - self.ic2);
        c.m0 * v0 + c.m1 * v1 + c.m2 * v2
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        for x in buf {
            *x = self.process(*x);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::biquad::{Biquad, Coeffs};
    use super::*;

    const FS: f64 = 48_000.0;

    /// The SVF and the RBJ biquad are the same filter
    #[test]
    fn matches_cookbook_biquads() {
        let shapes = [
            Shape::Lowpass,
            Shape::Highpass,
            Shape::Bandpass,
            Shape::Notch,
            Shape::Allpass,
            Shape::Peak,
            Shape::LowShelf,
            Shape::HighShelf,
        ];
        for shape in shapes {
            for (freq, q, db) in [(80.0, 0.707, 6.0), (1000.0, 2.5, -12.0), (12_000.0, 0.5, 9.0)] {
                let mut svf = Svf::new(SvfCoeffs::new(shape, FS, freq, q, db));
                let mut bq = Biquad::new(Coeffs::design(shape, FS, freq, q, db));
                let mut worst = 0.0f32;
                for n in 0..4096 {
                    let x = if n == 0 { 1.0 } else { 0.0 };
                    worst = worst.max((svf.process(x) - bq.process(x)).abs());
                }
                assert!(worst < 2e-5, "{shape:?} {freq} Hz: {worst}");
            }
        }
    }

    #[test]
    fn bands_rebuild_the_input() {
        let c = SvfCoeffs::new(Shape::Lowpass, FS, 500.0, 0.9, 0.0);
        let mut svf = Svf::new(c);
        for n in 0..2000 {
            let x = ((n * 7919) % 101) as f32 / 50.0 - 1.0;
            let b = svf.bands(x);
            assert!((b.low + c.k * b.band + b.high - x).abs() < 1e-5);
        }
    }

    #[test]
    fn stable_under_audio_rate_modulation() {
        let mut svf = Svf::new(SvfCoeffs::lowpass(FS, 1000.0));
        let mut peak = 0.0f32;
        for n in 0..96_000 {
            let f = 20.0 * 1000f64.powf(0.5 + 0.5 * (n as f64 * 0.01).sin());
            svf.set(SvfCoeffs::new(Shape::Lowpass, FS, f, 8.0, 0.0));
            let x = if (n / 37) % 2 == 0 { 1.0 } else { -1.0 };
            peak = peak.max(svf.process(x).abs());
        }
        assert!(peak.is_finite() && peak < 40.0, "{peak}");
    }
}
