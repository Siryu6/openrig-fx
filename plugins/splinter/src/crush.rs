//! The crush: sample-rate reduction (sample and hold) and bit reduction.

/// Sample-and-hold rate reducer and quantiser
#[derive(Debug, Clone)]
pub struct Crush {
    sample_rate: f64,
    phase: f64,
    held: f32,
}

impl Crush {
    pub fn new(sample_rate: f64) -> Self {
        Self { sample_rate, phase: 0.0, held: 0.0 }
    }

    pub fn reset(&mut self) {
        self.phase = 0.0;
        self.held = 0.0;
    }

    /// Whether `bits` and `rate` leave the signal untouched
    pub fn is_off(&self, bits: f32, rate: f32) -> bool {
        bits >= crate::info::BITS_OFF && f64::from(rate) >= self.sample_rate.min(f64::from(crate::info::RATE_OFF))
    }

    /// `x` held at `rate` Hz and quantised to `bits` (a step of
    /// 2^(1 − bits) on ±1: 1 bit is −1, 0 and 1)
    #[inline]
    pub fn next(&mut self, x: f32, bits: f32, rate: f32) -> f32 {
        let inc = f64::from(rate) / self.sample_rate;
        let y = if inc >= 1.0 {
            self.phase = 0.0;
            x
        } else {
            self.phase += inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.held = x;
            }
            self.held
        };
        if bits >= crate::info::BITS_OFF {
            y
        } else {
            let step = (1.0 - bits).exp2();
            (y / step).round() * step
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_is_transparent() {
        let mut c = Crush::new(48_000.0);
        assert!(c.is_off(16.0, 48_000.0));
        assert!(!c.is_off(15.5, 48_000.0));
        for i in 0..100 {
            let x = (i as f32 * 0.37).sin() * 0.9;
            assert_eq!(c.next(x, 16.0, 48_000.0), x);
        }
        // At 44.1 kHz the top of the range is already above the rate
        assert!(Crush::new(44_100.0).is_off(16.0, 44_100.0));
    }

    #[test]
    fn holds_and_quantises() {
        let mut c = Crush::new(48_000.0);
        // 12 kHz: each value held for 4 samples
        let y: Vec<f32> = (0..16).map(|i| c.next(i as f32 * 0.01, 16.0, 12_000.0)).collect();
        let changes: Vec<usize> = (1..16).filter(|&i| y[i] != y[i - 1]).collect();
        assert_eq!(changes, [3, 7, 11, 15], "{y:?}");
        // 3 bits: steps of 1/4
        assert_eq!(c.next(0.3, 3.0, 48_000.0), 0.25);
        assert_eq!(c.next(-0.9, 3.0, 48_000.0), -1.0);
        assert_eq!(c.next(0.3, 1.0, 48_000.0), 0.0);
        assert_eq!(c.next(0.6, 1.0, 48_000.0), 1.0);
    }
}
