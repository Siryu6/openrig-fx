//! The plugin around the tank: pre-delay, modulation, freeze, the filters
//! on the reverb and the mix.

use crate::info::{HIGH_CUT_OFF, LOW_CUT_OFF, MAX_PREDELAY_MS, PLATE};
use crate::tank::{Decay, MAX_EXCURSION_S, Tank};
use orx_dsp::Smoother;
use orx_dsp::delay::DelayLine;
use orx_dsp::filter::Shape;
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_dsp::lfo::Phasor;
use orx_dsp::math::sin_turns;
use orx_dsp::reader::{Motion, Reader};
use orx_dsp::xfade::Fade;
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Decay and filter coefficients follow their controls on this grid of
/// samples (counted by the plugin, so the output does not depend on the
/// block size)
const CONTROL_PERIOD: u32 = 16;
/// Filter on/off changes crossfade over this time
const SWITCH_MS: f64 = 20.0;
/// Freeze engages and releases over this time
const FREEZE_MS: f64 = 60.0;
/// Rates of the two modulated all-passes, Hz: close but unrelated, so the
/// movement never repeats audibly
const MOD_HZ: [f64; 2] = [0.83, 1.07];
/// What the tank writes below this level counts as silence (−100 dBFS)
const SILENCE: f32 = 1e-5;

#[derive(Debug, Clone, Copy)]
struct Ix {
    predelay: usize,
    decay: usize,
    low_decay: usize,
    high_decay: usize,
    crossover_low: usize,
    crossover_high: usize,
    low_cut: usize,
    high_cut: usize,
    modulation: usize,
    mix: usize,
    freeze: usize,
}

impl Ix {
    fn of(info: &PluginInfo) -> Self {
        let i = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
        Self {
            predelay: i("predelay"),
            decay: i("decay"),
            low_decay: i("low_decay"),
            high_decay: i("high_decay"),
            crossover_low: i("crossover_low"),
            crossover_high: i("crossover_high"),
            low_cut: i("low_cut"),
            high_cut: i("high_cut"),
            modulation: i("modulation"),
            mix: i("mix"),
            freeze: i("freeze"),
        }
    }
}

fn param(info: &PluginInfo, index: usize) -> Param {
    match info.ports.get(index).map(|p| p.kind) {
        Some(PortKind::Param(p)) => p,
        _ => Param::float(0.0, 0.0, 1.0),
    }
}

/// Dry and wet gains for a mix of `m` (0 to 1): the dry signal stays at
/// unity up to the middle, the reverb reaches unity there
#[inline]
fn mix_gains(m: f32) -> (f32, f32) {
    ((2.0 * (1.0 - m)).min(1.0), (2.0 * m).min(1.0))
}

/// Smoothed controls, in the order they are read from the ports
const SMOOTHED: usize = 9;
/// The first control-rate one among them
const CONTROL_RATE: usize = 2;

/// OR Plate
pub struct Plate {
    ix: Ix,
    sample_rate: f64,
    tank: Tank,
    predelay: [DelayLine; 2],
    /// Read heads of the pre-delay lines: a change crossfades
    predelay_heads: Reader,
    low_cut: [Svf; 2],
    high_cut: [Svf; 2],
    lfos: [Phasor; 2],
    /// modulation (samples), mix, then the control-rate ones: decay, low
    /// and high decay, crossovers, low and high cut
    smoothers: [Smoother; SMOOTHED],
    low_cut_on: Fade,
    high_cut_on: Fade,
    freeze: Fade,
    /// Samples to the next coefficient update
    control: u32,
    /// Samples since anything above silence went into or through the tank
    quiet: u64,
    /// Smoothers snap to the parameters on the next block
    fresh: bool,
}

impl Plate {
    fn ports(ix: Ix) -> [usize; SMOOTHED] {
        [
            ix.modulation,
            ix.mix,
            ix.decay,
            ix.low_decay,
            ix.high_decay,
            ix.crossover_low,
            ix.crossover_high,
            ix.low_cut,
            ix.high_cut,
        ]
    }

    /// Targets of the smoothers, in their units
    fn targets(&self, ctx: &Context<'_>) -> [f32; SMOOTHED] {
        let sr = self.sample_rate as f32;
        let mut t = Self::ports(self.ix).map(|i| ctx.params.get(i));
        t[0] *= 0.01 * (MAX_EXCURSION_S as f32) * sr;
        t[1] *= 0.01;
        t
    }

    /// Advance the control-rate smoothers by a period and set the
    /// coefficients
    fn update_controls(&mut self) {
        let sr = self.sample_rate;
        let [decay, low, high, crossover_low, crossover_high, low_cut, high_cut] =
            core::array::from_fn(|k| f64::from(self.smoothers[CONTROL_RATE + k].skip(CONTROL_PERIOD as usize)));
        self.tank.set_decay(Decay { seconds: decay, low, high, crossover_low, crossover_high });
        let q = core::f64::consts::FRAC_1_SQRT_2;
        let lc = SvfCoeffs::new(Shape::Highpass, sr, low_cut, q, 0.0);
        let hc = SvfCoeffs::lowpass(sr, high_cut);
        for (l, h) in self.low_cut.iter_mut().zip(self.high_cut.iter_mut()) {
            l.set(lc);
            h.set(hc);
        }
    }
}

impl Plugin for Plate {
    const INFO: &'static PluginInfo = &PLATE;
    const TRAILS: bool = true;

    fn new(config: &Config) -> Self {
        let info = Self::INFO;
        let ix = Ix::of(info);
        let sr = config.sample_rate;
        let smoothers = Self::ports(ix).map(|i| {
            let p = param(info, i);
            Smoother::new(p.smoothing, sr as f32, p.default)
        });
        let longest = (f64::from(MAX_PREDELAY_MS) * 0.001 * sr).ceil() as usize + 4;
        // Read before the write by a cubic interpolator: two samples are
        // the shortest delay (0 ms is 42 µs at 48 kHz)
        let predelay_heads = Reader::new(sr, 2.0, longest as f64 - 4.0);
        let switch = |samples: f64| Fade::new((samples * 0.001 * sr) as usize, false);
        let low_pass = SvfCoeffs::lowpass(sr, 10_000.0);
        let mut p = Self {
            ix,
            sample_rate: sr,
            tank: Tank::new(sr),
            predelay: [DelayLine::new(longest), DelayLine::new(longest)],
            predelay_heads,
            low_cut: [Svf::new(low_pass); 2],
            high_cut: [Svf::new(low_pass); 2],
            lfos: MOD_HZ.map(|hz| Phasor::new(hz, sr)),
            smoothers,
            low_cut_on: switch(SWITCH_MS),
            high_cut_on: switch(SWITCH_MS),
            freeze: switch(FREEZE_MS),
            control: 0,
            quiet: u64::MAX,
            fresh: true,
        };
        p.reset();
        p
    }

    fn reset(&mut self) {
        self.tank.reset();
        for l in self.predelay.iter_mut() {
            l.reset();
        }
        for f in self.low_cut.iter_mut().chain(self.high_cut.iter_mut()) {
            f.reset();
        }
        for (lfo, phase) in self.lfos.iter_mut().zip([0.0, 0.25]) {
            lfo.set_phase(phase);
        }
        self.control = 0;
        self.quiet = u64::MAX;
        self.fresh = true;
    }

    fn is_ringing(&self) -> bool {
        (self.quiet as f64) < self.predelay_heads.longest() + self.tank.reach() as f64 + 8.0
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let targets = self.targets(ctx);
        let fresh = self.fresh;
        for (s, t) in self.smoothers.iter_mut().zip(targets) {
            if fresh {
                s.reset(t);
            } else {
                s.set_target(t);
            }
        }
        let switches = [
            (&mut self.low_cut_on, ctx.params.get(ix.low_cut) > LOW_CUT_OFF),
            (&mut self.high_cut_on, ctx.params.get(ix.high_cut) < HIGH_CUT_OFF),
            (&mut self.freeze, ctx.params.on(ix.freeze)),
        ];
        for (fade, on) in switches {
            fade.set(on);
            if fresh {
                fade.finish();
            }
        }
        let predelay = 2.0 + f64::from(ctx.params.get(ix.predelay)) * 0.001 * self.sample_rate;
        if fresh {
            self.predelay_heads.snap(predelay);
            self.control = 0;
            self.fresh = false;
        }

        for i in 0..ctx.frames {
            if self.control == 0 {
                self.update_controls();
                self.control = CONTROL_PERIOD;
            }
            self.control -= 1;

            let depth = self.smoothers[0].next();
            let (dry, wet) = mix_gains(self.smoothers[1].next());
            self.predelay_heads.step(predelay, Motion::Jump);
            let low_on = self.low_cut_on.next();
            let high_on = self.high_cut_on.next();
            let hold = self.freeze.next();

            let mut x = [0.0f32; 2];
            let mut into = [0.0f32; 2];
            for c in 0..2 {
                x[c] = ctx.inputs.get(c).and_then(|s| s.get(i)).copied().unwrap_or(0.0);
                let line = &mut self.predelay[c];
                into[c] = self.predelay_heads.read(line, 0.0);
                line.write(x[c] * (1.0 - hold));
            }
            let offsets = [0, 1].map(|k| depth * sin_turns(self.lfos[k].next() as f32));
            let (out, written) = self.tank.process(into, hold, offsets);

            let loud = written.max(x[0].abs()).max(x[1].abs()) > SILENCE;
            self.quiet = if loud { 0 } else { self.quiet.saturating_add(1) };

            for c in 0..2 {
                let mut v = out[c];
                let lc = self.low_cut[c].process(v);
                v += (lc - v) * low_on;
                let hc = self.high_cut[c].process(v);
                v += (hc - v) * high_on;
                if let Some(o) = ctx.outputs.get_mut(c).and_then(|s| s.get_mut(i)) {
                    *o = dry * x[c] + wet * v;
                }
            }
        }
    }
}
