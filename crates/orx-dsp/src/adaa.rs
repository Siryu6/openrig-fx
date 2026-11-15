//! Antiderivative anti-aliasing (ADAA) of memoryless nonlinearities.
//!
//! A waveshaper `f` applied sample by sample creates harmonics above
//! Nyquist that fold back as inharmonic aliases. First-order ADAA
//! (Parker, Zavalishin and Le Bivic, DAFx 2016; Bilbao, Esqueda, Parker
//! and Välimäki, IEEE SPL 2017) replaces `f(x[n])` with the mean of `f`
//! over the straight line from `x[n−1]` to `x[n]`:
//!
//! ```text
//! y[n] = (F(x[n]) − F(x[n−1])) / (x[n] − x[n−1]),   F' = f
//! ```
//!
//! which is a continuous-time rectangular filter applied to `f` before
//! sampling: aliases drop by roughly 6 dB per octave below Nyquist more
//! than the naive shaper, at the cost of a half-sample delay and a gentle
//! high-frequency roll-off (the same mean applied to `f(x) = x` is
//! `(x[n] + x[n−1]) / 2`). When the two samples are too close the
//! quotient is ill-conditioned and `f` at the midpoint is used, the limit
//! of the same mean.
//!
//! `F` is evaluated in `f64`: the quotient of two nearby values of `F`
//! loses the digits `f32` would not have.

/// A nonlinearity with a closed-form antiderivative
pub trait Curve {
    /// The curve
    fn eval(&self, x: f64) -> f64;
    /// An antiderivative of [`Curve::eval`] (any constant)
    fn integral(&self, x: f64) -> f64;
}

/// `tanh x`, the smooth symmetric saturation of tape and transistor
/// stages; `F = ln cosh x`
#[derive(Debug, Clone, Copy, Default)]
pub struct Tanh;

impl Curve for Tanh {
    #[inline]
    fn eval(&self, x: f64) -> f64 {
        x.tanh()
    }

    #[inline]
    fn integral(&self, x: f64) -> f64 {
        // ln cosh x = |x| + ln(1 + e^−2|x|) − ln 2, without overflow
        let a = x.abs();
        a + (-2.0 * a).exp().ln_1p() - core::f64::consts::LN_2
    }
}

/// `x / (1 + |x|)`: a softer knee and a slower approach to ±1 than
/// `tanh`, the compressed character of bucket-brigade echoes;
/// `F = |x| − ln(1 + |x|)`
#[derive(Debug, Clone, Copy, Default)]
pub struct Rational;

impl Curve for Rational {
    #[inline]
    fn eval(&self, x: f64) -> f64 {
        x / (1.0 + x.abs())
    }

    #[inline]
    fn integral(&self, x: f64) -> f64 {
        let a = x.abs();
        a - a.ln_1p()
    }
}

/// `1.5x − 0.5x³` up to ±1, then ±1: a hard knee reached with a
/// continuous slope, the clipping of diodes to ground after a high-gain
/// stage; `F = 0.75x² − 0.125x⁴` inside, `|x| − 0.375` outside
#[derive(Debug, Clone, Copy, Default)]
pub struct Cubic;

impl Curve for Cubic {
    #[inline]
    fn eval(&self, x: f64) -> f64 {
        if x.abs() < 1.0 { x * (1.5 - 0.5 * x * x) } else { x.signum() }
    }

    #[inline]
    fn integral(&self, x: f64) -> f64 {
        let a = x.abs();
        if a < 1.0 {
            let x2 = x * x;
            x2 * (0.75 - 0.125 * x2)
        } else {
            a - 0.375
        }
    }
}

/// Below this input step the midpoint value replaces the quotient
const EPSILON: f64 = 1e-6;

/// First-order ADAA state of one channel
#[derive(Debug, Clone, Copy, Default)]
pub struct Adaa1 {
    x1: f64,
    f1: f64,
}

impl Adaa1 {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Start from `x` as the previous input of `curve`, so the next
    /// output does not ramp from 0 (after switching curves)
    pub fn prime(&mut self, curve: &impl Curve, x: f32) {
        self.x1 = f64::from(x);
        self.f1 = curve.integral(self.x1);
    }

    /// The anti-aliased value of `curve` for input `x` (half a sample
    /// late)
    #[inline]
    pub fn process(&mut self, curve: &impl Curve, x: f32) -> f32 {
        let x = f64::from(x);
        let f = curve.integral(x);
        let dx = x - self.x1;
        let y = if dx.abs() > EPSILON { (f - self.f1) / dx } else { curve.eval(0.5 * (x + self.x1)) };
        // A non-finite input must not stick: the next sample starts over
        if f.is_finite() {
            self.x1 = x;
            self.f1 = f;
        } else {
            self.reset();
        }
        y as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_integral(c: &impl Curve) {
        // F' = f by central differences
        for i in -80..=80 {
            let x = f64::from(i) * 0.1 + 0.013;
            let h = 1e-5;
            let d = (c.integral(x + h) - c.integral(x - h)) / (2.0 * h);
            assert!((d - c.eval(x)).abs() < 1e-6, "x {x}: {d} vs {}", c.eval(x));
        }
    }

    #[test]
    fn integrals_are_antiderivatives() {
        check_integral(&Tanh);
        check_integral(&Rational);
        check_integral(&Cubic);
    }

    /// Amplitude of the `freq` component of `x` (single DFT bin)
    fn tone(x: &[f32], freq: f64, rate: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, v) in x.iter().enumerate() {
            let p = core::f64::consts::TAU * freq * n as f64 / rate;
            re += f64::from(*v) * p.cos();
            im += f64::from(*v) * p.sin();
        }
        2.0 * (re * re + im * im).sqrt() / x.len() as f64
    }

    #[test]
    fn aliases_drop() {
        // 4.8 kHz at 48 kHz, driven hard: odd harmonics fold back onto
        // multiples of 2.4 kHz that are not harmonics; whole cycles only
        let rate = 48_000.0;
        let f0 = 4_700.0;
        let len = 48_000;
        let input: Vec<f32> =
            (0..len).map(|n| (6.0 * (core::f64::consts::TAU * f0 * n as f64 / rate).sin()) as f32).collect();
        for curve in [&Tanh as &dyn DynCurve, &Rational, &Cubic] {
            let naive: Vec<f32> = input.iter().map(|x| curve.eval_dyn(f64::from(*x)) as f32).collect();
            let mut a = Adaa1::default();
            let adaa: Vec<f32> = input.iter().map(|x| curve.process_dyn(&mut a, *x)).collect();
            // 5th harmonic 23.5 kHz stays; 7th (32.9 kHz) folds to 15.1 kHz,
            // 9th (42.3 kHz) to 5.7 kHz, 11th (51.7 kHz) to 3.7 kHz
            for alias in [15_100.0, 5_700.0, 3_700.0] {
                let (n, a) = (tone(&naive, alias, rate), tone(&adaa, alias, rate));
                let gain = 20.0 * (a / n).log10();
                assert!(gain < -6.0, "alias at {alias} Hz: {gain:.1} dB");
            }
            // The fundamental keeps its level (half-sample mean: cos(π·f/fs))
            let (n, a) = (tone(&naive, f0, rate), tone(&adaa, f0, rate));
            assert!((a / n - 1.0).abs() < 0.1, "{n} {a}");
        }
    }

    trait DynCurve {
        fn eval_dyn(&self, x: f64) -> f64;
        fn process_dyn(&self, a: &mut Adaa1, x: f32) -> f32;
    }

    impl<C: Curve> DynCurve for C {
        fn eval_dyn(&self, x: f64) -> f64 {
            self.eval(x)
        }
        fn process_dyn(&self, a: &mut Adaa1, x: f32) -> f32 {
            a.process(self, x)
        }
    }

    #[test]
    fn close_samples_use_the_midpoint() {
        let mut a = Adaa1::default();
        a.prime(&Tanh, 0.5);
        let y = a.process(&Tanh, 0.5);
        assert!((f64::from(y) - 0.5f64.tanh()).abs() < 1e-7);
    }

    #[test]
    fn non_finite_input_does_not_stick() {
        let mut a = Adaa1::default();
        let _ = a.process(&Tanh, f32::NAN);
        let _ = a.process(&Tanh, f32::INFINITY);
        let _ = a.process(&Tanh, 0.3);
        let y = a.process(&Tanh, 0.3);
        assert!((f64::from(y) - 0.3f64.tanh()).abs() < 1e-6);
    }
}
