//! The plugin: the shift and the dive as one gliding pitch, the shifter,
//! the mix.

use crate::info::{DROP, Mode, SHIFT_MAX, SHIFT_MIN};
use crate::shifter::{self, Shifter};
use orx_dsp::Smoother;
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Glide of the Shift knob, seconds per octave (fast, but no click and no
/// zipper while turning it)
pub const SHIFT_GLIDE_S: f64 = 0.05;

#[derive(Debug, Clone, Copy)]
struct Ix {
    shift: usize,
    mix: usize,
    mode: usize,
    dive: usize,
    dive_to: usize,
    dive_time: usize,
}

fn mode_of(mode: Mode) -> shifter::Mode {
    match mode {
        Mode::Tight => shifter::TIGHT,
        Mode::Quality => shifter::QUALITY,
    }
}

/// OR Drop
pub struct DropPedal {
    ix: Ix,
    rate: f64,
    mix: Smoother,
    mode: Mode,
    shifter: Shifter,
    /// Shift now and the one it glides to, semitones
    semis: f64,
    target: f64,
    /// Glide per sample, semitones
    step: f64,
    ratio: f64,
    dive: bool,
    fresh: bool,
}

impl DropPedal {
    fn glide_to(&mut self, target: f64, seconds: f64) {
        if target != self.target {
            self.target = target;
            let samples = (seconds * self.rate).max(1.0);
            self.step = ((target - self.semis).abs() / samples).max(1e-9);
        }
    }

    fn jump_to(&mut self, target: f64) {
        self.semis = target;
        self.target = target;
        self.ratio = (target / 12.0).exp2();
    }
}

impl Plugin for DropPedal {
    const INFO: &'static PluginInfo = &DROP;

    fn new(config: &Config) -> Self {
        let i = |s: &str| DROP.port_index(s).unwrap_or(usize::MAX);
        let ix = Ix {
            shift: i("shift"),
            mix: i("mix"),
            mode: i("mode"),
            dive: i("dive"),
            dive_to: i("dive_to"),
            dive_time: i("dive_time"),
        };
        let mix = match DROP.ports.get(ix.mix).map(|p| p.kind) {
            Some(PortKind::Param(p)) => p,
            _ => Param::float(100.0, 0.0, 100.0),
        };
        let rate = config.sample_rate;
        Self {
            ix,
            rate,
            mix: Smoother::new(mix.smoothing, rate as f32, mix.default),
            mode: Mode::Tight,
            shifter: Shifter::new(rate, shifter::TIGHT),
            semis: 0.0,
            target: 0.0,
            step: 0.0,
            ratio: 1.0,
            dive: false,
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.fresh = true;
    }

    fn latency(&self) -> u32 {
        self.shifter.latency() as u32
    }

    fn max_latency(config: &Config) -> u32 {
        Shifter::max_latency(config.sample_rate) as u32
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let p = &ctx.params;
        let (shift, dive_to) = (f64::from(p.int(ix.shift)), f64::from(p.int(ix.dive_to)));
        let (dive, dive_time) = (p.on(ix.dive), 0.001 * f64::from(p.get(ix.dive_time)));
        let mode = Mode::from_value(p.int(ix.mode));
        let mix = p.get(ix.mix);
        // Dive To counts from the shift: an octave down stays an octave
        // below the tuning played, whatever the shift
        let target = if dive { (shift + dive_to).clamp(f64::from(SHIFT_MIN), f64::from(SHIFT_MAX)) } else { shift };
        if self.fresh {
            self.fresh = false;
            self.mix.reset(mix);
            self.mode = mode;
            self.shifter.set_mode_now(mode_of(mode));
            self.dive = dive;
            self.jump_to(target);
        } else {
            self.mix.set_target(mix);
            if mode != self.mode {
                self.mode = mode;
                self.shifter.set_mode(mode_of(mode));
            }
            // The dive glides over its time, both ways; the knob glides at
            // a fixed rate per octave
            let seconds =
                if dive != self.dive || dive { dive_time } else { SHIFT_GLIDE_S * (target - self.semis).abs() / 12.0 };
            self.dive = dive;
            self.glide_to(target, seconds);
        }

        let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) else {
            return;
        };
        for (x, y) in input.iter().zip(output.iter_mut()).take(ctx.frames) {
            if self.semis != self.target {
                let d = self.target - self.semis;
                self.semis = if d.abs() <= self.step { self.target } else { self.semis + self.step.copysign(d) };
                self.ratio = (self.semis / 12.0).exp2();
            }
            let x = if x.is_finite() { *x } else { 0.0 };
            let (wet, dry) = self.shifter.process(x, self.ratio);
            let m = 0.01 * self.mix.next();
            *y = dry + m * (wet - dry);
        }
    }
}
