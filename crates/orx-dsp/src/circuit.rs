//! Building blocks for circuit models: capacitors as trapezoidal
//! companions and diode pairs solved by a safeguarded Newton method.
//!
//! A circuit model writes the nodal equations of its schematic with each
//! capacitor replaced by its companion: a conductance `2C/T` in parallel
//! with a current source carried from the last sample. Linear networks
//! then solve in closed form; a diode pair leaves one monotonic implicit
//! equation, `g·v + I(v) = rhs`, for [`DiodePair::solve`]. The result is
//! the bilinear transform of the analog circuit, with the capacitors'
//! voltages and currents as state, so that changing a part (a pot) moves
//! the circuit on from where it is, as the real one does.

/// A diode model (Shockley): saturation current and emission voltage
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diode {
    /// Saturation current, A
    pub is: f64,
    /// Emission coefficient times the thermal voltage, V
    pub nvt: f64,
}

/// Thermal voltage at room temperature, V
pub const THERMAL_VOLTAGE: f64 = 25.85e-3;

impl Diode {
    /// 1N914 / 1N4148 small-signal silicon diode
    pub const SILICON: Diode = Diode { is: 2.52e-9, nvt: 1.752 * THERMAL_VOLTAGE };
    /// Red LED
    pub const LED: Diode = Diode { is: 2.0e-19, nvt: 1.8 * THERMAL_VOLTAGE };
}

/// Trapezoidal companion state of a capacitor: its voltage and current
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Capacitor {
    pub v: f64,
    pub i: f64,
}

impl Capacitor {
    /// Companion conductance of a capacitance at a sample rate, `2C/T`
    pub fn conductance(farads: f64, sample_rate: f64) -> f64 {
        2.0 * farads * sample_rate
    }

    /// Current source of the companion for conductance `g`: the
    /// capacitor's current at the next sample is `g·v − source`
    #[inline]
    pub fn source(&self, g: f64) -> f64 {
        g * self.v + self.i
    }

    /// Voltage of the equivalent source in series with the companion's
    /// resistance `r = 1/g` (Thévenin form, for a capacitor in series
    /// with a resistor)
    #[inline]
    pub fn series_source(&self, r: f64) -> f64 {
        self.v + self.i * r
    }

    /// Takes the new voltage, with `source` the value used to solve for it
    #[inline]
    pub fn update(&mut self, v: f64, g: f64, source: f64) {
        self.i = g * v - source;
        self.v = v;
    }
}

/// Antiparallel diodes: `forward` in series conduct with a positive
/// voltage, `reverse` in series with a negative one
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiodePair {
    is: f64,
    forward: i32,
    reverse: i32,
    /// 1 / (forward·reverse·nVt): one exponential gives both directions
    scale: f64,
    /// Emission voltages of each direction
    a: f64,
    b: f64,
}

impl DiodePair {
    pub fn new(diode: Diode, forward: u32, reverse: u32) -> Self {
        let (forward, reverse) = (forward.clamp(1, 8) as i32, reverse.clamp(1, 8) as i32);
        Self {
            is: diode.is,
            forward,
            reverse,
            scale: 1.0 / (f64::from(forward * reverse) * diode.nvt),
            a: f64::from(forward) * diode.nvt,
            b: f64::from(reverse) * diode.nvt,
        }
    }

    /// Small-signal conductance at 0 V
    pub fn rest_conductance(&self) -> f64 {
        self.is / self.a + self.is / self.b
    }

    /// Current at voltage `v`, and its derivative
    #[inline]
    pub fn eval(&self, v: f64) -> (f64, f64) {
        let q = (v * self.scale).exp();
        let (ea, eb) = (q.powi(self.reverse), q.powi(-self.forward));
        (self.is * (ea - eb), self.is * self.scale * (f64::from(self.reverse) * ea + f64::from(self.forward) * eb))
    }

    /// Solves `g·v + current(v) = rhs` (g > 0) from `guess` by Newton's
    /// method. The left side rises monotonically, so every evaluation
    /// narrows a bracket of the root, from 0 to the voltage at which the
    /// conductance alone would carry `rhs`; a step leaving it bisects
    /// instead. The first step from far off also tightens the bracket to
    /// the voltage at which the conducting diodes alone would carry `rhs`
    /// (where the exponentials stay finite). Converges to well below a
    /// microvolt, in about two evaluations from a guess extrapolated from
    /// the last samples at a few times the audio rate.
    #[inline]
    pub fn solve(&self, g: f64, rhs: f64, guess: f64) -> f64 {
        const TOLERANCE: f64 = 1e-6;
        const ITERATIONS: usize = 60;
        const LONG_STEP: f64 = 0.1;
        let (mut lo, mut hi) = if rhs >= 0.0 { (0.0, rhs / g) } else { (rhs / g, 0.0) };
        let mut tight = false;
        let mut v = guess.clamp(lo, hi);
        for _ in 0..ITERATIONS {
            let (i, di) = self.eval(v);
            let f = g * v + i - rhs;
            if f > 0.0 {
                hi = v;
            } else {
                lo = v;
            }
            let mut next = v - f / (g + di);
            // A step leaving the bracket, a long one (the diodes switching
            // on from far below) or diodes carrying far more than the
            // whole current (far above) first tightens it
            let far = (next - v).abs() > LONG_STEP || i.abs() > 2.0 * rhs.abs();
            if !tight && (far || !(lo..=hi).contains(&next)) {
                tight = true;
                if rhs >= 0.0 {
                    hi = hi.min(self.a * (rhs / self.is).ln_1p());
                } else {
                    lo = lo.max(-self.b * (-rhs / self.is).ln_1p());
                }
            }
            if !(lo..=hi).contains(&next) {
                next = 0.5 * (lo + hi);
            }
            if (next - v).abs() < TOLERANCE {
                return next;
            }
            v = next;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_solver_finds_the_root() {
        for diode in [Diode::SILICON, Diode::LED] {
            for pair in [DiodePair::new(diode, 1, 1), DiodePair::new(diode, 2, 1), DiodePair::new(diode, 1, 2)] {
                for g in [1e-6, 2e-5, 1e-3] {
                    for k in -40i32..=40 {
                        let rhs = 1e-6 * f64::from(k) * f64::from(k.abs());
                        for guess in [-1.0, 0.0, 0.3, 5.0] {
                            let v = pair.solve(g, rhs, guess);
                            // The voltage error: residual over slope
                            let (i, di) = pair.eval(v);
                            let error = (g * v + i - rhs) / (g + di);
                            assert!(error.abs() < 1e-9, "g {g} rhs {rhs} guess {guess}: {error} V");
                        }
                    }
                }
            }
        }
        assert_eq!(DiodePair::new(Diode::SILICON, 1, 1).solve(1e-5, 0.0, 0.4), 0.0);
    }

    #[test]
    fn diodes_follow_the_shockley_equation() {
        let d = Diode::SILICON;
        let (a, b) = (2.0 * d.nvt, d.nvt);
        for k in -20..=20 {
            let v = 0.05 * f64::from(k);
            let (i, di) = DiodePair::new(d, 2, 1).eval(v);
            let exact = d.is * ((v / a).exp() - (-v / b).exp());
            let slope = d.is * ((v / a).exp() / a + (-v / b).exp() / b);
            assert!((i - exact).abs() <= 1e-12 * exact.abs().max(1e-9), "{v}");
            assert!((di - slope).abs() <= 1e-12 * slope, "{v}");
        }
        assert!((DiodePair::new(d, 2, 1).rest_conductance() - (d.is / a + d.is / b)).abs() < 1e-20);
    }

    #[test]
    fn a_capacitor_integrates_its_current() {
        // 1 µF charged by 1 mA (already flowing) for 1 ms: 1 V
        let (sr, c) = (48_000.0, 1e-6);
        let g = Capacitor::conductance(c, sr);
        let mut cap = Capacitor { v: 0.0, i: 1e-3 };
        for _ in 0..48 {
            let j = cap.source(g);
            cap.update((1e-3 + j) / g, g, j);
        }
        assert!((cap.v - 1.0).abs() < 1e-9, "{}", cap.v);
    }
}
