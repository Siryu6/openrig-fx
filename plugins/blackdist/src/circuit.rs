//! The circuit, solved at eight times the sample rate.
//!
//! Voltages are in volts with 1.0 at full scale; capacitors are
//! trapezoidal companions (`orx_dsp::circuit`).
//!
//! The gain stage is a non-inverting op-amp. Its feedback is the
//! distortion pot (100 kΩ, audio taper) with 100 pF across; two legs go
//! from the inverting input to the reference, 47 Ω + 2.2 µF and
//! 560 Ω + 4.7 µF. Given the op-amp's output `y`, the inverting input is
//! `α·y + β` (α and β from the companions). The op-amp itself is a
//! single pole: `dy/dt = ωt·(x − α·y − β)`, at most the slew rate,
//! integrated by the trapezoidal rule and held within the rails (where
//! the slope stops). The step solves in closed form: the linear
//! solution, or the slew-limited step when the linear one is steeper.
//!
//! The clipper: 1 kΩ from the op-amp into a node with two antiparallel
//! diodes to the reference and the filter branch, 1.5 kΩ plus the filter
//! pot (100 kΩ, audio taper) into 3.3 nF. One implicit equation for the
//! node, solved by `DiodePair::solve`; the output is the filter
//! capacitor's voltage, through the output buffer (unity, with its
//! coupling as a 3 Hz high pass).
//!
//! The diode selector (the OpenRig touch) swaps the pair: silicon (the
//! original), LEDs (a higher threshold), or none, where the node is a
//! plain divider and the op-amp's rails and slew rate do the clipping.

use crate::info::Voicing;
use orx_dsp::circuit::{Capacitor, Diode, DiodePair};
use orx_dsp::filter::onepole::DcBlocker;
use orx_dsp::math::{audio_taper, finite_or_zero};
use orx_dsp::oversample::{HalfbandDesign, Stage2x};

/// Oversampling factor of the circuit
pub const FACTOR: usize = 8;

/// Distortion pot, Ω, and the smallest resistance it leaves
pub const POT_DISTORTION: f64 = 100e3;
pub const R_DISTORTION_MIN: f64 = 1.0;
/// Capacitor across the feedback, F
pub const C_FEEDBACK: f64 = 100e-12;
/// The two legs to the reference: resistor, capacitor
pub const LEGS: [(f64, f64); 2] = [(47.0, 2.2e-6), (560.0, 4.7e-6)];
/// Op-amp: gain-bandwidth product (rad/s), slew rate (V/s), rails (V)
pub const OPAMP_GBW: f64 = core::f64::consts::TAU * 1e6;
pub const OPAMP_SLEW: f64 = 0.3e6;
pub const OPAMP_RAIL: f64 = 3.5;
/// Clipper: series resistor, filter resistor, filter pot, filter
/// capacitor
pub const R_CLIP: f64 = 1e3;
pub const R_FILTER: f64 = 1.5e3;
pub const POT_FILTER: f64 = 100e3;
pub const C_FILTER: f64 = 3.3e-9;
/// Corner of the output coupling, Hz
pub const COUPLING_HZ: f64 = 3.0;

/// Clipping diodes of a voicing, if any
pub fn diodes(voicing: Voicing) -> Option<DiodePair> {
    match voicing {
        Voicing::Silicon => Some(DiodePair::new(Diode::SILICON, 1, 1)),
        Voicing::Led => Some(DiodePair::new(Diode::LED, 1, 1)),
        Voicing::None => None,
    }
}

/// Feedback resistance at a Distortion position (0 to 1)
pub fn distortion_resistance(distortion: f32) -> f64 {
    (POT_DISTORTION * f64::from(audio_taper(distortion.clamp(0.0, 1.0)))).max(R_DISTORTION_MIN)
}

/// Filter branch resistance at a Tone position (0 to 1): the pot turns
/// the other way
pub fn filter_resistance(tone: f32) -> f64 {
    R_FILTER + POT_FILTER * f64::from(audio_taper(1.0 - tone.clamp(0.0, 1.0)))
}

/// A resistor in series with a capacitor, to the reference
#[derive(Debug, Clone, Copy)]
struct Leg {
    r: f64,
    /// Companion resistance of the capacitor
    r_cap: f64,
    /// Conductance of the whole leg
    g: f64,
    cap: Capacitor,
}

impl Leg {
    fn new(r: f64, c: f64, rate: f64) -> Self {
        let r_cap = 1.0 / Capacitor::conductance(c, rate);
        Self { r, r_cap, g: 1.0 / (r + r_cap), cap: Capacitor::default() }
    }

    /// The leg's equivalent source voltage
    #[inline]
    fn source(&self) -> f64 {
        self.cap.series_source(self.r_cap)
    }

    /// Takes the voltage `v` at its top, with `e` its source voltage
    #[inline]
    fn update(&mut self, v: f64, e: f64) {
        let i = self.g * (v - e);
        self.cap = Capacitor { v: v - i * self.r, i };
    }
}

/// The op-amp stage
#[derive(Debug, Clone, Copy)]
struct GainStage {
    /// Time step, s
    t: f64,
    g_feedback_cap: f64,
    /// Feedback resistor and capacitor in parallel
    g_feedback: f64,
    feedback_cap: Capacitor,
    legs: [Leg; 2],
    /// Op-amp output and its slope
    y: f64,
    d: f64,
}

impl GainStage {
    fn set_distortion(&mut self, r: f64) {
        self.g_feedback = 1.0 / r + self.g_feedback_cap;
    }

    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        // The inverting input as α·y + β: the feedback's current and the
        // legs' meet there
        let j = self.feedback_cap.source(self.g_feedback_cap);
        let e = self.legs.map(|l| l.source());
        let g_sum = self.g_feedback + self.legs[0].g + self.legs[1].g;
        let alpha = self.g_feedback / g_sum;
        let beta = (self.legs[0].g * e[0] + self.legs[1].g * e[1] - j) / g_sum;
        // Trapezoidal step of the op-amp, slew-limited, within the
        // rails
        let h = 0.5 * self.t;
        let carried = self.y + h * self.d;
        let linear = (carried + h * OPAMP_GBW * (x - beta)) / (1.0 + h * OPAMP_GBW * alpha);
        let mut d = (OPAMP_GBW * (x - alpha * linear - beta)).clamp(-OPAMP_SLEW, OPAMP_SLEW);
        let mut y = carried + h * d;
        if y.abs() > OPAMP_RAIL {
            y = y.clamp(-OPAMP_RAIL, OPAMP_RAIL);
            d = 0.0;
        }
        self.y = y;
        self.d = d;
        // The capacitors follow
        let v_minus = alpha * y + beta;
        self.feedback_cap.update(y - v_minus, self.g_feedback_cap, j);
        for (leg, e) in self.legs.iter_mut().zip(e) {
            leg.update(v_minus, e);
        }
        y
    }
}

/// The diodes to ground and the passive filter
#[derive(Debug, Clone, Copy)]
struct Clipper {
    /// None: the node is linear, the op-amp's rails do the clipping
    diodes: Option<DiodePair>,
    /// The filter branch, its resistor set by Tone
    filter: Leg,
    /// The node's voltage one sample before the last, for the guess
    v_before: f64,
    v: f64,
}

impl Clipper {
    fn set_tone(&mut self, r: f64) {
        self.filter.r = r;
        self.filter.g = 1.0 / (r + self.filter.r_cap);
    }

    #[inline]
    fn process(&mut self, y: f64) -> f64 {
        let e = self.filter.source();
        let g_clip = 1.0 / R_CLIP;
        let g = g_clip + self.filter.g;
        let i = g_clip * y + self.filter.g * e;
        let v = match &self.diodes {
            Some(d) => d.solve(g, i, 2.0 * self.v - self.v_before),
            None => i / g,
        };
        self.v_before = self.v;
        self.v = v;
        self.filter.update(v, e);
        self.filter.cap.v
    }
}

/// The whole pedal, run at the base rate
pub struct Circuit {
    up: [Stage2x; 3],
    down: [Stage2x; 3],
    gain: GainStage,
    clipper: Clipper,
    dc: DcBlocker,
}

impl Circuit {
    pub fn new(sample_rate: f64) -> Self {
        let rate = sample_rate * FACTOR as f64;
        let stages = [
            Stage2x::new(&HalfbandDesign::steep()),
            Stage2x::new(&HalfbandDesign::relaxed()),
            Stage2x::new(&HalfbandDesign::relaxed()),
        ];
        let g_feedback_cap = Capacitor::conductance(C_FEEDBACK, rate);
        let mut circuit = Self {
            up: stages,
            down: stages,
            gain: GainStage {
                t: 1.0 / rate,
                g_feedback_cap,
                g_feedback: 0.0,
                feedback_cap: Capacitor::default(),
                legs: LEGS.map(|(r, c)| Leg::new(r, c, rate)),
                y: 0.0,
                d: 0.0,
            },
            clipper: Clipper {
                diodes: diodes(Voicing::Silicon),
                filter: Leg::new(R_FILTER, C_FILTER, rate),
                v_before: 0.0,
                v: 0.0,
            },
            dc: DcBlocker::new(sample_rate, COUPLING_HZ),
        };
        circuit.set_distortion(0.5);
        circuit.set_tone(0.5);
        circuit
    }

    /// Capacitors discharged, filters at rest
    pub fn reset(&mut self) {
        for s in self.up.iter_mut().chain(self.down.iter_mut()) {
            s.reset();
        }
        self.gain.feedback_cap = Capacitor::default();
        for leg in &mut self.gain.legs {
            leg.cap = Capacitor::default();
        }
        self.gain.y = 0.0;
        self.gain.d = 0.0;
        self.clipper.filter.cap = Capacitor::default();
        self.clipper.v = 0.0;
        self.clipper.v_before = 0.0;
        self.dc.reset();
    }

    pub fn set_voicing(&mut self, voicing: Voicing) {
        self.clipper.diodes = diodes(voicing);
    }

    /// Distortion control, 0 to 1
    pub fn set_distortion(&mut self, distortion: f32) {
        self.gain.set_distortion(distortion_resistance(distortion));
    }

    /// Tone control, 0 to 1
    pub fn set_tone(&mut self, tone: f32) {
        self.clipper.set_tone(filter_resistance(tone));
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        // A non-finite input would poison the solver's state
        let [a, b] = self.up[0].upsample(finite_or_zero(x));
        let mut half = [0.0f32; 2];
        for (h, u) in half.iter_mut().zip([a, b]) {
            let [c, d] = self.up[1].upsample(u);
            let mut quarter = [0.0f32; 2];
            for (q, w) in quarter.iter_mut().zip([c, d]) {
                let pair = self.up[2].upsample(w);
                let out = pair.map(|v| self.clipper.process(self.gain.process(f64::from(v))) as f32);
                *q = self.down[2].downsample(out);
            }
            *h = self.down[1].downsample(quarter);
        }
        self.dc.process(self.down[0].downsample(half))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pots_span_their_range() {
        assert_eq!(distortion_resistance(0.0), R_DISTORTION_MIN);
        assert!((distortion_resistance(1.0) - POT_DISTORTION).abs() < 1.0);
        assert!((filter_resistance(1.0) - R_FILTER).abs() < 1e-9);
        assert!((filter_resistance(0.0) - R_FILTER - POT_FILTER).abs() < 1.0);
    }

    #[test]
    fn the_op_amp_slews_then_holds_at_the_rails() {
        let mut gain = Circuit::new(48_000.0).gain;
        gain.set_distortion(distortion_resistance(1.0));
        // A step far beyond what the gain allows: the output ramps at the
        // slew rate, then stops at the rail
        let step = OPAMP_SLEW * gain.t;
        let mut last = 0.0;
        let mut reached = None;
        for n in 0..1000 {
            let y = gain.process(0.5);
            assert!(y - last <= step * (1.0 + 1e-9) && y <= OPAMP_RAIL, "{y} after {last}");
            if y == OPAMP_RAIL && reached.is_none() {
                reached = Some(n);
            }
            last = y;
        }
        assert_eq!(last, OPAMP_RAIL);
        // At the slew rate all the way (the first step takes half, as
        // the trapezoidal slope starts from rest)
        let n = reached.unwrap() as f64;
        assert!((OPAMP_RAIL / step - n).abs() <= 1.0, "{n}");
    }
}
