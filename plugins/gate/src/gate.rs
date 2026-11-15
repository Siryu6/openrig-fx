//! The gate: a peak detector, an open/closed state with hysteresis and
//! hold, and a gain that rises linearly in amplitude when opening and
//! falls linearly in decibels when closing.

use crate::info::{FLOOR_DB, GATE};
use orx_dsp::env::EnvelopeFollower;
use orx_dsp::math::{db_to_gain, gain_to_db};
use orx_plugin::{Config, Context, Plugin, PluginInfo};

/// Opening time from fully closed, ms: fast enough to keep the pick
/// attack, slow enough not to click
const ATTACK_MS: f64 = 1.0;
/// Release of the peak detector, ms: bridges the zero crossings of the
/// lowest notes
const DETECTOR_MS: f32 = 10.0;
/// The gate closes this far below the threshold, dB
const HYSTERESIS_DB: f32 = 6.0;

#[derive(Debug, Clone, Copy)]
struct Ix {
    threshold: usize,
    decay: usize,
    hold: usize,
    range: usize,
    reduction: usize,
}

/// OR Gate
pub struct Gate {
    ix: Ix,
    sample_rate: f64,
    detector: EnvelopeFollower,
    open: bool,
    /// Samples left before closing
    hold_left: u32,
    /// Current gain, linear
    gain: f32,
}

impl Plugin for Gate {
    const INFO: &'static PluginInfo = &GATE;

    fn new(config: &Config) -> Self {
        let i = |s: &str| GATE.port_index(s).unwrap_or(usize::MAX);
        let sr = config.sample_rate;
        Self {
            ix: Ix {
                threshold: i("threshold"),
                decay: i("decay"),
                hold: i("hold"),
                range: i("range"),
                reduction: i("reduction"),
            },
            sample_rate: sr,
            detector: EnvelopeFollower::new(0.0, DETECTOR_MS, sr as f32),
            open: false,
            hold_left: 0,
            gain: db_to_gain(FLOOR_DB),
        }
    }

    fn reset(&mut self) {
        self.detector.reset();
        self.open = false;
        self.hold_left = 0;
        // Closed at the deepest range: the first note opens it
        self.gain = db_to_gain(FLOOR_DB);
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let sr = self.sample_rate;
        let threshold = ctx.params.get(ix.threshold);
        let open_at = db_to_gain(threshold);
        let close_at = db_to_gain(threshold - HYSTERESIS_DB);
        let hold = (f64::from(ctx.params.get(ix.hold)) * 0.001 * sr) as u32;
        let range = ctx.params.get(ix.range);
        let floor = db_to_gain(range);
        let decay_samples = (f64::from(ctx.params.get(ix.decay)) * 0.001 * sr).max(1.0);
        // Per-sample factor covering the range (at least 1 dB) in the decay
        let fall = db_to_gain((f64::from(range.min(-1.0)) / decay_samples) as f32);
        let rise = (1.0 / (ATTACK_MS * 0.001 * sr)) as f32;

        if let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) {
            for (x, y) in input.iter().zip(output.iter_mut()).take(ctx.frames) {
                let level = self.detector.process(*x);
                if self.open {
                    if level >= close_at {
                        self.hold_left = hold;
                    } else if self.hold_left > 0 {
                        self.hold_left -= 1;
                    } else {
                        self.open = false;
                    }
                } else if level > open_at {
                    self.open = true;
                    self.hold_left = hold;
                }
                self.gain = if self.open {
                    (self.gain + rise).min(1.0)
                } else if self.gain < floor {
                    // The range was raised while closed: rise to it
                    (self.gain + rise).min(floor)
                } else {
                    (self.gain * fall).max(floor)
                };
                *y = *x * self.gain;
            }
        }
        ctx.meters.set(ix.reduction, (-gain_to_db(self.gain)).clamp(0.0, -FLOOR_DB));
    }
}
