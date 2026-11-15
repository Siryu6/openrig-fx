//! Biquad filter (RBJ "Audio EQ Cookbook" designs), transposed direct
//! form II.

use super::{Shape, prewarp, sane_db, sane_q};
use crate::math::flush_tiny;

/// Normalised coefficients (`a0` = 1)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

impl Coeffs {
    /// Pass-through
    pub const IDENTITY: Self = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    /// RBJ cookbook design. `gain_db` is used by the bell and shelves only.
    /// Shelves take `q` as the cookbook's Q (0.707 is the steepest without
    /// overshoot).
    pub fn design(shape: Shape, sample_rate: f64, freq: f64, q: f64, gain_db: f64) -> Self {
        let q = sane_q(q);
        // ω0 consistent with the prewarp clamp
        let w0 = 2.0 * prewarp(freq, sample_rate).atan();
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a = 10f64.powf(sane_db(gain_db) / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::Lowpass => {
                let b1 = 1.0 - cos;
                (b1 / 2.0, b1, b1 / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            Shape::Highpass => {
                let b1 = -(1.0 + cos);
                (-b1 / 2.0, b1, -b1 / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha)
            }
            Shape::Bandpass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::Notch => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::Allpass => (1.0 - alpha, -2.0 * cos, 1.0 + alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::Peak => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
            Shape::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - s),
                    (a + 1.0) + (a - 1.0) * cos + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - s,
                )
            }
            Shape::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - s),
                    (a + 1.0) - (a - 1.0) * cos + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - s,
                )
            }
        };
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    /// Complex response at `freq`: (magnitude, phase in radians)
    pub fn response(&self, freq: f64, sample_rate: f64) -> (f64, f64) {
        let w = core::f64::consts::TAU * freq / sample_rate;
        // z^-1 = e^{-jw}
        let (s1, c1) = (-w).sin_cos();
        let (s2, c2) = (-2.0 * w).sin_cos();
        let num = (self.b0 + self.b1 * c1 + self.b2 * c2, self.b1 * s1 + self.b2 * s2);
        let den = (1.0 + self.a1 * c1 + self.a2 * c2, self.a1 * s1 + self.a2 * s2);
        let mag = (num.0.hypot(num.1)) / den.0.hypot(den.1).max(1e-300);
        let phase = num.1.atan2(num.0) - den.1.atan2(den.0);
        (mag, phase)
    }

    /// Magnitude at `freq`, dB
    pub fn magnitude_db(&self, freq: f64, sample_rate: f64) -> f64 {
        20.0 * self.response(freq, sample_rate).0.max(1e-15).log10()
    }
}

/// Biquad in transposed direct form II, `f32` signal path
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    s1: f32,
    s2: f32,
}

impl Default for Biquad {
    fn default() -> Self {
        Self::new(Coeffs::IDENTITY)
    }
}

impl Biquad {
    pub fn new(c: Coeffs) -> Self {
        let mut b = Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, s1: 0.0, s2: 0.0 };
        b.set(c);
        b
    }

    /// New coefficients; the state is kept (fine for slow changes; use an
    /// SVF for fast modulation)
    pub fn set(&mut self, c: Coeffs) {
        self.b0 = c.b0 as f32;
        self.b1 = c.b1 as f32;
        self.b2 = c.b2 as f32;
        self.a1 = c.a1 as f32;
        self.a2 = c.a2 as f32;
    }

    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.s1;
        self.s1 = flush_tiny(self.b1 * x - self.a1 * y + self.s2);
        self.s2 = flush_tiny(self.b2 * x - self.a2 * y);
        y
    }

    pub fn process_block(&mut self, buf: &mut [f32]) {
        for x in buf {
            *x = self.process(*x);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f64 = 48_000.0;

    #[test]
    fn textbook_points() {
        let lp = Coeffs::design(Shape::Lowpass, FS, 1000.0, core::f64::consts::FRAC_1_SQRT_2, 0.0);
        assert!((lp.magnitude_db(1000.0, FS) + 3.0103).abs() < 0.01);
        assert!(lp.magnitude_db(10.0, FS).abs() < 0.01);
        let peak = Coeffs::design(Shape::Peak, FS, 2000.0, 1.0, 6.0);
        assert!((peak.magnitude_db(2000.0, FS) - 6.0).abs() < 1e-6);
        let shelf = Coeffs::design(Shape::LowShelf, FS, 200.0, 0.707, -9.0);
        assert!((shelf.magnitude_db(5.0, FS) + 9.0).abs() < 0.05);
        assert!(shelf.magnitude_db(20_000.0, FS).abs() < 0.05);
        let ap = Coeffs::design(Shape::Allpass, FS, 700.0, 0.5, 0.0);
        for f in [20.0, 700.0, 15_000.0] {
            assert!(ap.magnitude_db(f, FS).abs() < 1e-9);
        }
    }

    #[test]
    fn filters_a_sine() {
        let c = Coeffs::design(Shape::Highpass, FS, 1000.0, 0.707, 0.0);
        let mut b = Biquad::new(c);
        let mut peak = 0.0f32;
        for n in 0..48_000 {
            let x = (core::f64::consts::TAU * 100.0 * n as f64 / FS).sin() as f32;
            let y = b.process(x);
            if n > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        let expected = c.response(100.0, FS).0 as f32;
        assert!((peak - expected).abs() < 1e-3, "{peak} vs {expected}");
    }

    #[test]
    fn survives_nonsense() {
        let c = Coeffs::design(Shape::Peak, FS, f64::NAN, f64::INFINITY, f64::NAN);
        let mut b = Biquad::new(c);
        for _ in 0..1000 {
            assert!(b.process(1.0).is_finite());
        }
    }
}
