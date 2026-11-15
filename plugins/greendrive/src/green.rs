//! The plugin: smoothed controls driving the circuit.

use crate::circuit::Circuit;
use crate::info::{GREEN_DRIVE, Voicing};
use orx_dsp::Smoother;
use orx_dsp::math::db_to_gain;
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Output gain matching the circuit to the bypassed level at the middle
/// of Drive, dB (measured on a guitar at −10 dBFS peaks, see the level
/// test)
pub const MAKEUP_DB: f32 = -8.1;

#[derive(Debug, Clone, Copy)]
struct Ix {
    drive: usize,
    tone: usize,
    level: usize,
    voicing: usize,
    tight: usize,
}

fn smoother(index: usize, sample_rate: f64) -> Smoother {
    let p = match GREEN_DRIVE.ports.get(index).map(|p| p.kind) {
        Some(PortKind::Param(p)) => p,
        _ => Param::float(0.0, 0.0, 1.0),
    };
    Smoother::new(p.smoothing, sample_rate as f32, p.default)
}

/// OR Green Drive
pub struct GreenDrive {
    ix: Ix,
    drive: Smoother,
    tone: Smoother,
    level: Smoother,
    tight: Smoother,
    voicing: Voicing,
    circuit: Circuit,
    fresh: bool,
}

impl Plugin for GreenDrive {
    const INFO: &'static PluginInfo = &GREEN_DRIVE;

    fn new(config: &Config) -> Self {
        let i = |s: &str| GREEN_DRIVE.port_index(s).unwrap_or(usize::MAX);
        let ix = Ix { drive: i("drive"), tone: i("tone"), level: i("level"), voicing: i("voicing"), tight: i("tight") };
        let sr = config.sample_rate;
        Self {
            ix,
            drive: smoother(ix.drive, sr),
            tone: smoother(ix.tone, sr),
            level: smoother(ix.level, sr),
            tight: smoother(ix.tight, sr),
            voicing: Voicing::Classic,
            circuit: Circuit::new(sr),
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.fresh = true;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let (drive, tone, level) = (ctx.params.get(ix.drive), ctx.params.get(ix.tone), ctx.params.get(ix.level));
        let tight = ctx.params.get(ix.tight);
        let voicing = Voicing::from_value(ctx.params.int(ix.voicing));
        if self.fresh {
            self.fresh = false;
            self.drive.reset(drive);
            self.tone.reset(tone);
            self.level.reset(level);
            self.tight.reset(tight);
            self.circuit.reset();
            self.circuit.set_tight(tight);
            self.circuit.set_drive(0.01 * drive);
            self.circuit.set_tone(0.01 * tone);
        } else {
            self.drive.set_target(drive);
            self.tone.set_target(tone);
            self.level.set_target(level);
            self.tight.set_target(tight);
        }
        if voicing != self.voicing {
            self.voicing = voicing;
            self.circuit.set_voicing(voicing);
        }

        let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) else {
            return;
        };
        for (x, y) in input.iter().zip(output.iter_mut()).take(ctx.frames) {
            if !self.drive.is_settled() {
                self.circuit.set_drive(0.01 * self.drive.next());
            }
            if !self.tone.is_settled() {
                self.circuit.set_tone(0.01 * self.tone.next());
            }
            if !self.tight.is_settled() {
                self.circuit.set_tight(self.tight.next());
            }
            let gain = db_to_gain(MAKEUP_DB + self.level.next());
            *y = self.circuit.process(*x) * gain;
        }
    }
}
