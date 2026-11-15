//! Envelope follower.

/// Peak envelope with separate attack and release times
#[derive(Debug, Clone, Copy)]
pub struct EnvelopeFollower {
    attack: f32,
    release: f32,
    value: f32,
}

/// Pole of a one-pole reaching 1 − 1/e of a step after `ms`
fn pole(ms: f32, sample_rate: f32) -> f32 {
    let samples = (ms * 0.001 * sample_rate).max(1e-3);
    (-1.0 / samples).exp()
}

impl EnvelopeFollower {
    pub fn new(attack_ms: f32, release_ms: f32, sample_rate: f32) -> Self {
        Self { attack: pole(attack_ms, sample_rate), release: pole(release_ms, sample_rate), value: 0.0 }
    }

    pub fn set_times(&mut self, attack_ms: f32, release_ms: f32, sample_rate: f32) {
        self.attack = pole(attack_ms, sample_rate);
        self.release = pole(release_ms, sample_rate);
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
    }

    /// Follow `x` (rectified here); returns the envelope
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = x.abs();
        let pole = if x > self.value { self.attack } else { self.release };
        self.value = crate::math::flush_tiny(x + (self.value - x) * pole);
        self.value
    }

    pub fn value(&self) -> f32 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attacks_fast_releases_slow() {
        let mut e = EnvelopeFollower::new(1.0, 100.0, 48_000.0);
        for _ in 0..480 {
            e.process(1.0);
        }
        assert!(e.value() > 0.99);
        for _ in 0..4800 {
            e.process(0.0);
        }
        // 100 ms of silence: down to 1/e
        assert!((e.value() - (-1.0f32).exp()).abs() < 0.01, "{}", e.value());
    }
}
