//! The echo processor.

use crate::info::{HIGH_CUT_OFF, LOW_CUT_OFF, MAX_SYNC_MS, MIN_MS, MONO, Mode, STEREO};
use crate::modulation::{MAX_DEPTH_S, Modulation};
use orx_dsp::Smoother;
use orx_dsp::adaa::{Adaa1, Rational, Tanh};
use orx_dsp::delay::DelayLine;
use orx_dsp::env::EnvelopeFollower;
use orx_dsp::filter::Shape;
use orx_dsp::filter::onepole::OnePole;
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_dsp::math::{db_to_gain, gain_to_db};
use orx_dsp::reader::{Motion, Reader};
use orx_dsp::xfade::Fade;
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};

/// Filter coefficients follow their controls on this grid of samples
/// (counted by the plugin, so the output does not depend on block sizes)
const CONTROL_PERIOD: u32 = 16;
/// Mode, ping-pong and filter on/off changes crossfade over this time
const SWITCH_MS: f64 = 20.0;
/// Glide of the tape transport and of the BBD clock
const TAPE_GLIDE: Motion = Motion::Glide { omega: 25.0, max_rate: 0.5 };
const ANALOG_GLIDE: Motion = Motion::Glide { omega: 40.0, max_rate: 0.5 };
/// Tape: low cut of the heads, Hz, and the high cut at 250 ms (the
/// faster the tape, the wider the band)
const TAPE_LOW_HZ: f64 = 60.0;
const TAPE_HIGH_HZ: f64 = 5_000.0;
/// BBD: bucket count of the line, and the share of its clock frequency
/// its filters pass
const BBD_STAGES: f64 = 4096.0;
const BBD_BAND: f64 = 0.45;
/// Loop limiter: the level the feedback loop never exceeds, and its
/// release (the attack is immediate)
const LIMIT: f32 = 1.0;
const LIMIT_RELEASE_MS: f64 = 150.0;
/// Ducking envelope, and the width of its knee around the threshold, dB
const DUCK_ATTACK_MS: f32 = 5.0;
const DUCK_RELEASE_MS: f32 = 250.0;
const DUCK_KNEE_DB: f32 = 6.0;
/// What the loop writes below this level counts as silence (−100 dBFS)
const SILENCE: f32 = 1e-5;

/// Port indices, found by symbol (they differ between the variants)
#[derive(Debug, Clone, Copy)]
struct Ix {
    time: usize,
    sync: usize,
    feedback: usize,
    level: usize,
    mode: usize,
    low_cut: usize,
    high_cut: usize,
    modulation: usize,
    drive: usize,
    duck: usize,
    duck_threshold: usize,
    dry: usize,
    ping_pong: Option<usize>,
    delay_ms: usize,
    ducking: usize,
}

impl Ix {
    fn of(info: &PluginInfo) -> Self {
        let i = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
        Self {
            time: i("time"),
            sync: i("sync"),
            feedback: i("feedback"),
            level: i("level"),
            mode: i("mode"),
            low_cut: i("low_cut"),
            high_cut: i("high_cut"),
            modulation: i("modulation"),
            drive: i("drive"),
            duck: i("duck"),
            duck_threshold: i("duck_threshold"),
            dry: i("dry"),
            ping_pong: info.port_index("ping_pong"),
            delay_ms: i("delay_ms"),
            ducking: i("ducking"),
        }
    }
}

fn param(info: &PluginInfo, index: usize) -> Param {
    match info.ports.get(index).map(|p| p.kind) {
        Some(PortKind::Param(p)) => p,
        _ => Param::float(0.0, 0.0, 1.0),
    }
}

/// Tape voicing of the loop: head band limits, then the tape's
/// saturation
#[derive(Debug, Clone, Copy)]
struct TapeVoice {
    low: OnePole,
    high: Svf,
    saturation: Adaa1,
}

/// Analog voicing: the BBD's anti-aliasing and reconstruction filters,
/// whose band follows its clock, then its softer clipping
#[derive(Debug, Clone, Copy)]
struct AnalogVoice {
    high: Svf,
    saturation: Adaa1,
}

/// One channel's line and loop state
struct Channel {
    line: DelayLine,
    low_cut: Svf,
    high_cut: Svf,
    tape: TapeVoice,
    analog: AnalogVoice,
}

impl Channel {
    fn reset(&mut self) {
        self.line.reset();
        self.low_cut.reset();
        self.high_cut.reset();
        self.reset_voice(Mode::Tape);
        self.reset_voice(Mode::Analog);
    }

    fn reset_voice(&mut self, mode: Mode) {
        match mode {
            Mode::Digital => {}
            Mode::Tape => {
                self.tape.low.reset();
                self.tape.high.reset();
                self.tape.saturation.reset();
            }
            Mode::Analog => {
                self.analog.high.reset();
                self.analog.saturation.reset();
            }
        }
    }

    /// The colour of `mode` on the loop signal `u`, driven by `drive`
    #[inline]
    fn voice(&mut self, mode: Mode, u: f32, drive: f32) -> f32 {
        match mode {
            Mode::Digital => u,
            Mode::Tape => {
                let t = &mut self.tape;
                let band = t.high.process(t.low.highpass(u));
                t.saturation.process(&Tanh, band * drive) / drive
            }
            Mode::Analog => {
                let a = &mut self.analog;
                let band = a.high.process(u);
                a.saturation.process(&Rational, band * drive) / drive
            }
        }
    }
}

/// High cut of the tape heads for a delay of `seconds`: the band grows
/// with the square root of the tape speed
fn tape_high_cut(seconds: f64) -> f64 {
    (TAPE_HIGH_HZ * (0.25 / seconds.max(1e-3)).sqrt()).clamp(2_500.0, 10_000.0)
}

/// High cut of a BBD line clocked for a delay of `seconds`
fn bbd_high_cut(seconds: f64) -> f64 {
    // A line of N buckets delays by N / (2·f_clock)
    let clock = BBD_STAGES / (2.0 * seconds.max(1e-3));
    (BBD_BAND * clock).clamp(1_200.0, 9_000.0)
}

/// OR Echo with `C` channels (1 or 2)
pub struct Echo<const C: usize> {
    ix: Ix,
    sample_rate: f64,
    channels: [Channel; C],
    reader: Reader,
    modulation: Modulation,
    feedback: Smoother,
    level: Smoother,
    drive: Smoother,
    duck: Smoother,
    low_cut: Smoother,
    high_cut: Smoother,
    low_cut_on: Fade,
    high_cut_on: Fade,
    dry: Fade,
    ping_pong: Fade,
    mode: Mode,
    /// Mode being faded out, while `mode_fade` runs
    old_mode: Mode,
    mode_fade: Fade,
    /// Gain the loop limiter remembers (peak, immediate attack)
    limit_peak: f32,
    limit_release: f32,
    duck_env: EnvelopeFollower,
    duck_reduction: f32,
    /// Samples to the next coefficient update
    control: u32,
    /// Samples since the loop last wrote anything above silence
    quiet: u64,
    /// Smoothers snap to the parameters on the next block
    fresh: bool,
}

impl<const C: usize> Echo<C> {
    /// Delay time asked for now, samples
    fn target(&self, ctx: &Context<'_>) -> f64 {
        let ms = match orx_plugin::tempo::seconds(ctx.params.int(self.ix.sync), ctx.position.bpm) {
            Some(s) => (s * 1000.0).clamp(f64::from(MIN_MS), f64::from(MAX_SYNC_MS)),
            None => f64::from(ctx.params.get(self.ix.time)),
        };
        ms * 0.001 * self.sample_rate
    }

    fn switch_fade(&self, on: bool) -> Fade {
        Fade::new((SWITCH_MS * 0.001 * self.sample_rate) as usize, on)
    }

    /// Filter coefficients for the current settings
    fn update_filters(&mut self) {
        let sr = self.sample_rate;
        let seconds = self.reader.current() / sr;
        let q = core::f64::consts::FRAC_1_SQRT_2;
        let low = SvfCoeffs::new(Shape::Highpass, sr, f64::from(self.low_cut.current()), q, 0.0);
        let high = SvfCoeffs::lowpass(sr, f64::from(self.high_cut.current()));
        let tape = SvfCoeffs::lowpass(sr, tape_high_cut(seconds));
        let analog = SvfCoeffs::lowpass(sr, bbd_high_cut(seconds));
        for ch in self.channels.iter_mut() {
            ch.low_cut.set(low);
            ch.high_cut.set(high);
            ch.tape.high.set(tape);
            ch.analog.high.set(analog);
        }
    }

    fn set_switches(&mut self, ctx: &Context<'_>, snap: bool) {
        let ix = self.ix;
        let targets = [
            (&mut self.low_cut_on, ctx.params.get(ix.low_cut) > LOW_CUT_OFF),
            (&mut self.high_cut_on, ctx.params.get(ix.high_cut) < HIGH_CUT_OFF),
            (&mut self.dry, ctx.params.on(ix.dry)),
            (&mut self.ping_pong, C == 2 && ix.ping_pong.is_some_and(|i| ctx.params.on(i))),
        ];
        for (fade, on) in targets {
            fade.set(on);
            if snap {
                fade.finish();
            }
        }
    }
}

/// Gain into the saturation for a drive setting, 0 to 100 %
fn drive_gain(percent: f32) -> f32 {
    db_to_gain(-6.0 + 0.18 * percent)
}

impl<const C: usize> Plugin for Echo<C> {
    const INFO: &'static PluginInfo = if C == 1 { &MONO } else { &STEREO };
    const TRAILS: bool = true;

    fn new(config: &Config) -> Self {
        let info = Self::INFO;
        let ix = Ix::of(info);
        let sr = config.sample_rate;
        let smoother = |i: usize| {
            let p = param(info, i);
            Smoother::new(p.smoothing, sr as f32, p.default)
        };
        let longest = (f64::from(MAX_SYNC_MS) * 0.001 + MAX_DEPTH_S) * sr;
        let shortest = f64::from(MIN_MS) * 0.001 * sr;
        let low_pass = SvfCoeffs::lowpass(sr, 5_000.0);
        let channels = core::array::from_fn(|_| Channel {
            line: DelayLine::new(longest.ceil() as usize + 4),
            low_cut: Svf::new(SvfCoeffs::new(Shape::Highpass, sr, f64::from(LOW_CUT_OFF), 0.7, 0.0)),
            high_cut: Svf::new(low_pass),
            tape: TapeVoice {
                low: OnePole::new(sr, TAPE_LOW_HZ),
                high: Svf::new(low_pass),
                saturation: Adaa1::default(),
            },
            analog: AnalogVoice { high: Svf::new(low_pass), saturation: Adaa1::default() },
        });
        let switch = |on| Fade::new((SWITCH_MS * 0.001 * sr) as usize, on);
        let mut e = Self {
            ix,
            sample_rate: sr,
            channels,
            reader: Reader::new(sr, shortest, f64::from(MAX_SYNC_MS) * 0.001 * sr),
            modulation: Modulation::new(sr),
            feedback: smoother(ix.feedback),
            level: smoother(ix.level),
            drive: smoother(ix.drive),
            duck: smoother(ix.duck),
            low_cut: smoother(ix.low_cut),
            high_cut: smoother(ix.high_cut),
            low_cut_on: switch(false),
            high_cut_on: switch(false),
            dry: switch(true),
            ping_pong: switch(false),
            mode: Mode::Digital,
            old_mode: Mode::Digital,
            mode_fade: switch(true),
            limit_peak: 0.0,
            limit_release: (-1.0 / (LIMIT_RELEASE_MS * 0.001 * sr)).exp() as f32,
            duck_env: EnvelopeFollower::new(DUCK_ATTACK_MS, DUCK_RELEASE_MS, sr as f32),
            duck_reduction: 0.0,
            control: 0,
            quiet: u64::MAX,
            fresh: true,
        };
        e.reset();
        e
    }

    fn reset(&mut self) {
        for ch in self.channels.iter_mut() {
            ch.reset();
        }
        self.modulation.reset();
        self.mode_fade.finish();
        self.limit_peak = 0.0;
        self.duck_env.reset();
        self.duck_reduction = 0.0;
        self.control = 0;
        self.quiet = u64::MAX;
        self.fresh = true;
    }

    fn is_ringing(&self) -> bool {
        let reach = self.reader.longest() + MAX_DEPTH_S * self.sample_rate + 4.0;
        (self.quiet as f64) < reach
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let target = self.target(ctx);
        let mode = Mode::from_value(ctx.params.int(ix.mode));
        let targets = [
            ctx.params.get(ix.feedback) * 0.01,
            ctx.params.get(ix.level) * 0.01,
            drive_gain(ctx.params.get(ix.drive)),
            ctx.params.get(ix.duck),
            ctx.params.get(ix.low_cut),
            ctx.params.get(ix.high_cut),
        ];
        let modulation = ctx.params.get(ix.modulation) * 0.01;
        let fresh = self.fresh;
        let smoothers = [
            &mut self.feedback,
            &mut self.level,
            &mut self.drive,
            &mut self.duck,
            &mut self.low_cut,
            &mut self.high_cut,
        ];
        for (s, t) in smoothers.into_iter().zip(targets) {
            if fresh {
                s.reset(t);
            } else {
                s.set_target(t);
            }
        }
        if fresh {
            self.mode = mode;
            self.old_mode = mode;
            self.mode_fade.finish();
            self.reader.snap(target);
            self.control = 0;
            self.fresh = false;
        } else if mode != self.mode {
            self.old_mode = self.mode;
            self.mode = mode;
            for ch in self.channels.iter_mut() {
                ch.reset_voice(mode);
            }
            self.mode_fade = self.switch_fade(false);
            self.mode_fade.set(true);
        }
        self.set_switches(ctx, fresh);
        self.modulation.set(mode, modulation, fresh);
        let motion = match mode {
            Mode::Digital => Motion::Jump,
            Mode::Tape => TAPE_GLIDE,
            Mode::Analog => ANALOG_GLIDE,
        };
        let duck_threshold = ctx.params.get(ix.duck_threshold);

        for i in 0..ctx.frames {
            if self.control == 0 {
                self.update_filters();
                self.control = CONTROL_PERIOD;
            }
            self.control -= 1;

            let feedback = self.feedback.next();
            let level = self.level.next();
            let drive = self.drive.next();
            let duck = self.duck.next();
            self.low_cut.next();
            self.high_cut.next();
            let low_on = self.low_cut_on.next();
            let high_on = self.high_cut_on.next();
            let dry = self.dry.next();
            let ping = self.ping_pong.next();
            let new_mode = self.mode_fade.next();
            let offsets = self.modulation.next();
            self.reader.step(target, motion);

            let mut x = [0.0f32; C];
            let mut echo = [0.0f32; C];
            for c in 0..C {
                x[c] = ctx.inputs.get(c).and_then(|s| s.get(i)).copied().unwrap_or(0.0);
                echo[c] = self.reader.read(&self.channels[c].line, offsets[c]);
            }

            // Into each line: its input and its echo, or in ping-pong the
            // summed input into the left and each echo into the other line
            let mut u = [0.0f32; C];
            for c in 0..C {
                let straight = x[c] + feedback * echo[c];
                u[c] = if C == 2 {
                    let crossed =
                        if c == 0 { 0.5 * (x[0] + x[C - 1]) + feedback * echo[C - 1] } else { feedback * echo[0] };
                    straight + (crossed - straight) * ping
                } else {
                    straight
                };
            }

            // Loop filters, the voicing, then the limiter
            let mut w = [0.0f32; C];
            let mut peak = 0.0f32;
            for c in 0..C {
                let ch = &mut self.channels[c];
                let mut s = u[c];
                let low = ch.low_cut.process(s);
                s += (low - s) * low_on;
                let high = ch.high_cut.process(s);
                s += (high - s) * high_on;
                let mut v = ch.voice(self.mode, s, drive);
                if new_mode < 1.0 {
                    let old = ch.voice(self.old_mode, s, drive);
                    v = old + (v - old) * new_mode;
                }
                w[c] = v;
                peak = peak.max(v.abs());
            }
            self.limit_peak = peak.max(self.limit_peak * self.limit_release);
            let limit = if self.limit_peak > LIMIT { LIMIT / self.limit_peak } else { 1.0 };
            let mut loud = false;
            for (ch, v) in self.channels.iter_mut().zip(w) {
                let v = v * limit;
                loud |= v.abs() > SILENCE;
                ch.line.write(v);
            }
            self.quiet = if loud { 0 } else { self.quiet.saturating_add(1) };

            // Ducking: the echoes dip while the input is above the threshold
            let input_peak = x.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let env = self.duck_env.process(input_peak);
            let mut wet = level;
            self.duck_reduction = 0.0;
            if duck > 0.0 {
                let over = gain_to_db(env) - duck_threshold;
                let t = (over / DUCK_KNEE_DB + 0.5).clamp(0.0, 1.0);
                self.duck_reduction = duck * t * t * (3.0 - 2.0 * t);
                wet *= db_to_gain(-self.duck_reduction);
            }

            for c in 0..C {
                if let Some(o) = ctx.outputs.get_mut(c).and_then(|s| s.get_mut(i)) {
                    *o = dry * x[c] + wet * echo[c];
                }
            }
        }

        ctx.meters.set(ix.delay_ms, (self.reader.current() / self.sample_rate * 1000.0) as f32);
        ctx.meters.set(ix.ducking, self.duck_reduction);
    }
}
