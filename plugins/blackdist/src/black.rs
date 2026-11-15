//! The plugin: smoothed controls driving the circuit.

use crate::circuit::Circuit;
use crate::info::{BLACK_DIST, Voicing};
use orx_dsp::Smoother;
use orx_dsp::math::db_to_gain;
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Output gain matching the circuit to the bypassed level at the middle
/// of Distortion, dB (measured on a guitar at −10 dBFS peaks, see the level
/// test)
pub const MAKEUP_DB: f32 = -22.8;

#[derive(Debug, Clone, Copy)]
struct Ix {
    distortion: usize,
    tone: usize,
    level: usize,
    voicing: usize,
}

fn smoother(index: usize, sample_rate: f64) -> Smoother {
    let p = match BLACK_DIST.ports.get(index).map(|p| p.kind) {
        Some(PortKind::Param(p)) => p,
        _ => Param::float(0.0, 0.0, 1.0),
    };
    Smoother::new(p.smoothing, sample_rate as f32, p.default)
}

/// OR Black Dist
pub struct BlackDist {
    ix: Ix,
    distortion: Smoother,
    tone: Smoother,
    level: Smoother,
    voicing: Voicing,
    circuit: Circuit,
    fresh: bool,
}

impl Plugin for BlackDist {
    const INFO: &'static PluginInfo = &BLACK_DIST;

    fn new(config: &Config) -> Self {
        let i = |s: &str| BLACK_DIST.port_index(s).unwrap_or(usize::MAX);
        let ix = Ix { distortion: i("distortion"), tone: i("tone"), level: i("level"), voicing: i("voicing") };
        let sr = config.sample_rate;
        Self {
            ix,
            distortion: smoother(ix.distortion, sr),
            tone: smoother(ix.tone, sr),
            level: smoother(ix.level, sr),
            voicing: Voicing::Silicon,
            circuit: Circuit::new(sr),
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.fresh = true;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let (distortion, tone, level) =
            (ctx.params.get(ix.distortion), ctx.params.get(ix.tone), ctx.params.get(ix.level));
        let voicing = Voicing::from_value(ctx.params.int(ix.voicing));
        if self.fresh {
            self.fresh = false;
            self.distortion.reset(distortion);
            self.tone.reset(tone);
            self.level.reset(level + voicing.compensation_db());
            self.circuit.reset();
            self.circuit.set_distortion(0.01 * distortion);
            self.circuit.set_tone(0.01 * tone);
        } else {
            self.distortion.set_target(distortion);
            self.tone.set_target(tone);
            self.level.set_target(level + voicing.compensation_db());
        }
        if voicing != self.voicing {
            // The circuit changes at once, its level compensation glides
            // with the level
            self.voicing = voicing;
            self.circuit.set_voicing(voicing);
        }

        let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) else {
            return;
        };
        for (x, y) in input.iter().zip(output.iter_mut()).take(ctx.frames) {
            if !self.distortion.is_settled() {
                self.circuit.set_distortion(0.01 * self.distortion.next());
            }
            if !self.tone.is_settled() {
                self.circuit.set_tone(0.01 * self.tone.next());
            }
            let gain = db_to_gain(MAKEUP_DB + self.level.next());
            *y = self.circuit.process(*x) * gain;
        }
    }
}
