//! The compressor: a feed-forward gain computer in decibels with a
//! quadratic soft knee, and a smooth branching peak detector on the gain
//! reduction (Giannoulis, Massberg and Reiss, "Digital dynamic range
//! compressor design: a tutorial and analysis", JAES 2012).

use crate::info::{COMP, METER_DB};
use orx_dsp::Smoother;
use orx_dsp::math::{db_to_gain, gain_to_db};
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Levels below this count as silence in the detector, dB
const SILENCE_DB: f32 = -120.0;

/// The static curve: output level for input level `x`, dB
pub fn curve(x: f32, threshold: f32, ratio: f32, knee: f32) -> f32 {
    let over = x - threshold;
    let slope = 1.0 / ratio - 1.0;
    if 2.0 * over.abs() < knee {
        let t = over + 0.5 * knee;
        x + slope * t * t / (2.0 * knee)
    } else if over > 0.0 {
        threshold + over / ratio
    } else {
        x
    }
}

#[derive(Debug, Clone, Copy)]
struct Ix {
    threshold: usize,
    ratio: usize,
    knee: usize,
    attack: usize,
    release: usize,
    level: usize,
    mix: usize,
    reduction: usize,
}

/// Pole of a one-pole reaching 1 − 1/e of a step after `ms`
fn pole(ms: f32, sample_rate: f64) -> f32 {
    (-1.0 / (f64::from(ms) * 0.001 * sample_rate).max(1.0)).exp() as f32
}

/// OR Comp
pub struct Comp {
    ix: Ix,
    sample_rate: f64,
    /// Smoothed gain reduction, dB (≥ 0)
    reduction: f32,
    level: Smoother,
    mix: Smoother,
    fresh: bool,
}

impl Plugin for Comp {
    const INFO: &'static PluginInfo = &COMP;

    fn new(config: &Config) -> Self {
        let i = |s: &str| COMP.port_index(s).unwrap_or(usize::MAX);
        let ix = Ix {
            threshold: i("threshold"),
            ratio: i("ratio"),
            knee: i("knee"),
            attack: i("attack"),
            release: i("release"),
            level: i("level"),
            mix: i("mix"),
            reduction: i("reduction"),
        };
        let sr = config.sample_rate;
        let smoother = |index: usize| match COMP.ports.get(index).map(|p| p.kind) {
            Some(PortKind::Param(p)) => Smoother::new(p.smoothing, sr as f32, p.default),
            _ => Smoother::new(Param::float(0.0, 0.0, 1.0).smoothing, sr as f32, 0.0),
        };
        Self { ix, sample_rate: sr, reduction: 0.0, level: smoother(ix.level), mix: smoother(ix.mix), fresh: true }
    }

    fn reset(&mut self) {
        self.reduction = 0.0;
        self.fresh = true;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let p = |i: usize| ctx.params.get(i);
        let (threshold, ratio, knee) = (p(ix.threshold), p(ix.ratio).max(1.0), p(ix.knee));
        let attack = pole(p(ix.attack), self.sample_rate);
        let release = pole(p(ix.release), self.sample_rate);
        let (level, mix) = (p(ix.level), 0.01 * p(ix.mix));
        if self.fresh {
            self.fresh = false;
            self.level.reset(level);
            self.mix.reset(mix);
        } else {
            self.level.set_target(level);
            self.mix.set_target(mix);
        }

        if let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) {
            for (x, y) in input.iter().zip(output.iter_mut()).take(ctx.frames) {
                let db = gain_to_db(x.abs()).max(SILENCE_DB);
                let target = db - curve(db, threshold, ratio, knee);
                let a = if target > self.reduction { attack } else { release };
                self.reduction = target + (self.reduction - target) * a;
                let wet = *x * db_to_gain(self.level.next() - self.reduction);
                let m = self.mix.next();
                *y = *x + (wet - *x) * m;
            }
        }
        ctx.meters.set(ix.reduction, self.reduction.clamp(0.0, METER_DB));
    }
}
