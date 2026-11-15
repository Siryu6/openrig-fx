//! The four circuits. Each is a short chain of filters around one
//! clipping stage, with the corners and gains of the classic pedal
//! topologies; the clipping runs at four times the sample rate through a
//! first-order antiderivative waveshaper, which together keep aliases
//! far below the harmonics.

use crate::info::Mode;
use orx_dsp::adaa::{Adaa1, Cubic, Curve, Tanh};
use orx_dsp::filter::Shape;
use orx_dsp::filter::onepole::{DcBlocker, OnePole};
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_dsp::math::{audio_taper, db_to_gain};
use orx_dsp::oversample::{HalfbandDesign, Stage2x};

const Q: f64 = core::f64::consts::FRAC_1_SQRT_2;
/// Clipping level of the diodes, full scale = 1
const CLIP: f32 = 0.5;
/// Bias of the fuzz stage, in clipping levels: what makes it asymmetric
const FUZZ_BIAS: f32 = 0.3;
/// Corner of the bass shelf, Hz
const BASS_HZ: f64 = 150.0;
/// Output gains matching the clipping circuits' levels to the bypassed
/// level at the middle of Drive, dB (measured on a guitar at −10 dBFS
/// peaks, see the level test); a boost's Drive is its gain
const MAKEUP_DB: [f32; 4] = [0.0, -15.0, -20.0, -12.4];

/// Two-transistor fuzz: a hard, transistor-saturation knee on the
/// positive side (the cubic, stretched to keep a unit slope at 0) and the
/// slow, soft cutoff of the other transistor on the negative side, which
/// also limits lower (`x / (1 + |x| / k)`, towards −k). Both halves pass
/// through 0 with a unit slope.
#[derive(Debug, Clone, Copy, Default)]
pub struct Fuzz;

const STRETCH: f64 = 1.5;
/// Limit of the negative side
const CUTOFF: f64 = 0.7;

impl Curve for Fuzz {
    #[inline]
    fn eval(&self, x: f64) -> f64 {
        if x >= 0.0 { Cubic.eval(x / STRETCH) } else { x / (1.0 - x / CUTOFF) }
    }

    #[inline]
    fn integral(&self, x: f64) -> f64 {
        if x >= 0.0 { STRETCH * Cubic.integral(x / STRETCH) } else { CUTOFF * (-x - CUTOFF * (-x / CUTOFF).ln_1p()) }
    }
}

/// Control values of the circuit, at the end of a control period
#[derive(Debug, Clone, Copy, Default)]
pub struct Controls {
    /// 0 to 1
    pub drive: f32,
    /// 0 to 1
    pub tone: f32,
    pub bass_db: f32,
    pub level_db: f32,
}

/// A value moving linearly to a target over a control period
#[derive(Debug, Clone, Copy, Default)]
struct Ramp {
    value: f32,
    step: f32,
}

impl Ramp {
    /// Reach `target` in `samples` (0: at once)
    fn to(&mut self, target: f32, samples: u32) {
        if samples == 0 {
            self.value = target;
            self.step = 0.0;
        } else {
            self.step = (target - self.value) / samples as f32;
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        self.value += self.step;
        self.value
    }
}

/// The clipping stage at four times the rate: two half-band stages up,
/// the antiderivative waveshaper, two down. A clean signal can ride along
/// through the same filters (and the same half-sample mean as the
/// shaper), so that it stays aligned with the clipped one when they sum.
#[derive(Debug, Clone, Copy)]
struct Oversampled {
    /// Up and down filters of the clipped signal, by stage
    path: [Stage2x; 2],
    /// Up filters of the clean signal
    clean: [Stage2x; 2],
    shaper: Adaa1,
    /// Previous clean sample at the high rate
    clean_prev: f32,
}

impl Oversampled {
    fn new() -> Self {
        let stages = [Stage2x::new(&HalfbandDesign::steep()), Stage2x::new(&HalfbandDesign::relaxed())];
        Self { path: stages, clean: stages, shaper: Adaa1::default(), clean_prev: 0.0 }
    }

    /// Start from rest, the shaper's input resting at `rest` of `curve`
    fn reset(&mut self, curve: &impl Curve, rest: f32) {
        for s in self.path.iter_mut().chain(self.clean.iter_mut()) {
            s.reset();
        }
        self.shaper.prime(curve, rest);
        self.clean_prev = 0.0;
    }

    /// `shape(u)` (with `offset` added at the high rate, `shift`
    /// subtracted), plus `clean` when given
    #[inline]
    fn run(&mut self, curve: &impl Curve, u: f32, offset: f32, shift: f32, clean: Option<f32>) -> f32 {
        let [a, b] = self.path[0].upsample(u);
        let quad = [self.path[1].upsample(a), self.path[1].upsample(b)];
        let clean = clean.map(|x| {
            let [a, b] = self.clean[0].upsample(x);
            [self.clean[1].upsample(a), self.clean[1].upsample(b)]
        });
        let mut half = [0.0f32; 2];
        for (k, pair) in quad.iter().enumerate() {
            let mut out = [0.0f32; 2];
            for (j, u) in pair.iter().enumerate() {
                let mut y = self.shaper.process(curve, u + offset) - shift;
                if let Some(c) = &clean {
                    let x = c[k][j];
                    y += 0.5 * (x + self.clean_prev);
                    self.clean_prev = x;
                }
                out[j] = y;
            }
            half[k] = self.path[1].downsample(out);
        }
        self.path[0].downsample(half)
    }
}

/// One circuit with its state
pub struct Voice {
    mode: Mode,
    sample_rate: f64,
    coupling: OnePole,
    bass: Svf,
    path_hp: OnePole,
    path_lp: OnePole,
    clipper: Oversampled,
    dc: DcBlocker,
    tone_shelf: Svf,
    tone_lp: OnePole,
    tone_hp: OnePole,
    gain: Ramp,
    out: Ramp,
    blend: Ramp,
}

impl Voice {
    pub fn new(sample_rate: f64) -> Self {
        let flat = SvfCoeffs::new(Shape::Peak, sample_rate, 1000.0, Q, 0.0);
        let mut v = Self {
            mode: Mode::Boost,
            sample_rate,
            coupling: OnePole::new(sample_rate, 20.0),
            bass: Svf::new(flat),
            path_hp: OnePole::new(sample_rate, 720.0),
            path_lp: OnePole::new(sample_rate, 10_000.0),
            clipper: Oversampled::new(),
            dc: DcBlocker::new(sample_rate, 5.0),
            tone_shelf: Svf::new(flat),
            tone_lp: OnePole::new(sample_rate, 400.0),
            tone_hp: OnePole::new(sample_rate, 1200.0),
            gain: Ramp::default(),
            out: Ramp::default(),
            blend: Ramp::default(),
        };
        v.start(Mode::Boost, &Controls::default());
        v
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Become `mode` from rest, set to `controls` at once
    pub fn start(&mut self, mode: Mode, controls: &Controls) {
        self.mode = mode;
        let sr = self.sample_rate;
        // Input coupling: wide open for the boost, tighter for the
        // high-gain circuits, whose bass would otherwise flub
        let coupling = match mode {
            Mode::Boost | Mode::Overdrive => 20.0,
            Mode::Distortion | Mode::Fuzz => 80.0,
        };
        self.coupling = OnePole::new(sr, coupling);
        self.path_hp.reset();
        self.path_lp.reset();
        self.bass.reset();
        self.tone_shelf.reset();
        self.tone_lp = OnePole::new(sr, 400.0);
        self.tone_hp.reset();
        self.dc.reset();
        match mode {
            Mode::Boost | Mode::Overdrive => self.clipper.reset(&Tanh, 0.0),
            Mode::Distortion => self.clipper.reset(&Cubic, 0.0),
            Mode::Fuzz => self.clipper.reset(&Fuzz, FUZZ_BIAS),
        }
        self.configure(controls, 0);
    }

    /// Follow `controls` over the next `samples` (0: at once)
    pub fn configure(&mut self, c: &Controls, samples: u32) {
        let sr = self.sample_rate;
        let nyquist = 0.45 * sr;
        self.bass.set(SvfCoeffs::new(Shape::LowShelf, sr, BASS_HZ, Q, f64::from(c.bass_db)));
        let tone = f64::from(c.tone);
        let gain = match self.mode {
            Mode::Boost => {
                self.tone_shelf.set(SvfCoeffs::new(Shape::HighShelf, sr, 1500.0, Q, (tone - 0.5) * 20.0));
                db_to_gain(20.0 * c.drive)
            }
            Mode::Overdrive => {
                // Drive pot in series with 51 kΩ over 4.7 kΩ to ground,
                // 51 pF across: more gain, less treble in the clipped path
                let r = 51e3 + f64::from(audio_taper(c.drive)) * 500e3;
                let corner = 1.0 / (core::f64::consts::TAU * r * 51e-12);
                self.path_lp.set_freq(sr, corner.min(nyquist));
                self.tone_shelf.set(SvfCoeffs::new(Shape::HighShelf, sr, 1000.0, Q, -15.0 + 20.0 * tone));
                (r / 4.7e3) as f32 / CLIP
            }
            Mode::Distortion => {
                // The op-amp's gain-bandwidth product closes the top
                // as the gain rises
                let g = db_to_gain(6.0 + 60.0 * c.drive);
                self.path_lp.set_freq(sr, (3e6 / f64::from(g)).min(nyquist));
                self.tone_lp.set_freq(sr, 475.0 * (16_000.0f64 / 475.0).powf(tone));
                g / CLIP
            }
            Mode::Fuzz => {
                self.blend.to(c.tone, samples);
                db_to_gain(20.0 + 40.0 * c.drive) / CLIP
            }
        };
        self.gain.to(gain, samples);
        self.out.to(db_to_gain(MAKEUP_DB[self.mode as usize] + c.level_db), samples);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = self.bass.process(self.coupling.highpass(x));
        let g = self.gain.next();
        let y = match self.mode {
            Mode::Boost => self.clipper.run(&Tanh, g * x, 0.0, 0.0, None),
            Mode::Overdrive => {
                // The clean signal plus the clipped, mid-focused gain path
                let p = self.path_lp.lowpass(g * self.path_hp.highpass(x));
                CLIP * self.clipper.run(&Tanh, p, 0.0, 0.0, Some(x / CLIP))
            }
            Mode::Distortion => {
                let u = self.path_lp.lowpass(g * x);
                CLIP * self.clipper.run(&Cubic, u, 0.0, 0.0, None)
            }
            Mode::Fuzz => {
                let rest = Fuzz.eval(f64::from(FUZZ_BIAS)) as f32;
                CLIP * self.clipper.run(&Fuzz, g * x, FUZZ_BIAS, rest, None)
            }
        };
        let y = self.dc.process(y);
        let y = match self.mode {
            Mode::Boost | Mode::Overdrive => self.tone_shelf.process(y),
            Mode::Distortion => self.tone_lp.lowpass(y),
            Mode::Fuzz => {
                let t = self.blend.next();
                let lo = self.tone_lp.lowpass(y);
                let hi = self.tone_hp.highpass(y);
                lo + (hi - lo) * t
            }
        };
        y * self.out.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzz_integral_is_an_antiderivative() {
        for i in -60..=60 {
            let x = f64::from(i) * 0.1 + 0.013;
            let h = 1e-5;
            let d = (Fuzz.integral(x + h) - Fuzz.integral(x - h)) / (2.0 * h);
            assert!((d - Fuzz.eval(x)).abs() < 1e-6, "x {x}");
        }
        assert_eq!(Fuzz.integral(0.0), 0.0);
    }

    #[test]
    fn fuzz_is_continuous_with_a_unit_slope() {
        let h = 1e-7;
        assert!((Fuzz.eval(h) / h - 1.0).abs() < 1e-6);
        assert!((Fuzz.eval(-h) / -h - 1.0).abs() < 1e-6);
        assert!(Fuzz.eval(10.0) == 1.0 && Fuzz.eval(-10.0) > -CUTOFF);
    }
}
