//! Oversampling with polyphase IIR half-band filters.
//!
//! Each 2× stage is a half-band elliptic low pass built from two chains of
//! first-order all-pass sections in z² (the design of L. de Soras' HIIR
//! library, after Valenzuela & Constantinides, "Digital signal processing
//! schemes for efficient interpolation and decimation", IEE Proc. 1983).
//! It costs one multiply per coefficient per output sample, and is
//! minimum-phase-like: a few samples of latency at low frequency, no
//! pre-ringing. 4× and 8× cascade 2× stages; later stages are cheaper
//! because the useful band is already small relative to their rate.
//!
//! Use for non-linear stages (saturation, waveshaping) whose harmonics
//! would otherwise fold back below Nyquist.

use crate::alloc;

/// Most coefficients a stage may have
pub const MAX_COEFS: usize = 16;

/// Coefficients of a half-band filter
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HalfbandDesign {
    coefs: [f64; MAX_COEFS],
    len: usize,
}

impl HalfbandDesign {
    /// Design `n` coefficients (1..=16) for a transition band of
    /// `transition` (fraction of the sample rate, 0 < t < 0.5: the pass
    /// band ends at 0.25 − t and the stop band starts at 0.25 + t).
    pub fn new(n: usize, transition: f64) -> Self {
        let n = n.clamp(1, MAX_COEFS);
        let transition = if transition.is_finite() { transition.clamp(1e-4, 0.49) } else { 0.05 };
        let (k, q) = transition_param(transition);
        let order = 2 * n + 1;
        let mut coefs = [0.0; MAX_COEFS];
        for (i, c) in coefs.iter_mut().take(n).enumerate() {
            *c = coef(i, k, q, order);
        }
        Self { coefs, len: n }
    }

    /// Stop band attenuation of the design, dB (positive)
    pub fn attenuation_db(n: usize, transition: f64) -> f64 {
        let (_, q) = transition_param(transition.clamp(1e-4, 0.49));
        let order = (2 * n.clamp(1, MAX_COEFS) + 1) as f64;
        // |H| in the stop band ≈ 4·q^(order/2) for these elliptic designs
        -20.0 * (4.0 * q.powf(order / 2.0)).log10()
    }

    pub fn coefs(&self) -> &[f64] {
        &self.coefs[..self.len]
    }

    /// Steep stage for the first 2× (≈ 100 dB, pass band to 0.2·fs of the
    /// high rate: 19.2 kHz at 48 kHz base rate)
    pub fn steep() -> Self {
        Self::new(12, 0.0436)
    }

    /// Relaxed stage for later 2× stages (useful band already ≤ fs/8)
    pub fn relaxed() -> Self {
        Self::new(6, 0.13)
    }
}

/// Elliptic modulus parameters (k, q) for a transition band
fn transition_param(transition: f64) -> (f64, f64) {
    let mut k = ((1.0 - transition * 2.0) * core::f64::consts::PI / 4.0).tan();
    k *= k;
    let kksqrt = (1.0 - k * k).powf(0.25);
    let e = 0.5 * (1.0 - kksqrt) / (1.0 + kksqrt);
    let e4 = e * e * e * e;
    let q = e * (1.0 + e4 * (2.0 + e4 * (15.0 + 150.0 * e4)));
    (k, q)
}

fn coef(index: usize, k: f64, q: f64, order: usize) -> f64 {
    let c = (index + 1) as f64;
    let order = order as f64;
    let pi = core::f64::consts::PI;
    // Numerator series: Σ (-1)^i q^(i(i+1)) sin((2i+1)cπ/order)
    let mut num = 0.0;
    let mut sign = 1.0;
    for i in 0..64 {
        let fi = i as f64;
        let term = q.powf(fi * (fi + 1.0)) * ((2.0 * fi + 1.0) * c * pi / order).sin() * sign;
        num += term;
        sign = -sign;
        if term.abs() < 1e-100 {
            break;
        }
    }
    // Denominator series: Σ_{i≥1} (-1)^i q^(i²) cos(2icπ/order)
    let mut den = 0.0;
    let mut sign = -1.0;
    for i in 1..64 {
        let fi = i as f64;
        let term = q.powf(fi * fi) * (2.0 * fi * c * pi / order).cos() * sign;
        den += term;
        sign = -sign;
        if term.abs() < 1e-100 {
            break;
        }
    }
    let num = num * q.powf(0.25);
    let den = den + 0.5;
    let ww = num / den;
    let wwsq = ww * ww;
    let x = ((1.0 - wwsq * k) * (1.0 - wwsq / k)).sqrt() / (1.0 + wwsq);
    (1.0 - x) / (1.0 + x)
}

/// State of the two all-pass chains of a stage
#[derive(Debug, Clone, Copy)]
struct Chains {
    coefs: [f32; MAX_COEFS],
    len: usize,
    x: [f32; MAX_COEFS],
    y: [f32; MAX_COEFS],
}

impl Chains {
    fn new(design: &HalfbandDesign) -> Self {
        let mut coefs = [0.0; MAX_COEFS];
        for (c, d) in coefs.iter_mut().zip(design.coefs()) {
            *c = *d as f32;
        }
        Self { coefs, len: design.len, x: [0.0; MAX_COEFS], y: [0.0; MAX_COEFS] }
    }

    fn reset(&mut self) {
        self.x = [0.0; MAX_COEFS];
        self.y = [0.0; MAX_COEFS];
    }

    /// Even coefficients filter `a`, odd ones `b`
    #[inline]
    fn run(&mut self, mut a: f32, mut b: f32) -> (f32, f32) {
        let len = self.len.min(MAX_COEFS);
        let mut i = 0;
        while i < len {
            let c = self.coefs[i];
            let t = (a - self.y[i]) * c + self.x[i];
            self.x[i] = a;
            self.y[i] = t;
            a = t;
            if i + 1 < len {
                let c = self.coefs[i + 1];
                let t = (b - self.y[i + 1]) * c + self.x[i + 1];
                self.x[i + 1] = b;
                self.y[i + 1] = t;
                b = t;
            }
            i += 2;
        }
        (a, b)
    }
}

/// One 2× up/down stage
#[derive(Debug, Clone, Copy)]
pub struct Stage2x {
    up: Chains,
    down: Chains,
}

impl Stage2x {
    pub fn new(design: &HalfbandDesign) -> Self {
        Self { up: Chains::new(design), down: Chains::new(design) }
    }

    pub fn reset(&mut self) {
        self.up.reset();
        self.down.reset();
    }

    /// One input sample → two output samples at twice the rate
    #[inline]
    pub fn upsample(&mut self, x: f32) -> [f32; 2] {
        let (a, b) = self.up.run(x, x);
        [a, b]
    }

    /// Two samples at the high rate → one output sample
    #[inline]
    pub fn downsample(&mut self, x: [f32; 2]) -> f32 {
        let (a, b) = self.down.run(x[1], x[0]);
        0.5 * (a + b)
    }
}

/// Oversampler by 2^stages (1 to 3 stages: 2×, 4×, 8×)
pub struct Oversampler {
    stages: [Stage2x; 3],
    count: usize,
    /// Scratch buffers at each rate (max block × 2, × 4, × 8)
    buf: [Box<[f32]>; 3],
    max_block: usize,
}

impl Oversampler {
    /// `factor` is 2, 4 or 8 (others round to the nearest of those)
    pub fn new(factor: usize, max_block: usize) -> Self {
        let count = match factor {
            0..=2 => 1,
            3..=5 => 2,
            _ => 3,
        };
        let steep = HalfbandDesign::steep();
        let relaxed = HalfbandDesign::relaxed();
        let stages = [Stage2x::new(&steep), Stage2x::new(&relaxed), Stage2x::new(&relaxed)];
        let max_block = max_block.max(1);
        Self {
            stages,
            count,
            buf: [
                alloc::zeroed(max_block * 2),
                alloc::zeroed(if count > 1 { max_block * 4 } else { 0 }),
                alloc::zeroed(if count > 2 { max_block * 8 } else { 0 }),
            ],
            max_block,
        }
    }

    pub fn factor(&self) -> usize {
        1 << self.count
    }

    pub fn reset(&mut self) {
        for s in &mut self.stages {
            s.reset();
        }
    }

    /// Process `io` in place: upsample, run `f` on the oversampled block,
    /// downsample. Blocks longer than the max block are cut in pieces.
    pub fn process(&mut self, io: &mut [f32], mut f: impl FnMut(&mut [f32])) {
        for chunk in io.chunks_mut(self.max_block) {
            let n = chunk.len();
            // Up
            let [b2, b4, b8] = &mut self.buf;
            let [s0, s1, s2] = &mut self.stages;
            for (x, out) in chunk.iter().zip(b2.as_chunks_mut::<2>().0) {
                *out = s0.upsample(*x);
            }
            let top: &mut [f32] = match self.count {
                1 => &mut b2[..2 * n],
                2 => {
                    for (x, out) in b2[..2 * n].iter().zip(b4.as_chunks_mut::<2>().0) {
                        *out = s1.upsample(*x);
                    }
                    &mut b4[..4 * n]
                }
                _ => {
                    for (x, out) in b2[..2 * n].iter().zip(b4.as_chunks_mut::<2>().0) {
                        *out = s1.upsample(*x);
                    }
                    for (x, out) in b4[..4 * n].iter().zip(b8.as_chunks_mut::<2>().0) {
                        *out = s2.upsample(*x);
                    }
                    &mut b8[..8 * n]
                }
            };
            f(top);
            // Down
            if self.count > 2 {
                for (pair, out) in b8[..8 * n].as_chunks::<2>().0.iter().zip(b4.iter_mut()) {
                    *out = s2.downsample(*pair);
                }
            }
            if self.count > 1 {
                for (pair, out) in b4[..4 * n].as_chunks::<2>().0.iter().zip(b2.iter_mut()) {
                    *out = s1.downsample(*pair);
                }
            }
            for (pair, out) in b2[..2 * n].as_chunks::<2>().0.iter().zip(chunk.iter_mut()) {
                *out = s0.downsample(*pair);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::f64::consts::TAU;

    /// Level (dB) of the `freq` component of `x`, by correlation
    fn level_db(x: &[f32], freq: f64, fs: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, v) in x.iter().enumerate() {
            let w = TAU * freq * n as f64 / fs;
            re += *v as f64 * w.cos();
            im += *v as f64 * w.sin();
        }
        let amp = 2.0 * re.hypot(im) / x.len() as f64;
        20.0 * amp.max(1e-20).log10()
    }

    #[test]
    fn design_reaches_its_attenuation() {
        let d = HalfbandDesign::steep();
        assert!(d.coefs().iter().all(|c| *c > 0.0 && *c < 1.0));
        assert!(d.coefs().windows(2).all(|w| w[0] < w[1]), "{:?}", d.coefs());
        assert!(HalfbandDesign::attenuation_db(12, 0.0436) > 95.0);
    }

    /// A tone upsampled 2×: the tone passes, its image at fs − f is gone
    #[test]
    fn upsampling_rejects_images() {
        let fs = 48_000.0;
        for (freq, max_image) in [(1000.0, -95.0), (15_000.0, -95.0)] {
            let mut stage = Stage2x::new(&HalfbandDesign::steep());
            let mut hi = Vec::new();
            for n in 0..48_000 {
                let x = (TAU * freq * n as f64 / fs).sin() as f32;
                hi.extend_from_slice(&stage.upsample(x));
            }
            let tail = &hi[16_000..];
            let pass = level_db(tail, freq, 2.0 * fs);
            let image = level_db(tail, fs - freq, 2.0 * fs);
            assert!(pass.abs() < 0.01, "{freq}: pass {pass}");
            assert!(image < max_image, "{freq}: image {image}");
        }
    }

    /// A tone above the base Nyquist at the high rate is removed before
    /// decimation (no aliasing)
    #[test]
    fn downsampling_rejects_aliases() {
        let fs2 = 96_000.0;
        let mut stage = Stage2x::new(&HalfbandDesign::steep());
        let mut out = Vec::new();
        for n in 0..48_000 {
            let s = |k: usize| (TAU * 30_000.0 * k as f64 / fs2).sin() as f32;
            out.push(stage.downsample([s(2 * n), s(2 * n + 1)]));
        }
        // 30 kHz would alias to 18 kHz
        let alias = level_db(&out[8000..], 18_000.0, 48_000.0);
        assert!(alias < -95.0, "{alias}");
    }

    #[test]
    fn round_trip_is_transparent_in_the_pass_band() {
        for factor in [2, 4, 8] {
            let fs = 48_000.0;
            let mut os = Oversampler::new(factor, 64);
            assert_eq!(os.factor(), factor);
            let mut x: Vec<f32> = (0..24_000).map(|n| (TAU * 5000.0 * n as f64 / fs).sin() as f32).collect();
            for block in x.chunks_mut(100) {
                os.process(block, |_| {});
            }
            let level = level_db(&x[8000..], 5000.0, fs);
            assert!(level.abs() < 0.01, "{factor}×: {level}");
        }
    }
}
