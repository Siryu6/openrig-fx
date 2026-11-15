//! Delay lines with fractional reads.
//!
//! A [`DelayLine`] is a power-of-two ring buffer sized at construction for
//! the longest delay it will serve. Convention: [`DelayLine::write`] the
//! current input first, then read; `read(0)` is that input, `read(d)` the
//! input of `d` samples ago.
//!
//! Interpolators, from cheapest to best:
//! - [`DelayLine::read_linear`]: 2 points; dulls highs (-3.9 dB at
//!   Nyquist/2 with half-sample delay). Fine for slow chorus.
//! - [`DelayLine::read_hermite`]: 4-point cubic Hermite (Catmull-Rom);
//!   the usual choice for modulated delays.
//! - [`DelayLine::read_lagrange`]: 4-point third-order Lagrange; flatter
//!   magnitude than Hermite, slightly more ripple in phase.
//! - [`AllpassReader`]: first-order Thiran all-pass; flat magnitude, for
//!   fixed or slowly moving delays inside feedback loops (reverbs, physical
//!   models). Not for fast modulation (its state lags).

use crate::alloc;

/// Ring buffer of samples
pub struct DelayLine {
    buf: Box<[f32]>,
    mask: usize,
    /// Index of the last written sample
    pos: usize,
}

/// Points around the read position used by 4-point interpolators
const GUARD: usize = 4;

impl DelayLine {
    /// A line able to delay by up to `max_delay` samples (any read method)
    pub fn new(max_delay: usize) -> Self {
        let len = (max_delay.saturating_add(GUARD)).next_power_of_two().max(8);
        Self { buf: alloc::zeroed(len), mask: len - 1, pos: 0 }
    }

    /// Longest delay a read may ask for (cubic reads: one less)
    pub fn max_delay(&self) -> usize {
        self.buf.len() - GUARD
    }

    pub fn reset(&mut self) {
        self.buf.fill(0.0);
        self.pos = 0;
    }

    /// Append the current input sample
    #[inline]
    pub fn write(&mut self, x: f32) {
        self.pos = (self.pos + 1) & self.mask;
        // `pos` is masked: always in range
        if let Some(slot) = self.buf.get_mut(self.pos) {
            *slot = x;
        }
    }

    /// Sample written `d` writes ago (`d` clamped to the capacity)
    #[inline]
    pub fn read(&self, d: usize) -> f32 {
        let d = d.min(self.mask);
        self.buf.get((self.pos.wrapping_sub(d)) & self.mask).copied().unwrap_or(0.0)
    }

    /// Integer part (clamped) and fraction of a delay, for a reader that
    /// needs `before` points before it and `after` after
    #[inline]
    fn split(&self, d: f32, min: f32) -> (usize, f32) {
        let max = (self.max_delay() - 1) as f32;
        let d = if d >= min { if d <= max { d } else { max } } else { min };
        let i = d as usize; // d ≥ 0: truncation is floor
        (i, d - i as f32)
    }

    /// Linear interpolation, `d` ≥ 0
    #[inline]
    pub fn read_linear(&self, d: f32) -> f32 {
        let (i, t) = self.split(d, 0.0);
        let a = self.read(i);
        let b = self.read(i + 1);
        a + (b - a) * t
    }

    /// 4-point cubic Hermite (Catmull-Rom), `d` ≥ 1
    #[inline]
    pub fn read_hermite(&self, d: f32) -> f32 {
        let (i, t) = self.split(d, 1.0);
        let (xm1, x0, x1, x2) = (self.read(i - 1), self.read(i), self.read(i + 1), self.read(i + 2));
        let c1 = 0.5 * (x1 - xm1);
        let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
        let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
        ((c3 * t + c2) * t + c1) * t + x0
    }

    /// 4-point third-order Lagrange, `d` ≥ 1
    #[inline]
    pub fn read_lagrange(&self, d: f32) -> f32 {
        let (i, t) = self.split(d, 1.0);
        let (xm1, x0, x1, x2) = (self.read(i - 1), self.read(i), self.read(i + 1), self.read(i + 2));
        // Nodes at -1, 0, 1, 2; evaluate at t
        let (tm1, t0, t1, t2) = (t + 1.0, t, t - 1.0, t - 2.0);
        let lm1 = -t0 * t1 * t2 / 6.0;
        let l0 = tm1 * t1 * t2 / 2.0;
        let l1 = -tm1 * t0 * t2 / 2.0;
        let l2 = tm1 * t0 * t1 / 6.0;
        xm1 * lm1 + x0 * l0 + x1 * l1 + x2 * l2
    }
}

/// First-order all-pass (Thiran) fractional reader of a [`DelayLine`]
#[derive(Debug, Clone, Copy, Default)]
pub struct AllpassReader {
    prev: f32,
}

impl AllpassReader {
    pub fn reset(&mut self) {
        self.prev = 0.0;
    }

    /// Read at delay `d` ≥ 1. Call once per sample, after the write.
    #[inline]
    pub fn read(&mut self, line: &DelayLine, d: f32) -> f32 {
        let (mut i, mut f) = line.split(d, 1.0);
        // Keep the fraction in [0.1, 1.1): the pole stays away from -1
        if f < 0.1 && i > 1 {
            i -= 1;
            f += 1.0;
        }
        let eta = (1.0 - f) / (1.0 + f);
        let y = eta * (line.read(i) - self.prev) + line.read(i + 1);
        self.prev = crate::math::flush_tiny(y);
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts::TAU;

    #[test]
    fn integer_delay_is_exact() {
        let mut line = DelayLine::new(100);
        for n in 0..300 {
            line.write(n as f32);
            if n >= 100 {
                assert_eq!(line.read(100), (n - 100) as f32);
                assert_eq!(line.read_hermite(37.0), (n - 37) as f32);
                assert_eq!(line.read_lagrange(37.0), (n - 37) as f32);
                assert_eq!(line.read_linear(0.0), n as f32);
            }
        }
    }

    /// Error of each interpolator on a delayed sine, against the exact one
    fn error(read: impl Fn(&DelayLine) -> f32, freq: f64, d: f64) -> f64 {
        let fs = 48_000.0;
        let mut line = DelayLine::new(64);
        let mut worst = 0.0f64;
        for n in 0..2000 {
            line.write((TAU * freq * n as f64 / fs).sin() as f32);
            if n > 100 {
                let exact = (TAU * freq * (n as f64 - d) / fs).sin();
                worst = worst.max((read(&line) as f64 - exact).abs());
            }
        }
        worst
    }

    #[test]
    fn fractional_reads_are_accurate() {
        let d = 10.37;
        let lin = error(|l| l.read_linear(d as f32), 1000.0, d);
        let her = error(|l| l.read_hermite(d as f32), 1000.0, d);
        let lag = error(|l| l.read_lagrange(d as f32), 1000.0, d);
        assert!(lin < 1e-2, "linear {lin}");
        assert!(her < 2e-4, "hermite {her}");
        assert!(lag < 1e-4, "lagrange {lag}");
        assert!(lag < lin && her < lin);
    }

    #[test]
    fn allpass_has_the_right_phase_delay() {
        let d = 10.6f32;
        let mut reader = AllpassReader::default();
        let fs = 48_000.0;
        let freq = 200.0;
        let mut line = DelayLine::new(64);
        let mut worst = 0.0f64;
        for n in 0..20_000 {
            line.write((TAU * freq * n as f64 / fs).sin() as f32);
            let y = reader.read(&line, d);
            if n > 10_000 {
                let exact = (TAU * freq * (n as f64 - d as f64) / fs).sin();
                worst = worst.max((y as f64 - exact).abs());
            }
        }
        assert!(worst < 1e-3, "{worst}");
    }

    #[test]
    fn clamps_out_of_range_delays() {
        let mut line = DelayLine::new(16);
        line.write(1.0);
        for d in [-5.0, f32::NAN, 1e9, f32::INFINITY] {
            assert!(line.read_hermite(d).is_finite());
            assert!(line.read_linear(d).is_finite());
        }
        assert!(line.read(usize::MAX).is_finite());
    }
}
