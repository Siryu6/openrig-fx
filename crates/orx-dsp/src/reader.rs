//! Read heads of a delay line whose delay changes while it plays: where
//! the output comes from, and how that place moves when the time changes.
//!
//! Two heads read the line. A *jump* (digital) puts the idle head at the
//! new time and crossfades to it with a raised cosine: no pitch change,
//! and no click whatever the distance. A *glide* (tape, analog) moves the
//! active head itself like a motor with inertia or a BBD clock settling:
//! a critically damped second-order response to the new time, its speed
//! limited, so the echoes bend in pitch on the way as on the real
//! machines. A change of motion never moves a head at once: a jump
//! starts from wherever the glide had brought the head.

use crate::delay::DelayLine;
use crate::math::sin_turns;

/// Crossfade of a jump, ms
const JUMP_MS: f64 = 40.0;
/// Below this distance, samples, a head is at its target
const SETTLED: f64 = 1e-3;

/// How the time follows its target
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Crossfade to a second head at the new time
    Jump,
    /// Move the head: natural frequency, rad/s, and the largest rate of
    /// change of the delay (a rate of 0.5 plays the echo half a step
    /// faster or slower, up to ±50 % in pitch)
    Glide { omega: f64, max_rate: f64 },
}

#[derive(Debug, Clone, Copy, Default)]
struct Head {
    /// Delay, samples
    delay: f64,
    /// Change of delay per sample
    velocity: f64,
}

/// The two heads of a delay line (one reader can serve several lines of
/// the same length, one per channel). Reads happen before the line is
/// written: a delay of `d` samples reads `read(d − 1)`.
#[derive(Debug, Clone)]
pub struct Reader {
    sample_rate: f64,
    heads: [Head; 2],
    active: usize,
    /// Progress of the crossfade to the active head, 0 to 1 (1: settled)
    fade: f64,
    fade_step: f64,
    /// Bounds of a head's delay, samples
    min: f64,
    max: f64,
}

impl Reader {
    pub fn new(sample_rate: f64, min: f64, max: f64) -> Self {
        Self {
            sample_rate,
            heads: [Head { delay: min, velocity: 0.0 }; 2],
            active: 0,
            fade: 1.0,
            fade_step: 1.0 / (JUMP_MS * 0.001 * sample_rate).max(1.0),
            min,
            max,
        }
    }

    /// Both heads at `delay`, still
    pub fn snap(&mut self, delay: f64) {
        let delay = delay.clamp(self.min, self.max);
        self.heads = [Head { delay, velocity: 0.0 }; 2];
        self.fade = 1.0;
    }

    /// Delay of the active head, samples
    pub fn current(&self) -> f64 {
        self.heads[self.active & 1].delay
    }

    /// Longest delay any head reads now, samples
    pub fn longest(&self) -> f64 {
        if self.fade < 1.0 { self.heads[0].delay.max(self.heads[1].delay) } else { self.current() }
    }

    /// Move one sample towards `target` (samples)
    #[inline]
    pub fn step(&mut self, target: f64, motion: Motion) {
        let target = target.clamp(self.min, self.max);
        let a = self.active & 1;
        match motion {
            Motion::Jump => {
                self.heads[a].velocity = 0.0;
                if self.fade >= 1.0 && (self.heads[a].delay - target).abs() > SETTLED {
                    // Changes during a crossfade wait for its end
                    let b = a ^ 1;
                    self.heads[b] = Head { delay: target, velocity: 0.0 };
                    self.active = b;
                    self.fade = 0.0;
                }
            }
            Motion::Glide { omega, max_rate } => {
                let w = omega / self.sample_rate;
                let h = &mut self.heads[a];
                let error = target - h.delay;
                if error.abs() <= SETTLED && h.velocity.abs() <= SETTLED * w {
                    *h = Head { delay: target, velocity: 0.0 };
                } else {
                    // Critically damped: x'' = w²·error − 2w·x'
                    let accel = w * w * error - 2.0 * w * h.velocity;
                    h.velocity = (h.velocity + accel).clamp(-max_rate, max_rate);
                    h.delay = (h.delay + h.velocity).clamp(self.min, self.max);
                }
            }
        }
        if self.fade < 1.0 {
            self.fade = (self.fade + self.fade_step).min(1.0);
        }
    }

    /// The output of `line` (read before this sample is written), the
    /// heads moved by `offset` samples
    #[inline]
    pub fn read(&self, line: &DelayLine, offset: f32) -> f32 {
        let a = self.active & 1;
        // The sample about to be written counts as delay 0: one less
        let at = |h: &Head| line.read_hermite((h.delay - 1.0) as f32 + offset);
        let now = at(&self.heads[a]);
        if self.fade >= 1.0 {
            return now;
        }
        let s = sin_turns(0.25 * self.fade as f32);
        let w = s * s;
        let before = at(&self.heads[a ^ 1]);
        before + (now - before) * w
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    #[test]
    fn a_jump_crossfades_without_moving_a_head() {
        let mut r = Reader::new(SR, 10.0, 10_000.0);
        r.snap(1000.0);
        r.step(2000.0, Motion::Jump);
        assert_eq!(r.current(), 2000.0);
        assert_eq!(r.longest(), 2000.0);
        // A change during the fade waits
        for _ in 0..100 {
            r.step(3000.0, Motion::Jump);
        }
        assert_eq!(r.current(), 2000.0);
        for _ in 0..2000 {
            r.step(3000.0, Motion::Jump);
        }
        assert_eq!(r.current(), 3000.0);
    }

    #[test]
    fn a_glide_settles_without_overshoot() {
        let mut r = Reader::new(SR, 10.0, 100_000.0);
        r.snap(10_000.0);
        let motion = Motion::Glide { omega: 25.0, max_rate: 0.5 };
        let mut last = r.current();
        let mut largest_rate = 0.0f64;
        // 0.83 s at the largest rate, then the approach
        for _ in 0..96_000 {
            r.step(20_000.0, motion);
            let d = r.current();
            assert!(d >= last - 1e-9 && d <= 20_000.0 + 1e-9, "{d}");
            largest_rate = largest_rate.max(d - last);
            last = d;
        }
        assert_eq!(r.current(), 20_000.0);
        assert!(largest_rate <= 0.5 + 1e-12);
    }

    #[test]
    fn crossfade_is_smooth() {
        // Two constant signals at the two heads: the output moves from one
        // to the other with no step above the raised cosine's slope
        let mut line = DelayLine::new(5000);
        for i in 0..5000 {
            line.write(if i < 2500 { 1.0 } else { 0.0 });
        }
        let mut r = Reader::new(SR, 10.0, 4000.0);
        r.snap(100.0);
        let mut out = vec![r.read(&line, 0.0)];
        for _ in 0..3000 {
            r.step(3000.0, Motion::Jump);
            out.push(r.read(&line, 0.0));
        }
        assert_eq!(out[0], 0.0);
        assert!((out[2999] - 1.0).abs() < 1e-6);
        let step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        let fade = JUMP_MS * 0.001 * SR;
        assert!(f64::from(step) < 1.6 / fade, "{step}");
    }
}
