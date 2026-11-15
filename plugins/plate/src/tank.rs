//! The reverberator: input diffusion and a figure-eight tank, after
//! J. Dattorro, "Effect Design, Part 1: Reverberator and Other Filters"
//! (JAES 45(9), 1997), the classic studio plate structure.
//!
//! Each input passes four all-passes that smear it into a dense cloud.
//! The tank is two halves in a loop, each a modulated all-pass, a delay,
//! an absorbent filter, an all-pass and a second delay; each half's end
//! feeds the other's start. The left input enters the half the left
//! outputs mostly tap, the right input the other, so a stereo source
//! keeps its sides. Fourteen taps spread over the six delay elements
//! build the two outputs, decorrelated.
//!
//! The decay is set as a reverberation time: every delay element of the
//! loop, the all-passes' own included, gets the gain `10^(−3·L/(fs·T60))`
//! for its length `L`. That is the substitution `z⁻¹ → λ·z⁻¹` over the
//! whole loop, which scales the radius of every pole by the same `λ`: every
//! mode falls by 60 dB in `T60`, whatever its frequency or the sample
//! rate. (Gains on the plain delays only would leave the modes caught in
//! the all-passes' inner loops ringing longer: about 18 % longer here.) Each half's absorbent filter
//! is a pair of first-order shelves (Jot's method) whose gains give the
//! lows and the highs their own reverberation time across the two
//! crossovers: the curve a host draws from the decay roles is the one
//! the plate has.

use orx_dsp::delay::DelayLine;
use orx_dsp::filter::Shape;
use orx_dsp::filter::svf::{Svf, SvfCoeffs};

/// Sample rate of the published lengths
const DESIGN_RATE: f64 = 29_761.0;
/// Input all-passes: length at `DESIGN_RATE`, gain
const INPUT_DIFFUSION: [(usize, f32); 4] = [(142, 0.75), (107, 0.75), (379, 0.625), (277, 0.625)];
/// Gain of the tank's modulated all-passes (sign as published) and of
/// its second all-passes
const DECAY_DIFFUSION_1: f32 = -0.70;
const DECAY_DIFFUSION_2: f32 = 0.50;
/// The tank's halves at `DESIGN_RATE`: modulated all-pass, first delay,
/// all-pass, second delay
const HALVES: [[usize; 4]; 2] = [[672, 4453, 1800, 3720], [908, 4217, 2656, 3163]];
/// Largest excursion of the modulated all-passes at full depth, s
pub(crate) const MAX_EXCURSION_S: f64 = 1.1e-3;
/// Output scale of the published structure
const OUTPUT_GAIN: f32 = 0.6;

/// A tap: half, element (0 first delay, 1 all-pass, 2 second delay),
/// position at `DESIGN_RATE`, sign
type Tap = (usize, usize, usize, f32);

/// Taps of the left and right outputs
const TAPS: [[Tap; 7]; 2] = [
    [
        (1, 0, 266, 1.0),
        (1, 0, 2974, 1.0),
        (1, 1, 1913, -1.0),
        (1, 2, 1996, 1.0),
        (0, 0, 1990, -1.0),
        (0, 1, 187, -1.0),
        (0, 2, 1066, -1.0),
    ],
    [
        (0, 0, 353, 1.0),
        (0, 0, 3627, 1.0),
        (0, 1, 1228, -1.0),
        (0, 2, 2673, 1.0),
        (1, 0, 2111, -1.0),
        (1, 1, 335, -1.0),
        (1, 2, 121, -1.0),
    ],
];

fn scaled(len: usize, sample_rate: f64) -> usize {
    ((len as f64) * sample_rate / DESIGN_RATE).round().max(1.0) as usize
}

/// A Schroeder all-pass of `len` samples
struct Allpass {
    line: DelayLine,
    len: usize,
}

impl Allpass {
    fn new(len: usize, extra: usize) -> Self {
        Self { line: DelayLine::new(len + extra + 4), len }
    }

    /// `x` through the all-pass of gain `g`, its delay attenuated by `loss`
    #[inline]
    fn process(&mut self, x: f32, g: f32, loss: f32) -> f32 {
        // Read before the write: `read(len − 1)` is `len` samples ago
        self.process_delayed(x, g, self.line.read(self.len - 1) * loss)
    }

    /// With the delay moved by `offset` samples (cubic interpolation)
    #[inline]
    fn process_modulated(&mut self, x: f32, g: f32, loss: f32, offset: f32) -> f32 {
        let d = self.line.read_hermite((self.len - 1) as f32 + offset);
        self.process_delayed(x, g, d * loss)
    }

    #[inline]
    fn process_delayed(&mut self, x: f32, g: f32, delayed: f32) -> f32 {
        let w = x + g * delayed;
        self.line.write(w);
        delayed - g * w
    }
}

/// A plain delay of `len` samples
struct Delay {
    line: DelayLine,
    len: usize,
}

impl Delay {
    fn new(len: usize) -> Self {
        Self { line: DelayLine::new(len + 4), len }
    }

    /// The output of this sample, before [`Delay::write`]
    #[inline]
    fn output(&self) -> f32 {
        self.line.read(self.len - 1)
    }
}

/// The frequency-dependent part of a half's loss: a low shelf and a high
/// shelf, second order (Butterworth, no bump), each centred on its
/// crossover (half its gain, in dB, there). Second order keeps the
/// transitions within about an octave and a half, so a large ratio
/// between the bands does not reach into the mids.
#[derive(Clone, Copy)]
struct Absorbent {
    low: Svf,
    high: Svf,
}

impl Absorbent {
    /// `x` through the shelves, crossfaded to `x` itself by `hold`
    #[inline]
    fn process(&mut self, x: f32, hold: f32) -> f32 {
        let y = self.high.process(self.low.process(x));
        y + (x - y) * hold
    }
}

/// Loop gains of one half for a decay setting
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct HalfGains {
    /// Mid-band gains of the modulated all-pass, the first delay, the
    /// second all-pass and the second delay, for their lengths
    pub(crate) elements: [f32; 4],
    /// Low and high band gains of the absorbent filter, relative to the mids
    pub(crate) low: f32,
    pub(crate) high: f32,
}

/// What sets the decay: mid RT60 (s), the low and high RT60 as multiples
/// of it, the crossovers (Hz)
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Decay {
    pub(crate) seconds: f64,
    pub(crate) low: f64,
    pub(crate) high: f64,
    pub(crate) crossover_low: f64,
    pub(crate) crossover_high: f64,
}

struct Half {
    modulated: Allpass,
    first: Delay,
    absorbent: Absorbent,
    diffuser: Allpass,
    second: Delay,
    gains: HalfGains,
}

impl Half {
    fn new(lengths: [usize; 4], sample_rate: f64) -> Self {
        let [a, d1, b, d2] = lengths.map(|l| scaled(l, sample_rate));
        let excursion = (MAX_EXCURSION_S * sample_rate).ceil() as usize + 2;
        let flat = SvfCoeffs::new(Shape::HighShelf, sample_rate, 1000.0, core::f64::consts::FRAC_1_SQRT_2, 0.0);
        Self {
            modulated: Allpass::new(a, excursion),
            first: Delay::new(d1),
            absorbent: Absorbent { low: Svf::new(flat), high: Svf::new(flat) },
            diffuser: Allpass::new(b, 0),
            second: Delay::new(d2),
            gains: HalfGains::default(),
        }
    }

    /// Lengths of the four delay elements, samples
    fn lengths(&self) -> [usize; 4] {
        [self.modulated.len, self.first.len, self.diffuser.len, self.second.len]
    }

    fn reset(&mut self) {
        self.modulated.line.reset();
        self.first.line.reset();
        self.absorbent.low.reset();
        self.absorbent.high.reset();
        self.diffuser.line.reset();
        self.second.line.reset();
    }

    fn element(&self, element: usize) -> &DelayLine {
        match element {
            0 => &self.first.line,
            1 => &self.diffuser.line,
            _ => &self.second.line,
        }
    }
}

/// The input diffusers and the tank
pub(crate) struct Tank {
    sample_rate: f64,
    diffusers: [[Allpass; 4]; 2],
    halves: [Half; 2],
    taps: [[(usize, usize, usize, f32); 7]; 2],
    /// Longest path from an input to the last output it reaches, samples
    reach: usize,
}

impl Tank {
    pub(crate) fn new(sample_rate: f64) -> Self {
        let diffusers =
            core::array::from_fn(|_| INPUT_DIFFUSION.map(|(len, _)| Allpass::new(scaled(len, sample_rate), 0)));
        let halves = HALVES.map(|h| Half::new(h, sample_rate));
        let taps = TAPS.map(|side| side.map(|(h, e, at, sign)| (h, e, scaled(at, sample_rate), sign)));
        let diffusion: usize = INPUT_DIFFUSION.iter().map(|(len, _)| scaled(*len, sample_rate)).sum();
        let tank: usize = halves.iter().flat_map(|h| h.lengths()).sum();
        Self { sample_rate, diffusers, halves, taps, reach: diffusion + tank }
    }

    pub(crate) fn reset(&mut self) {
        for d in self.diffusers.iter_mut().flatten() {
            d.line.reset();
        }
        for h in self.halves.iter_mut() {
            h.reset();
        }
    }

    /// Input diffusion plus a full turn of the tank, samples: after this
    /// long without sound above silence at the tank's nodes, the plate is
    /// silent
    pub(crate) fn reach(&self) -> usize {
        self.reach
    }

    /// Set the loop gains for `decay`
    pub(crate) fn set_decay(&mut self, decay: Decay) {
        let sr = self.sample_rate;
        // Gain of `len` samples for a reverberation time `t`
        let gain = |len: usize, t: f64| 10f64.powf(-3.0 * len as f64 / (sr * t));
        for h in self.halves.iter_mut() {
            let lengths = h.lengths();
            let total = lengths.iter().sum::<usize>() as f64;
            let relative = |multiple: f64| {
                // The band's gain over the half divided by the mids'
                10f64.powf(-3.0 * total / (sr * decay.seconds) * (1.0 / multiple - 1.0)) as f32
            };
            h.gains = HalfGains {
                elements: lengths.map(|l| gain(l, decay.seconds) as f32),
                low: relative(decay.low),
                high: relative(decay.high),
            };
            let q = core::f64::consts::FRAC_1_SQRT_2;
            let db = |gain: f32| 20.0 * f64::from(gain).log10();
            h.absorbent.low.set(SvfCoeffs::new(Shape::LowShelf, sr, decay.crossover_low, q, db(h.gains.low)));
            h.absorbent.high.set(SvfCoeffs::new(Shape::HighShelf, sr, decay.crossover_high, q, db(h.gains.high)));
        }
    }

    #[cfg(test)]
    pub(crate) fn gains(&self) -> [HalfGains; 2] {
        [self.halves[0].gains, self.halves[1].gains]
    }

    /// One sample. `input` is the left and right input, `hold` (0 to 1)
    /// moves every loop gain towards 1 (freeze), `offsets` move the two
    /// modulated all-passes (samples). Returns the left and right
    /// outputs, and the largest level the tank wrote.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2], hold: f32, offsets: [f32; 2]) -> ([f32; 2], f32) {
        let mut diffused = [0.0f32; 2];
        for ((d, chain), x) in diffused.iter_mut().zip(self.diffusers.iter_mut()).zip(input) {
            *d = chain.iter_mut().zip(INPUT_DIFFUSION).fold(x, |v, (ap, (_, g))| ap.process(v, g, 1.0));
        }
        let held = |g: f32| g + (1.0 - g) * hold;
        // Each half's end, from the samples before this one
        let ends = [0, 1].map(|k| {
            let h = &self.halves[k];
            h.second.output() * held(h.gains.elements[3])
        });
        let mut written = 0.0f32;
        // The left input enters half 1 (the left outputs' half), the
        // right half 0
        for (k, h) in self.halves.iter_mut().enumerate() {
            let x = diffused[k ^ 1] + ends[k ^ 1];
            let g = h.gains;
            let a = h.modulated.process_modulated(x, DECAY_DIFFUSION_1, held(g.elements[0]), offsets[k]);
            let d1 = h.first.output();
            h.first.line.write(a);
            let absorbed = h.absorbent.process(d1, hold) * held(g.elements[1]);
            let b = h.diffuser.process(absorbed, DECAY_DIFFUSION_2, held(g.elements[2]));
            h.second.line.write(b);
            written = written.max(a.abs()).max(b.abs());
        }
        let out = self.taps.map(|side| {
            side.iter().map(|&(h, e, at, sign)| sign * self.halves[h].element(e).read(at)).sum::<f32>() * OUTPUT_GAIN
        });
        (out, written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 48_000.0;

    fn decay(seconds: f64) -> Decay {
        Decay { seconds, low: 1.0, high: 1.0, crossover_low: 300.0, crossover_high: 4000.0 }
    }

    /// Envelope of the impulse response, dB per second, from the energy
    /// in two windows of the tail
    fn slope(tank: &mut Tank, seconds: f64) -> f64 {
        let n = (seconds * 1.5 * SR) as usize;
        let mut energy = vec![0.0f64; n];
        for (i, e) in energy.iter_mut().enumerate() {
            let x = if i == 0 { 1.0 } else { 0.0 };
            let ([l, r], _) = tank.process([x, x], 0.0, [0.0, 0.0]);
            *e = f64::from(l) * f64::from(l) + f64::from(r) * f64::from(r);
        }
        let window = |from: f64| {
            let a = (from * SR) as usize;
            energy[a..a + (0.1 * SR) as usize].iter().sum::<f64>()
        };
        let (t0, t1) = (0.3 * seconds, 1.2 * seconds);
        10.0 * (window(t1) / window(t0)).log10() / (t1 - t0)
    }

    #[test]
    fn the_tail_falls_60_db_in_the_decay_time() {
        for seconds in [5.0, 2.0, 0.8] {
            let mut t = Tank::new(SR);
            t.set_decay(decay(seconds));
            let s = slope(&mut t, seconds);
            assert!((s + 60.0 / seconds).abs() < 0.02 * 60.0 / seconds, "{seconds} s: {s} dB/s");
        }
    }

    #[test]
    fn band_gains_follow_their_decay_times() {
        let mut t = Tank::new(SR);
        t.set_decay(Decay { seconds: 2.0, low: 0.5, high: 0.25, crossover_low: 300.0, crossover_high: 4000.0 });
        for (h, g) in t.halves.iter().zip(t.gains()) {
            let len: usize = h.lengths().iter().sum();
            let mid = g.elements.iter().map(|e| f64::from(*e)).product::<f64>();
            let expect = |seconds: f64| 10f64.powf(-3.0 * len as f64 / (SR * seconds));
            assert!((mid / expect(2.0) - 1.0).abs() < 1e-5);
            assert!((mid * f64::from(g.low) / expect(1.0) - 1.0).abs() < 1e-5);
            assert!((mid * f64::from(g.high) / expect(0.5) - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn a_held_tank_keeps_its_energy() {
        let mut t = Tank::new(SR);
        t.set_decay(decay(1.0));
        let energy = |t: &mut Tank, n: usize, hold: f32, x: f32| {
            (0..n)
                .map(|i| {
                    let input = if i < 4800 { x * ((i as f32) * 0.05).sin() } else { 0.0 };
                    let ([l, r], _) = t.process([input, input], hold, [0.0, 0.0]);
                    f64::from(l * l + r * r)
                })
                .sum::<f64>()
        };
        energy(&mut t, 9600, 0.0, 0.5);
        let first = energy(&mut t, 48_000, 1.0, 0.0);
        energy(&mut t, 48_000 * 8, 1.0, 0.0);
        let later = energy(&mut t, 48_000, 1.0, 0.0);
        // Nine seconds later, within 1 dB: the loop is lossless, the level
        // at the taps only wanders as the energy moves around the tank
        let db = 10.0 * (later / first).log10();
        assert!(db.abs() < 1.0, "{first} then {later}");
    }
}
