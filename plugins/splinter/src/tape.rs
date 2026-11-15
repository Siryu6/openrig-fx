//! The tape stop: the input read from the ring at a speed falling
//! linearly from 1 to 0, and back up to 1 for a tape start.
//!
//! The read head starts on the current sample (no delay), so the stop
//! begins without any change in the sound; the delay then grows by
//! `1 − v` each sample. The level follows `√v`, so the last, slowest
//! moments fade to silence instead of holding a DC value. A tape start
//! runs the speed back up and ends with the head behind the input by
//! half the start time: a short crossfade joins the input then.

use crate::ramp::Ramp;
use orx_dsp::delay::DelayLine;

/// Crossfade back to the input, seconds
const LEAVE_S: f64 = 0.010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Off,
    Stopping,
    Stopped,
    Starting,
    /// Fading back to the input
    Leaving,
}

/// The tape stop
#[derive(Debug, Clone)]
pub struct Tape {
    state: State,
    /// Speed, 0 to 1
    v: f64,
    /// Speed change per sample (down while stopping, up while starting)
    down: f64,
    up: f64,
    /// Read delay, samples
    delay: f64,
    env: Ramp,
    live: Ramp,
    leave: usize,
}

impl Tape {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            state: State::Off,
            v: 1.0,
            down: 0.0,
            up: 0.0,
            delay: 0.0,
            env: Ramp::at(0.0),
            live: Ramp::at(1.0),
            leave: (LEAVE_S * sample_rate) as usize,
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Off;
        self.v = 1.0;
        self.delay = 0.0;
        self.env = Ramp::at(0.0);
        self.live = Ramp::at(1.0);
    }

    /// Longest delay the head can reach for a stop of `frames` (a release
    /// in the middle of the stop, then a start of half its time)
    pub fn max_delay(frames: f64) -> f64 {
        0.75 * frames
    }

    /// Nothing sounds but the input
    pub fn is_idle(&self) -> bool {
        self.state == State::Off
    }

    /// The tape is held (stopping or stopped)
    pub fn is_holding(&self) -> bool {
        matches!(self.state, State::Stopping | State::Stopped)
    }

    /// Stop the tape over `frames` samples, from the speed it has now
    pub fn stop(&mut self, frames: f64) {
        if matches!(self.state, State::Off | State::Leaving) {
            // From the input: the head on the current sample. A stop
            // during the fade back starts over from the input as well
            self.v = 1.0;
            self.delay = 0.0;
            self.env = Ramp::at(1.0);
            self.live = Ramp::at(0.0);
        }
        self.down = 1.0 / frames.max(1.0);
        self.state = State::Stopping;
    }

    /// HOLD released: spin back up over `frames` (`start`), or fade back
    /// to the input
    pub fn release(&mut self, start: bool, frames: f64) {
        match self.state {
            State::Stopping | State::Stopped if start => {
                if self.state == State::Stopped {
                    // Nothing heard: start from the input as it is now
                    self.delay = 0.0;
                }
                self.up = 1.0 / frames.max(1.0);
                self.state = State::Starting;
            }
            State::Stopping | State::Stopped => self.leave(),
            _ => {}
        }
    }

    fn leave(&mut self) {
        self.state = State::Leaving;
        self.env.go(0.0, self.leave);
        self.live.go(1.0, self.leave);
    }

    /// Gain of the input now (call once per sample, before
    /// [`Tape::process`])
    #[inline]
    pub fn live(&mut self) -> f32 {
        self.live.next()
    }

    /// The tape's output (the ring holds the current sample)
    #[inline]
    pub fn process(&mut self, ring: &DelayLine) -> f32 {
        if self.state == State::Off {
            return 0.0;
        }
        let e = self.env.next();
        let y = if self.state == State::Stopped {
            0.0
        } else {
            let d = self.delay.min((ring.max_delay() - 2) as f64) as f32;
            let x = if d < 1.0 { ring.read_linear(d) } else { ring.read_hermite(d) };
            x * (self.v.sqrt() as f32) * e
        };
        self.delay += 1.0 - self.v;
        match self.state {
            State::Stopping => {
                self.v -= self.down;
                if self.v <= 0.0 {
                    self.v = 0.0;
                    self.state = State::Stopped;
                }
            }
            State::Starting => {
                self.v += self.up;
                if self.v >= 1.0 {
                    self.v = 1.0;
                    self.leave();
                }
            }
            State::Leaving if self.env.is_settled() => self.state = State::Off,
            _ => {}
        }
        y
    }

    /// Speed now (1 when off)
    pub fn speed(&self) -> f32 {
        if self.state == State::Off { 1.0 } else { self.v as f32 }
    }
}
