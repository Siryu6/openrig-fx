//! Topology-preserving one-pole filters.

use super::prewarp;
use crate::math::flush_tiny;

/// One-pole low pass and high pass (TPT), 6 dB/octave.
///
/// In `f32` the low pass settles within about `1e-8 / (4·g)` of a constant
/// input (g = tan(π·f/fs)): fine for audio and smoothing, not for exact
/// DC removal at a few hertz (use [`DcBlocker`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct OnePole {
    /// g / (1 + g)
    gain: f32,
    state: f32,
}

impl OnePole {
    pub fn new(sample_rate: f64, freq: f64) -> Self {
        let mut f = Self::default();
        f.set_freq(sample_rate, freq);
        f
    }

    pub fn set_freq(&mut self, sample_rate: f64, freq: f64) {
        let g = prewarp(freq, sample_rate);
        self.gain = (g / (1.0 + g)) as f32;
    }

    pub fn reset(&mut self) {
        self.state = 0.0;
    }

    /// Low pass output
    #[inline]
    pub fn lowpass(&mut self, x: f32) -> f32 {
        let v = (x - self.state) * self.gain;
        let y = v + self.state;
        self.state = flush_tiny(y + v);
        y
    }

    /// High pass output (input minus the low pass)
    #[inline]
    pub fn highpass(&mut self, x: f32) -> f32 {
        x - self.lowpass(x)
    }
}

/// DC blocker: first-order high pass `y = g·(x − x[n−1]) + R·y[n−1]`.
///
/// Unlike `x − lowpass(x)`, which stalls a few 1e-5 away from zero in
/// `f32` when the cutoff is a few hertz, the differentiator form removes a
/// constant exactly.
#[derive(Debug, Clone, Copy, Default)]
pub struct DcBlocker {
    pole: f32,
    gain: f32,
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    /// High pass at `freq` (5 Hz is inaudible and settles in ~30 ms)
    pub fn new(sample_rate: f64, freq: f64) -> Self {
        let freq = if freq.is_finite() { freq.clamp(0.1, 0.1 * sample_rate) } else { 5.0 };
        let pole = (-core::f64::consts::TAU * freq / sample_rate).exp();
        Self { pole: pole as f32, gain: (0.5 * (1.0 + pole)) as f32, x1: 0.0, y1: 0.0 }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.gain * (x - self.x1) + self.pole * self.y1;
        self.x1 = x;
        self.y1 = flush_tiny(y);
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_power_at_cutoff() {
        let fs = 48_000.0;
        let mut lp = OnePole::new(fs, 1000.0);
        let mut peak = 0.0f32;
        for n in 0..48_000 {
            let x = (core::f64::consts::TAU * 1000.0 * n as f64 / fs).sin() as f32;
            let y = lp.lowpass(x);
            if n > 4800 {
                peak = peak.max(y.abs());
            }
        }
        assert!((peak - core::f32::consts::FRAC_1_SQRT_2).abs() < 2e-3, "{peak}");
    }

    #[test]
    fn blocks_dc() {
        let mut dc = DcBlocker::new(48_000.0, 5.0);
        let mut y = 1.0;
        for _ in 0..48_000 {
            y = dc.process(1.0);
        }
        assert!(y.abs() < 1e-6, "{y}");
    }
}
