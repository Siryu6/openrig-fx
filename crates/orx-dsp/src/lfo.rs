//! Low-frequency oscillators.
//!
//! The phase is kept in `f64` turns: with `f32`, a 0.1 Hz LFO at 48 kHz
//! would lose up to a few percent of its rate to rounding.

use crate::math::{TAU, sin_turns};

/// Phase accumulator in turns, `[0, 1)`
#[derive(Debug, Clone, Copy, Default)]
pub struct Phasor {
    phase: f64,
    inc: f64,
}

impl Phasor {
    pub fn new(rate_hz: f64, sample_rate: f64) -> Self {
        let mut p = Self::default();
        p.set_rate(rate_hz, sample_rate);
        p
    }

    /// Rate in Hz (non-finite or negative: stopped)
    #[inline]
    pub fn set_rate(&mut self, rate_hz: f64, sample_rate: f64) {
        self.inc = if rate_hz.is_finite() && rate_hz > 0.0 && sample_rate > 0.0 {
            (rate_hz / sample_rate).min(0.5)
        } else {
            0.0
        };
    }

    /// Current phase, then advance by one sample
    #[inline]
    pub fn next(&mut self) -> f64 {
        let p = self.phase;
        self.phase += self.inc;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        p
    }

    #[inline]
    pub fn phase(&self) -> f64 {
        self.phase
    }

    /// Jump to `phase` (any value: wrapped to [0, 1))
    pub fn set_phase(&mut self, phase: f64) {
        self.phase = if phase.is_finite() { phase.rem_euclid(1.0) } else { 0.0 };
    }

    /// Phase increment per sample (turns)
    pub fn increment(&self) -> f64 {
        self.inc
    }
}

/// Sine, `phase` in turns, output in [-1, 1]
#[inline]
pub fn sine(phase: f32) -> f32 {
    sin_turns(phase)
}

/// Triangle starting at 0 and rising, [-1, 1]
#[inline]
pub fn triangle(phase: f32) -> f32 {
    let p = phase - phase.floor();
    let t = 4.0 * p;
    if p < 0.25 {
        t
    } else if p < 0.75 {
        2.0 - t
    } else {
        t - 4.0
    }
}

/// Sine morphing to a rounded square as `k` grows: `tanh(k·sin)/tanh(k)`.
/// `k` ≤ 0.01 is a sine. The edges stay smooth (no clicks) for any `k`.
#[inline]
pub fn sine_to_square(phase: f32, k: f32) -> f32 {
    let s = sin_turns(phase);
    if k <= 0.01 {
        return s;
    }
    (k * s).tanh() / k.tanh()
}

/// Steepness `k` of [`sine_to_square`] for a shape amount in [0, 1]
/// (0: sine, 1: square-ish with edges about 2 % of the period)
#[inline]
pub fn shape_steepness(amount: f32) -> f32 {
    let a = amount.clamp(0.0, 1.0);
    // Quadratic so the first half of the knob stays round
    a * a * 16.0
}

/// Angular helper: radians of a phase in turns
#[inline]
pub fn turns_to_radians(turns: f32) -> f32 {
    turns * TAU
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_rates_stay_exact() {
        let mut p = Phasor::new(0.1, 48_000.0);
        for _ in 0..480_000 * 2 {
            p.next();
        }
        // 20 s at 0.1 Hz: exactly two turns
        assert!(p.phase() < 1e-6 || p.phase() > 1.0 - 1e-6, "{}", p.phase());
    }

    #[test]
    fn shapes_stay_in_range() {
        for i in 0..1000 {
            let ph = i as f32 / 1000.0;
            for k in [0.0, 1.0, 4.0, 16.0] {
                let v = sine_to_square(ph, k);
                assert!((-1.0..=1.0).contains(&v));
            }
            assert!((-1.0..=1.0).contains(&triangle(ph)));
        }
        assert!((sine_to_square(0.25, 16.0) - 1.0).abs() < 1e-6);
        assert!((triangle(0.25) - 1.0).abs() < 1e-6);
    }
}
