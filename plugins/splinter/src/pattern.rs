//! The chop: a gate opening on the steps of a one-bar pattern.

use orx_dsp::noise::Rng;

/// A pattern over a bar of four quarter notes: `steps` equal steps, step
/// `i` open when bit `i` of `open` is set
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pattern {
    pub steps: u32,
    pub open: u32,
}

/// Steps written left to right, `x` open and `.` closed
const fn pattern(s: &str) -> Pattern {
    let b = s.as_bytes();
    let mut open = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'x' {
            open |= 1 << i;
        }
        i += 1;
    }
    Pattern { steps: b.len() as u32, open }
}

/// The patterns of the Chop control, from its value 1
pub const PATTERNS: [Pattern; 6] = [
    // 1/8
    pattern("xxxxxxxx"),
    // 1/16
    pattern("xxxxxxxxxxxxxxxx"),
    // Triplets of 1/8
    pattern("xxxxxxxxxxxx"),
    // Gallop: an eighth and two sixteenths
    pattern("x.xxx.xxx.xxx.xx"),
    // Offbeat eighths
    pattern(".x.x.x.x"),
    // Breakdown: groups of 3, 3, 2, 3, 3, 2 sixteenths
    pattern("x..x..x.x..x..x."),
];

/// Gate edges, seconds: short enough for a tight chop, long enough not
/// to click
const SLEW_S: f64 = 0.002;
/// Share of the steps chaos flips at most
const CHAOS_FLIPS: f32 = 0.35;

/// The gate of the chop
#[derive(Debug, Clone)]
pub struct Chop {
    slew: f32,
    gain: f32,
    /// Step of the bar now, and whether chaos flipped it
    step: usize,
    flipped: bool,
    rng: Rng,
}

impl Chop {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            slew: (1.0 / (SLEW_S * sample_rate)) as f32,
            gain: 1.0,
            step: usize::MAX,
            flipped: false,
            rng: Rng::new(1),
        }
    }

    pub fn reset(&mut self) {
        self.gain = 1.0;
        self.step = usize::MAX;
        self.flipped = false;
        self.rng = Rng::new(1);
    }

    /// Gain now for `pattern` (None: open) at `pos` quarter notes in the
    /// bar, open for `duty` (0 to 1) of each step, `chaos` (0 to 1) the
    /// chance of a step flipping
    #[inline]
    pub fn next(&mut self, pattern: Option<Pattern>, pos: f64, duty: f32, chaos: f32) -> f32 {
        let target = match pattern {
            None => 1.0,
            Some(p) => {
                let s = pos * f64::from(p.steps) * 0.25;
                let i = s as usize; // pos ≥ 0: truncation is floor
                if i != self.step {
                    self.step = i;
                    self.flipped = chaos > 0.0 && self.rng.uniform() < chaos * CHAOS_FLIPS;
                }
                let bit = i % p.steps as usize;
                let on = (p.open >> bit) & 1 == 1;
                let open = (on != self.flipped) && (s - i as f64) < f64::from(duty);
                if open { 1.0 } else { 0.0 }
            }
        };
        let d = target - self.gain;
        self.gain = if d.abs() <= self.slew { target } else { self.gain + self.slew.copysign(d) };
        self.gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_read_left_to_right() {
        assert_eq!(PATTERNS[3].steps, 16);
        assert_eq!(PATTERNS[3].open & 0b1111, 0b1101);
        assert_eq!(PATTERNS[4].open, 0b1010_1010);
        assert_eq!(PATTERNS[5].open.count_ones(), 6);
        assert_eq!(PATTERNS[2].steps, 12);
    }

    #[test]
    fn gate_follows_steps_and_duty() {
        let sr = 48_000.0;
        let mut c = Chop::new(sr);
        // 1/8 at 50 %: open on the first half of each eighth
        let q = 1.0 / 24_000.0; // quarters per sample at 120 BPM
        let mut gains = Vec::new();
        for n in 0..96_000 {
            gains.push(c.next(Some(PATTERNS[0]), n as f64 * q, 0.5, 0.0));
        }
        // An eighth is 12000 samples
        assert_eq!(gains[3_000], 1.0);
        assert_eq!(gains[9_000], 0.0);
        assert_eq!(gains[15_000], 1.0);
        // Edges are slewed: no step larger than the slope
        let slope = (1.0 / (SLEW_S * sr)) as f32;
        assert!(gains.windows(2).all(|w| (w[1] - w[0]).abs() <= slope + 1e-6));
        // No pattern: fully open, after the slew
        let open: Vec<f32> = (0..200).map(|_| c.next(None, 0.3, 0.5, 1.0)).collect();
        assert_eq!(open[199], 1.0);
    }
}
