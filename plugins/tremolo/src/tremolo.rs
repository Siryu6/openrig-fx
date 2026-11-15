//! The tremolo processor.

use crate::info::{MONO, Mode, STEREO};
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_dsp::lfo::{Phasor, shape_steepness, sine_to_square};
use orx_dsp::math::{db_to_gain, sin_turns};
use orx_dsp::xfade::Fade;
use orx_dsp::{Smoother, Smoothing};
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Points of the modulation cycle sent to views
const CYCLE: usize = 64;
/// Opto lamp: time constants of rising and falling light
const OPTO_ATTACK_MS: f64 = 4.0;
const OPTO_RELEASE_MS: f64 = 40.0;
/// Mode changes crossfade over this time
const MODE_FADE_MS: f64 = 10.0;
/// Fastest LFO when synced to a fast tempo
const MAX_SYNC_RATE: f64 = 40.0;

/// Port indices, found by symbol (they differ between the variants)
#[derive(Debug, Clone, Copy)]
struct Ix {
    rate: usize,
    sync: usize,
    depth: usize,
    shape: usize,
    mode: usize,
    crossover: usize,
    output: usize,
    spread: Option<usize>,
    phase: usize,
    gain: usize,
    cycle: usize,
}

impl Ix {
    fn of(info: &PluginInfo) -> Self {
        let i = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
        Self {
            rate: i("rate"),
            sync: i("sync"),
            depth: i("depth"),
            shape: i("shape"),
            mode: i("mode"),
            crossover: i("crossover"),
            output: i("output"),
            spread: info.port_index("spread"),
            phase: i("lfo_phase"),
            gain: i("lfo_gain"),
            cycle: i("cycle"),
        }
    }
}

fn param(info: &PluginInfo, index: usize) -> Param {
    match info.ports.get(index).map(|p| p.kind) {
        Some(PortKind::Param(p)) => p,
        _ => Param::float(0.0, 0.0, 1.0),
    }
}

/// One channel's audio state
#[derive(Debug, Clone, Copy)]
struct Channel {
    /// Crossover: the first filter gives the Butterworth low and high
    /// bands, the second and third square them (Linkwitz-Riley)
    split: Svf,
    low: Svf,
    high: Svf,
    /// Opto lamp light, 0 to 1
    light: f32,
}

/// OR Tremolo with `C` channels (1 or 2)
pub struct Tremolo<const C: usize> {
    ix: Ix,
    sample_rate: f64,
    lfo: Phasor,
    rate: Smoother,
    depth: Smoother,
    shape: Smoother,
    crossover: Smoother,
    output: Smoother,
    spread: Smoother,
    channels: [Channel; C],
    mode: Mode,
    /// Mode being faded out, while `fade` runs
    old_mode: Mode,
    fade: Fade,
    opto_attack: f32,
    opto_release: f32,
    /// Gain applied at each phase bin, per channel (for views)
    cycle: [[f32; CYCLE]; C],
    /// Smoothers snap to the parameters on the next block
    fresh: bool,
}

fn crossover_coeffs(sample_rate: f64, f: f32) -> (SvfCoeffs, SvfCoeffs, SvfCoeffs) {
    use orx_dsp::filter::Shape;
    let q = core::f64::consts::FRAC_1_SQRT_2;
    let f = f64::from(f);
    (
        // Its band outputs are used; its mix is irrelevant
        SvfCoeffs::new(Shape::Lowpass, sample_rate, f, q, 0.0),
        SvfCoeffs::new(Shape::Lowpass, sample_rate, f, q, 0.0),
        SvfCoeffs::new(Shape::Highpass, sample_rate, f, q, 0.0),
    )
}

/// One-pole coefficient for a time constant
fn pole(ms: f64, sample_rate: f64) -> f32 {
    (1.0 - (-1000.0 / (ms * sample_rate)).exp()) as f32
}

impl<const C: usize> Tremolo<C> {
    fn set_crossover(&mut self, f: f32) {
        let (a, b, c) = crossover_coeffs(self.sample_rate, f);
        for ch in self.channels.iter_mut() {
            ch.split.set(a);
            ch.low.set(b);
            ch.high.set(c);
        }
    }

    /// LFO rate now: the tempo division when synced and the tempo known
    fn target_rate(&self, ctx: &Context<'_>) -> f32 {
        match orx_plugin::tempo::seconds(ctx.params.int(self.ix.sync), ctx.position.bpm) {
            Some(period) => (1.0 / period).clamp(0.01, MAX_SYNC_RATE) as f32,
            None => ctx.params.get(self.ix.rate),
        }
    }

    /// The static gain curve of the current settings (before any opto lag)
    fn static_cycle(&mut self) {
        let k = shape_steepness(self.shape.current() * 0.01);
        let d = self.depth.current() * 0.01;
        let spread = self.spread.current() / 360.0;
        for (c, row) in self.cycle.iter_mut().enumerate() {
            for (i, g) in row.iter_mut().enumerate() {
                let phase = i as f32 / CYCLE as f32 + if c == 1 { spread } else { 0.0 };
                let w = sine_to_square(phase, k);
                *g = match self.mode {
                    Mode::Pan => pan_gain(c, d * w),
                    Mode::Opto => {
                        let m = 0.5 * (1.0 - w);
                        1.0 - d * m * m.sqrt()
                    }
                    _ => 1.0 - d * 0.5 * (1.0 - w),
                };
            }
        }
    }
}

/// Equal-power pan gain of channel `c` for a position `p` in [-1, 1],
/// unity at the centre
#[inline]
fn pan_gain(c: usize, p: f32) -> f32 {
    // sqrt2 cos(pi/4 + x) = cos x - sin x and sqrt2 sin(pi/4 + x) = cos x + sin x:
    // exactly 1 in the centre, so zero depth is bit-transparent
    let x = p.clamp(-1.0, 1.0) * 0.125;
    // |x| <= 1/8 turn, so the cosine is positive; from the sine it is exactly 1 at 0
    let sin = sin_turns(x);
    let cos = (1.0 - sin * sin).sqrt();
    if c == 0 { cos - sin } else { cos + sin }
}

impl<const C: usize> Plugin for Tremolo<C> {
    const INFO: &'static PluginInfo = if C == 1 { &MONO } else { &STEREO };

    fn new(config: &Config) -> Self {
        let info = Self::INFO;
        let ix = Ix::of(info);
        let sr = config.sample_rate;
        let smoother = |i: usize| {
            let p = param(info, i);
            Smoother::new(p.smoothing, sr as f32, p.default)
        };
        let crossover = smoother(ix.crossover);
        let (a, b, c) = crossover_coeffs(sr, crossover.current());
        let channel = Channel { split: Svf::new(a), low: Svf::new(b), high: Svf::new(c), light: 0.0 };
        let mut t = Self {
            ix,
            sample_rate: sr,
            lfo: Phasor::new(5.0, sr),
            rate: smoother(ix.rate),
            depth: smoother(ix.depth),
            shape: smoother(ix.shape),
            crossover,
            output: Smoother::new(Smoothing::Linear { ms: 20.0 }, sr as f32, 1.0),
            spread: ix.spread.map_or(Smoother::new(Smoothing::None, sr as f32, 0.0), smoother),
            channels: [channel; C],
            mode: Mode::Classic,
            old_mode: Mode::Classic,
            fade: Fade::new((MODE_FADE_MS * 0.001 * sr) as usize, true),
            opto_attack: pole(OPTO_ATTACK_MS, sr),
            opto_release: pole(OPTO_RELEASE_MS, sr),
            cycle: [[1.0; CYCLE]; C],
            fresh: true,
        };
        t.reset();
        t
    }

    fn reset(&mut self) {
        self.lfo.set_phase(0.0);
        for ch in self.channels.iter_mut() {
            ch.split.reset();
            ch.low.reset();
            ch.high.reset();
            // The lamp is lit as the LFO drive at phase 0 (sine peak region)
            ch.light = 0.5;
        }
        self.fade.finish();
        self.fresh = true;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let rate = self.target_rate(ctx);
        let mode = Mode::from_value(ctx.params.int(ix.mode));
        let targets = [
            ctx.params.get(ix.depth),
            ctx.params.get(ix.shape),
            ctx.params.get(ix.crossover),
            db_to_gain(ctx.params.get(ix.output)),
            ix.spread.map_or(0.0, |i| ctx.params.get(i)),
        ];
        let smoothers = [&mut self.depth, &mut self.shape, &mut self.crossover, &mut self.output, &mut self.spread];
        if self.fresh {
            self.rate.reset(rate);
            for (s, t) in smoothers.into_iter().zip(targets) {
                s.reset(t);
            }
            self.mode = mode;
            self.old_mode = mode;
            self.fade.finish();
            let f = self.crossover.current();
            self.set_crossover(f);
            self.static_cycle();
            self.fresh = false;
        } else {
            self.rate.set_target(rate);
            for (s, t) in smoothers.into_iter().zip(targets) {
                s.set_target(t);
            }
            if mode != self.mode {
                // Fade from the old mode to the new one
                self.old_mode = self.mode;
                self.mode = mode;
                self.fade = Fade::new((MODE_FADE_MS * 0.001 * self.sample_rate) as usize, false);
                self.fade.set(true);
            }
        }

        let sr = self.sample_rate;
        let (attack, release) = (self.opto_attack, self.opto_release);
        let mut last_gain = 1.0;
        for i in 0..ctx.frames {
            self.lfo.set_rate(f64::from(self.rate.next()), sr);
            let phase = self.lfo.next() as f32;
            let d = self.depth.next() * 0.01;
            let k = shape_steepness(self.shape.next() * 0.01);
            let out = self.output.next();
            let spread = self.spread.next() / 360.0;
            if !self.crossover.is_settled() {
                let f = self.crossover.next();
                self.set_crossover(f);
            }
            let fading = !self.fade.is_settled();
            let mix = if fading { self.fade.next() } else { 1.0 };

            for c in 0..C {
                let x = ctx.inputs.get(c).map_or(0.0, |s| s[i]);
                let ch = &mut self.channels[c];
                let ph = if c == 1 { phase + spread } else { phase };
                let ph = ph - ph.floor();
                let w = sine_to_square(ph, k);
                // Drive: 0 at the LFO peak (full level), 1 at its trough
                let m = 0.5 * (1.0 - w);
                // The lamp and the crossover always run, so a mode switch
                // finds them settled
                let target = m;
                let a = if target > ch.light { attack } else { release };
                ch.light += a * (target - ch.light);
                let bands = ch.split.bands(x);
                let low = ch.low.process(bands.low);
                let high = ch.high.process(bands.high);

                let voice = |mode: Mode| -> (f32, f32) {
                    match mode {
                        Mode::Classic => {
                            let g = 1.0 - d * m;
                            (g * x, g)
                        }
                        Mode::Opto => {
                            let l = ch.light;
                            let g = 1.0 - d * l * l.sqrt();
                            (g * x, g)
                        }
                        Mode::Harmonic => {
                            let gl = 1.0 - d * m;
                            let gh = 1.0 - d * (1.0 - m);
                            (gl * low + gh * high, gl)
                        }
                        Mode::Pan => {
                            // One LFO for both sides (spread unused)
                            let wp = if c == 1 { sine_to_square(phase, k) } else { w };
                            let g = pan_gain(c, d * wp);
                            (g * x, g)
                        }
                    }
                };
                let (mut y, g) = voice(self.mode);
                if fading {
                    let (y_old, _) = voice(self.old_mode);
                    y = y_old + (y - y_old) * mix;
                }
                if let Some(o) = ctx.outputs.get_mut(c) {
                    o[i] = y * out;
                }
                let bin = ((ph * CYCLE as f32) as usize).min(CYCLE - 1);
                self.cycle[c][bin] = g;
                if c == 0 {
                    last_gain = g;
                }
            }
        }

        ctx.meters.set(ix.phase, self.lfo.phase() as f32);
        ctx.meters.set(ix.gain, last_gain);
        if ctx.streams.due(ix.cycle) {
            let mut values = [0.0f32; CYCLE * 2];
            for (c, row) in self.cycle.iter().enumerate() {
                values[c * CYCLE..(c + 1) * CYCLE].copy_from_slice(row);
            }
            ctx.streams.send(ix.cycle, (ctx.frames - 1) as u32, self.lfo.phase() as f32, &values[..C * CYCLE]);
        }
    }
}
