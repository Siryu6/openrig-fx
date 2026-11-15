//! Raised-cosine fades, ending exactly on their target.

use orx_dsp::math::cos_turns;

/// A gain moving from one value to another along half a cosine
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ramp {
    from: f32,
    to: f32,
    k: u32,
    n: u32,
}

impl Ramp {
    /// A ramp resting at `value`
    pub const fn at(value: f32) -> Self {
        Self { from: value, to: value, k: 0, n: 0 }
    }

    /// The gain now
    #[inline]
    pub fn value(&self) -> f32 {
        if self.k >= self.n {
            self.to
        } else {
            let t = self.k as f32 / self.n as f32;
            self.from + (self.to - self.from) * (0.5 - 0.5 * cos_turns(0.5 * t))
        }
    }

    /// The gain now, then one sample on
    #[inline]
    pub fn next(&mut self) -> f32 {
        let v = self.value();
        if self.k < self.n {
            self.k += 1;
        }
        v
    }

    /// Move from the gain now to `to` over `samples` (0: at once)
    pub fn go(&mut self, to: f32, samples: usize) {
        let from = self.value();
        *self = Self { from, to, k: 0, n: u32::try_from(samples).unwrap_or(u32::MAX) };
    }

    /// The target, reached or not
    pub fn target(&self) -> f32 {
        self.to
    }

    /// The ramp has reached its target
    pub fn is_settled(&self) -> bool {
        self.k >= self.n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ends_exactly_and_moves_smoothly() {
        let mut r = Ramp::at(1.0);
        assert_eq!(r.next(), 1.0);
        r.go(0.0, 100);
        let v: Vec<f32> = (0..101).map(|_| r.next()).collect();
        assert_eq!(v[0], 1.0);
        assert_eq!(v[100], 0.0);
        assert!(r.is_settled());
        assert!(v.windows(2).all(|w| w[1] <= w[0]));
        assert!((v[50] - 0.5).abs() < 1e-6);
        // Restarting midway starts from the gain reached
        r.go(1.0, 10);
        r.next();
        r.next();
        let mid = r.value();
        r.go(0.0, 10);
        assert_eq!(r.value(), mid);
        r.go(0.25, 0);
        assert_eq!(r.next(), 0.25);
    }
}
