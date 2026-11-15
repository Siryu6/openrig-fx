//! Deterministic noise (no OS randomness: renders are reproducible).

/// xorshift32 generator
#[derive(Debug, Clone, Copy)]
pub struct Rng(u32);

impl Rng {
    /// A generator from a seed (0 is replaced: xorshift needs a non-zero
    /// state)
    pub const fn new(seed: u32) -> Self {
        Self(if seed == 0 { 0x9E37_79B9 } else { seed })
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1)
    #[inline]
    pub fn uniform(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Uniform white noise in [-1, 1)
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        self.uniform() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_centred_and_bounded() {
        let mut r = Rng::new(1);
        let mut sum = 0.0f64;
        for _ in 0..100_000 {
            let v = r.bipolar();
            assert!((-1.0..1.0).contains(&v));
            sum += v as f64;
        }
        assert!((sum / 100_000.0).abs() < 0.01);
    }
}
