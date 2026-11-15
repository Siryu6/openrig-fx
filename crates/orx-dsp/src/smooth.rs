//! Parameter smoothing.
//!
//! Control values arrive once per block; applied as steps they click
//! (gain), zipper (filter frequency) or jump (delay time). A [`Smoother`]
//! turns each new target into a ramp:
//!
//! - [`Smoothing::Linear`]: straight line reaching the target in exactly the
//!   given time. Right for mix and gain amounts, and anything that must
//!   arrive on time.
//! - [`Smoothing::Exponential`]: one-pole glide, within -60 dB of the target
//!   after the given time. Feels natural on knobs turned by hand; never
//!   overshoots.
//! - [`Smoothing::Multiplicative`]: straight line on a log scale (constant
//!   ratio per sample). Right for frequencies, rates and times: a sweep
//!   from 100 Hz to 10 kHz spends as long in each octave. Falls back to
//!   linear when a value is not strictly positive.

/// How a parameter moves to a new value. Times are in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Smoothing {
    /// Jump at once (switches, choices)
    None,
    Linear {
        ms: f32,
    },
    Exponential {
        ms: f32,
    },
    Multiplicative {
        ms: f32,
    },
}

impl Smoothing {
    /// The ramp time in milliseconds (0 for `None`)
    pub const fn ms(self) -> f32 {
        match self {
            Smoothing::None => 0.0,
            Smoothing::Linear { ms } | Smoothing::Exponential { ms } | Smoothing::Multiplicative { ms } => ms,
        }
    }
}

/// A smoothed value. Real-time safe; [`Smoother::next`] costs a compare and
/// one multiply or add.
#[derive(Debug, Clone)]
pub struct Smoother {
    kind: Smoothing,
    sample_rate: f32,
    current: f32,
    target: f32,
    /// Linear ramp: increment; log ramp: ratio; exponential: pole
    step: f32,
    /// The running ramp multiplies (log scale) instead of adding
    ratio: bool,
    /// Samples left on a linear or log ramp
    remaining: u32,
}

/// Exponential smoothing reaches -60 dB of the step after the given time
const LN_1000: f32 = 6.907_755;

impl Smoother {
    pub fn new(kind: Smoothing, sample_rate: f32, initial: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() { sample_rate.max(1.0) } else { 48_000.0 };
        let initial = if initial.is_finite() { initial } else { 0.0 };
        let step = match kind {
            Smoothing::Exponential { ms } => {
                let samples = (ms * 0.001 * sample_rate).max(1.0);
                (-LN_1000 / samples).exp()
            }
            _ => 0.0,
        };
        Self { kind, sample_rate, current: initial, target: initial, step, ratio: false, remaining: 0 }
    }

    /// Samples of a linear or log ramp
    pub fn ramp_len(&self) -> u32 {
        (self.kind.ms() * 0.001 * self.sample_rate).round().clamp(1.0, 1e9) as u32
    }

    /// Start moving to `target`. A non-finite target is ignored.
    #[inline]
    pub fn set_target(&mut self, target: f32) {
        if !target.is_finite() || target == self.target {
            return;
        }
        self.target = target;
        match self.kind {
            Smoothing::None => self.current = target,
            Smoothing::Exponential { .. } => {}
            Smoothing::Linear { .. } => self.start(false),
            Smoothing::Multiplicative { .. } => {
                self.start(self.current > 0.0 && target > 0.0);
            }
        }
    }

    fn start(&mut self, ratio: bool) {
        let n = self.ramp_len();
        self.ratio = ratio;
        self.step = if ratio {
            ((self.target / self.current).ln() / n as f32).exp()
        } else {
            (self.target - self.current) / n as f32
        };
        self.remaining = n;
    }

    /// Jump to `value` at once (ignored when not finite)
    #[inline]
    pub fn reset(&mut self, value: f32) {
        if value.is_finite() {
            self.current = value;
            self.target = value;
            self.remaining = 0;
        }
    }

    /// Next value of the ramp
    #[inline]
    pub fn next(&mut self) -> f32 {
        match self.kind {
            Smoothing::None => {}
            Smoothing::Exponential { .. } => {
                if self.current != self.target {
                    let next = self.target + (self.current - self.target) * self.step;
                    // Settle exactly instead of creeping through subnormals
                    let tolerance = 1e-6 * self.target.abs().max(1e-3);
                    self.current = if (next - self.target).abs() <= tolerance { self.target } else { next };
                }
            }
            Smoothing::Linear { .. } | Smoothing::Multiplicative { .. } => {
                if self.remaining > 0 {
                    self.remaining -= 1;
                    self.current = if self.remaining == 0 {
                        self.target
                    } else if self.ratio {
                        self.current * self.step
                    } else {
                        self.current + self.step
                    };
                }
            }
        }
        self.current
    }

    /// Advance by `n` samples at once, returning the value reached
    pub fn skip(&mut self, n: usize) -> f32 {
        match self.kind {
            Smoothing::Linear { .. } | Smoothing::Multiplicative { .. } => {
                if n as u64 >= u64::from(self.remaining) {
                    self.current = self.target;
                    self.remaining = 0;
                } else {
                    // n < remaining ≤ u32::MAX
                    let k = n as u32;
                    self.remaining -= k;
                    self.current = if self.ratio {
                        self.current * self.step.powi(k as i32)
                    } else {
                        self.current + self.step * k as f32
                    };
                }
                self.current
            }
            _ => {
                for _ in 0..n.min(1 << 20) {
                    if self.is_settled() {
                        break;
                    }
                    self.next();
                }
                self.current
            }
        }
    }

    /// Write the next `out.len()` values
    #[inline]
    pub fn fill(&mut self, out: &mut [f32]) {
        if self.is_settled() {
            out.fill(self.current);
        } else {
            for o in out {
                *o = self.next();
            }
        }
    }

    /// The value reached
    #[inline]
    pub fn current(&self) -> f32 {
        self.current
    }

    /// The value moved to
    #[inline]
    pub fn target(&self) -> f32 {
        self.target
    }

    /// True when the value no longer moves
    #[inline]
    pub fn is_settled(&self) -> bool {
        self.current == self.target
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_arrives_on_time() {
        let mut s = Smoother::new(Smoothing::Linear { ms: 1.0 }, 48_000.0, 0.0);
        s.set_target(1.0);
        let values: Vec<f32> = (0..48).map(|_| s.next()).collect();
        assert_eq!(values[47], 1.0);
        assert!(values[46] < 1.0);
        assert!(values.windows(2).all(|w| w[1] > w[0]));
        assert!((values[23] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn exponential_settles_without_overshoot() {
        let mut s = Smoother::new(Smoothing::Exponential { ms: 10.0 }, 48_000.0, 1.0);
        s.set_target(0.0);
        let mut prev = 1.0;
        for _ in 0..480 {
            let v = s.next();
            assert!(v <= prev && v >= 0.0);
            prev = v;
        }
        assert!(prev < 1.1e-3, "{prev}"); // -60 dB after the given time
        for _ in 0..48_000 {
            s.next();
        }
        assert!(s.is_settled());
    }

    #[test]
    fn multiplicative_is_geometric() {
        let mut s = Smoother::new(Smoothing::Multiplicative { ms: 1.0 }, 48_000.0, 100.0);
        s.set_target(10_000.0);
        let half = (0..24).map(|_| s.next()).last().unwrap_or_default();
        assert!((half - 1000.0).abs() < 1.0, "{half}"); // geometric midpoint
        let end = (0..24).map(|_| s.next()).last().unwrap_or_default();
        assert_eq!(end, 10_000.0);
        // Through zero: falls back to a straight line
        s.set_target(-1.0);
        let end = (0..48).map(|_| s.next()).last().unwrap_or_default();
        assert_eq!(end, -1.0);
    }

    #[test]
    fn skip_matches_stepping() {
        for kind in
            [Smoothing::Linear { ms: 2.0 }, Smoothing::Multiplicative { ms: 2.0 }, Smoothing::Exponential { ms: 2.0 }]
        {
            let mut a = Smoother::new(kind, 48_000.0, 50.0);
            let mut b = a.clone();
            a.set_target(400.0);
            b.set_target(400.0);
            for _ in 0..37 {
                a.next();
            }
            let skipped = b.skip(37);
            assert!((a.current() - skipped).abs() < 1e-2, "{kind:?}");
        }
    }

    #[test]
    fn ignores_non_finite() {
        let mut s = Smoother::new(Smoothing::Linear { ms: 1.0 }, 48_000.0, 0.5);
        s.set_target(f32::NAN);
        s.reset(f32::INFINITY);
        assert_eq!(s.next(), 0.5);
    }
}
