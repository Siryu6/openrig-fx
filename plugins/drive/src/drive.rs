//! The plugin: controls, the active circuit and the crossfade to the next
//! one when the mode changes.

use crate::circuit::{Controls, Voice};
use crate::info::{DRIVE, Mode};
use orx_dsp::Smoother;
use orx_dsp::xfade::Fade;
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Filter coefficients and gain ramps follow the controls on this grid of
/// samples (counted by the plugin: the output does not depend on the
/// block size)
const CONTROL_PERIOD: u32 = 16;
/// Mode changes crossfade over this time
const SWITCH_MS: f64 = 30.0;

#[derive(Debug, Clone, Copy)]
struct Ix {
    mode: usize,
    drive: usize,
    tone: usize,
    bass: usize,
    level: usize,
}

impl Ix {
    fn of(info: &PluginInfo) -> Self {
        let i = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
        Self { mode: i("mode"), drive: i("drive"), tone: i("tone"), bass: i("bass"), level: i("level") }
    }

    /// The smoothed ports, in the order of [`Drive::smoothers`]
    fn smoothed(self) -> [usize; 4] {
        [self.drive, self.tone, self.bass, self.level]
    }
}

fn param(info: &PluginInfo, index: usize) -> Param {
    match info.ports.get(index).map(|p| p.kind) {
        Some(PortKind::Param(p)) => p,
        _ => Param::float(0.0, 0.0, 1.0),
    }
}

/// OR Drive
pub struct Drive {
    ix: Ix,
    /// drive, tone, bass, level
    smoothers: [Smoother; 4],
    controls: Controls,
    /// The active circuit and, while a mode change crossfades, the
    /// previous one
    voices: [Voice; 2],
    current: usize,
    /// Share of the active circuit; settled means the active one alone
    fade: Fade,
    control: u32,
    fresh: bool,
}

impl Drive {
    /// Advance the smoothers by a control period
    fn advance(&mut self) -> Controls {
        let [drive, tone, bass_db, level_db] =
            core::array::from_fn(|k| self.smoothers[k].skip(CONTROL_PERIOD as usize));
        Controls { drive: 0.01 * drive, tone: 0.01 * tone, bass_db, level_db }
    }
}

impl Plugin for Drive {
    const INFO: &'static PluginInfo = &DRIVE;

    fn new(config: &Config) -> Self {
        let info = Self::INFO;
        let ix = Ix::of(info);
        let sr = config.sample_rate;
        let smoothers = ix.smoothed().map(|i| {
            let p = param(info, i);
            Smoother::new(p.smoothing, sr as f32, p.default)
        });
        Self {
            ix,
            smoothers,
            controls: Controls::default(),
            voices: [Voice::new(sr), Voice::new(sr)],
            current: 0,
            fade: Fade::new((SWITCH_MS * 0.001 * sr) as usize, false),
            control: 0,
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.fresh = true;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let fresh = self.fresh;
        for (s, i) in self.smoothers.iter_mut().zip(ix.smoothed()) {
            let v = ctx.params.get(i);
            if fresh {
                s.reset(v);
            } else {
                s.set_target(v);
            }
        }
        let mode = Mode::from_value(ctx.params.int(ix.mode));
        if fresh {
            self.fresh = false;
            self.controls = self.advance();
            self.voices[self.current].start(mode, &self.controls);
            self.fade.set(false);
            self.fade.finish();
            self.control = CONTROL_PERIOD;
        }

        let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) else {
            return;
        };
        for (x, y) in input.iter().zip(output.iter_mut()).take(ctx.frames) {
            let switching = !self.fade.is_settled();
            if self.control == 0 {
                self.controls = self.advance();
                self.voices[self.current].configure(&self.controls, CONTROL_PERIOD);
                if switching {
                    self.voices[1 - self.current].configure(&self.controls, CONTROL_PERIOD);
                }
                self.control = CONTROL_PERIOD;
            }
            self.control -= 1;
            // A new mode waits for the previous crossfade to end
            if !switching && mode != self.voices[self.current].mode() {
                self.current = 1 - self.current;
                self.voices[self.current].start(mode, &self.controls);
                self.fade.set(false);
                self.fade.finish();
                self.fade.set(true);
            }
            *y = if self.fade.is_settled() {
                self.voices[self.current].process(*x)
            } else {
                let w = self.fade.next();
                let new = self.voices[self.current].process(*x);
                let old = self.voices[1 - self.current].process(*x);
                old + (new - old) * w
            };
        }
    }
}
