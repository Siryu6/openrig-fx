//! Crossfades.

use crate::math::{cos_turns, sin_turns};

/// Equal-power gains (a, b) for a fade position `t` in [0, 1]: a² + b² = 1
#[inline]
pub fn equal_power(t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0) * 0.25;
    (cos_turns(t), sin_turns(t))
}

/// Linear fade between two states, in a fixed number of samples
#[derive(Debug, Clone, Copy)]
pub struct Fade {
    position: f32,
    target: f32,
    step: f32,
}

impl Fade {
    /// A fade taking `samples` from one end to the other, starting at `on`
    pub fn new(samples: usize, on: bool) -> Self {
        let v = if on { 1.0 } else { 0.0 };
        Self { position: v, target: v, step: 1.0 / samples.max(1) as f32 }
    }

    /// Move towards fully on (true) or fully off
    #[inline]
    pub fn set(&mut self, on: bool) {
        self.target = if on { 1.0 } else { 0.0 };
    }

    /// Jump to the target at once
    pub fn finish(&mut self) {
        self.position = self.target;
    }

    /// Position after one more sample, 0 (off) to 1 (on)
    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.position < self.target {
            self.position = (self.position + self.step).min(self.target);
        } else if self.position > self.target {
            self.position = (self.position - self.step).max(self.target);
        }
        self.position
    }

    #[inline]
    pub fn position(&self) -> f32 {
        self.position
    }

    /// Fully on or off, not moving
    #[inline]
    pub fn is_settled(&self) -> bool {
        self.position == self.target
    }

    #[inline]
    pub fn is_off(&self) -> bool {
        self.position == 0.0 && self.target == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_power_keeps_power() {
        for i in 0..=10 {
            let (a, b) = equal_power(i as f32 / 10.0);
            assert!((a * a + b * b - 1.0).abs() < 1e-6);
        }
        assert_eq!(equal_power(0.0).1, 0.0);
    }

    #[test]
    fn fade_takes_its_time() {
        let mut f = Fade::new(10, false);
        f.set(true);
        let v: Vec<f32> = (0..10).map(|_| f.next()).collect();
        assert!((v[9] - 1.0).abs() < 1e-6);
        assert!(v[8] < 1.0);
        assert!(f.is_settled());
    }
}
