//! The circuit, solved at four times the sample rate.
//!
//! Voltages are in volts with 1.0 at full scale. Every capacitor is a
//! trapezoidal companion (a conductance `2C/T` and a current source),
//! which keeps its voltage and current as state: the discretisation is
//! the bilinear transform of the analog circuit, and a control that
//! changes a resistor or a capacitor moves the circuit on from where its
//! capacitors are, as the real one does.
//!
//! The clipping stage is a non-inverting op-amp. Its inverting input
//! follows the input; the gain leg (4.7 kΩ and 47 nF to the reference)
//! draws a current that the feedback must supply: the drive resistance
//! (51 kΩ plus a 500 kΩ audio-taper pot), 51 pF and two antiparallel
//! silicon diodes, all in parallel. The voltage across the feedback
//! solves one implicit equation per sample, by Newton's method kept
//! inside a bracket of the root (the equation is monotonic, so the
//! bracket always holds it); the output is the input plus that voltage.
//!
//! The tone stage: 1 kΩ and 220 nF to the reference (a fixed 723 Hz low
//! pass), then a second op-amp, non-inverting, with 1 kΩ to its positive
//! input and 1 kΩ of feedback. A 20 kΩ tone pot spans its two inputs;
//! its wiper goes to the reference through 220 Ω and 220 nF. Towards the
//! positive input, the branch shunts the signal's treble (a cut); towards
//! the negative input, it shunts the feedback's (a treble boost, up to
//! 1 + 1 kΩ / 220 Ω). Linear: it solves in closed form.
//!
//! The input and output buffers are emitter followers: unity gain. Their
//! coupling capacitors are a high pass of a few hertz, kept as the output
//! DC blocker (the asymmetric voicing has a DC component).
//!
//! The OpenRig touch, Tight: before the clipping stage, two coupling
//! sections (22 nF into a resistor to the reference, one before and one
//! after the input buffer) whose resistors are the two gangs of one pot.
//! A 12 dB/octave high pass from 5 Hz (out of the way of the lowest
//! tunings) to 700 Hz: it takes the low end out of the boost a high-gain
//! amp then clips, which is what tightens the palm mutes.

use crate::info::Voicing;
use orx_dsp::circuit::{Capacitor, Diode, DiodePair};
use orx_dsp::filter::onepole::DcBlocker;
use orx_dsp::math::finite_or_zero;
use orx_dsp::oversample::{HalfbandDesign, Stage2x};

/// Oversampling factor of the circuit
pub const FACTOR: usize = 4;

/// Gain leg resistor, Ω
pub const R_GAIN: f64 = 4.7e3;
/// Fixed drive resistor and drive pot, Ω
pub const R_DRIVE: f64 = 51e3;
pub const POT_DRIVE: f64 = 500e3;
/// Feedback capacitor of the clipping stage, F
pub const C_FEEDBACK: f64 = 51e-12;

/// Tone stage: fixed low pass (resistor, capacitor), resistor to the
/// positive input, pot, wiper branch (resistor, capacitor), feedback
pub const R_TONE_LP: f64 = 1e3;
pub const C_TONE_LP: f64 = 220e-9;
pub const R_TONE_IN: f64 = 1e3;
pub const POT_TONE: f64 = 20e3;
pub const R_TONE_WIPER: f64 = 220.0;
pub const C_TONE_WIPER: f64 = 220e-9;
pub const R_TONE_FEEDBACK: f64 = 1e3;

/// Corner of the output coupling, Hz
pub const COUPLING_HZ: f64 = 3.0;

/// Tight sections: capacitor, F, and the corner range of the pot, Hz
pub const C_TIGHT: f64 = 22e-9;
pub const TIGHT_MIN_HZ: f32 = 5.0;
pub const TIGHT_MAX_HZ: f32 = 700.0;

/// Resistance of a tight section for its corner frequency
pub fn tight_resistance(hz: f32) -> f64 {
    1.0 / (core::f64::consts::TAU * f64::from(hz.clamp(TIGHT_MIN_HZ, TIGHT_MAX_HZ)) * C_TIGHT)
}

/// Values that change with the voicing
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Values {
    /// Gain leg capacitor, F
    pub c_gain: f64,
    /// Diodes conducting with the output above the input: in series
    pub forward: u32,
    /// Diodes conducting the other way
    pub reverse: u32,
}

impl Values {
    pub fn of(voicing: Voicing) -> Self {
        match voicing {
            Voicing::Classic => Self { c_gain: 47e-9, forward: 1, reverse: 1 },
            Voicing::Fat => Self { c_gain: 100e-9, forward: 1, reverse: 1 },
            Voicing::Asymmetric => Self { c_gain: 47e-9, forward: 2, reverse: 1 },
        }
    }
}

/// Drive pot resistance at a control position (0 to 1), plus the fixed
/// resistor
pub fn drive_resistance(drive: f32) -> f64 {
    R_DRIVE + POT_DRIVE * f64::from(orx_dsp::math::audio_taper(drive.clamp(0.0, 1.0)))
}

/// One coupling section: the capacitor in series, the resistor to the
/// reference, unloaded (the buffer after it draws nothing)
#[derive(Debug, Clone, Copy)]
struct Coupling {
    g_cap: f64,
    r: f64,
    cap: Capacitor,
}

impl Coupling {
    #[inline]
    fn process(&mut self, vin: f64) -> f64 {
        // i = g·(vin − v_out) − j and v_out = r·i
        let j = self.cap.source(self.g_cap);
        let i = (self.g_cap * vin - j) / (1.0 + self.g_cap * self.r);
        let out = self.r * i;
        self.cap.update(vin - out, self.g_cap, j);
        out
    }
}

/// The op-amp with diodes in its feedback
#[derive(Debug, Clone, Copy)]
struct ClipStage {
    /// Companion conductance of the feedback capacitor
    g_feedback_cap: f64,
    /// Companion resistance of the gain leg capacitor, and the
    /// conductance of the whole leg
    r_gain_cap: f64,
    g_leg: f64,
    /// Drive resistance and feedback capacitor in parallel
    g_feedback: f64,
    diodes: DiodePair,
    gain_cap: Capacitor,
    feedback_cap: Capacitor,
    /// The feedback voltage one sample before the last, for the guess
    v_before: f64,
}

impl ClipStage {
    fn set_gain_cap(&mut self, g: f64) {
        self.r_gain_cap = 1.0 / g;
        self.g_leg = 1.0 / (R_GAIN + self.r_gain_cap);
    }

    fn set_drive(&mut self, r: f64) {
        self.g_feedback = 1.0 / r + self.g_feedback_cap;
    }

    #[inline]
    fn process(&mut self, vin: f64) -> f64 {
        // Gain leg: the resistor in series with the capacitor's companion
        let e_leg = self.gain_cap.series_source(self.r_gain_cap);
        let i_leg = self.g_leg * (vin - e_leg);
        // Feedback: what the gain leg draws flows through the resistor,
        // the capacitor and the diodes. The guess extrapolates the last
        // two samples.
        let j = self.feedback_cap.source(self.g_feedback_cap);
        let last = self.feedback_cap.v;
        let v = self.diodes.solve(self.g_feedback, i_leg + j, 2.0 * last - self.v_before);
        self.v_before = last;
        self.gain_cap = Capacitor { v: vin - i_leg * R_GAIN, i: i_leg };
        self.feedback_cap.update(v, self.g_feedback_cap, j);
        vin + v
    }
}

/// The tone network and its op-amp
#[derive(Debug, Clone, Copy)]
struct ToneStage {
    g_lp_cap: f64,
    /// Companion resistance of the wiper capacitor
    r_wiper_cap: f64,
    /// The wiper branch: resistor in series with the companion
    g_wiper: f64,
    /// Coefficients of the closed-form solution for a tone setting
    k: f64,
    m: f64,
    r_a: f64,
    r_p: f64,
    u: f64,
    out: f64,
    lp_cap: Capacitor,
    wiper_cap: Capacitor,
}

impl ToneStage {
    fn new(g_lp_cap: f64, g_wiper_cap: f64) -> Self {
        let r_wiper_cap = 1.0 / g_wiper_cap;
        let mut t = Self {
            g_lp_cap,
            r_wiper_cap,
            g_wiper: 1.0 / (R_TONE_WIPER + r_wiper_cap),
            k: 0.0,
            m: 0.0,
            r_a: 0.0,
            r_p: 0.0,
            u: 0.0,
            out: 0.0,
            lp_cap: Capacitor::default(),
            wiper_cap: Capacitor::default(),
        };
        t.set(0.5);
        t
    }

    /// Both op-amp inputs sit at `vp`; let `u = vp − wiper`. The wiper
    /// node gives `u = g9·(vp − e6) / s` (s: the three conductances at
    /// the wiper), the positive input `g8·(va − vp) = g_plus·u`, and the
    /// node of the fixed low pass the rest; the feedback resistor carries
    /// the current of the pot's negative side.
    fn set(&mut self, tone: f32) {
        let t = f64::from(tone.clamp(0.0, 1.0));
        // A wiper at an end still has the track's contact resistance
        let g_plus = 1.0 / (t * POT_TONE).max(1.0);
        let g_minus = 1.0 / ((1.0 - t) * POT_TONE).max(1.0);
        let (g7, g8, g9) = (1.0 / R_TONE_LP, 1.0 / R_TONE_IN, self.g_wiper);
        let s = g_plus + g_minus + g9;
        self.k = g_plus * g9 / s;
        self.m = g8 * self.k / (g8 + self.k);
        self.r_a = 1.0 / (g7 + self.g_lp_cap + self.m);
        self.r_p = 1.0 / (g8 + self.k);
        self.u = g9 / s;
        self.out = R_TONE_FEEDBACK * g_minus;
    }

    #[inline]
    fn process(&mut self, vin: f64) -> f64 {
        let j5 = self.lp_cap.source(self.g_lp_cap);
        let e6 = self.wiper_cap.series_source(self.r_wiper_cap);
        let va = (vin / R_TONE_LP + j5 + self.m * e6) * self.r_a;
        let vp = (va / R_TONE_IN + self.k * e6) * self.r_p;
        let u = self.u * (vp - e6);
        let wiper = vp - u;
        let i6 = self.g_wiper * (wiper - e6);
        self.lp_cap.update(va, self.g_lp_cap, j5);
        self.wiper_cap = Capacitor { v: wiper - i6 * R_TONE_WIPER, i: i6 };
        vp + self.out * u
    }
}

/// The whole pedal, run at the base rate
pub struct Circuit {
    /// 2 / T at the oversampled rate: a capacitor C has the companion
    /// conductance 2C/T
    two_over_t: f64,
    up: [Stage2x; 2],
    down: [Stage2x; 2],
    tight: [Coupling; 2],
    clip: ClipStage,
    tone: ToneStage,
    dc: DcBlocker,
}

impl Circuit {
    pub fn new(sample_rate: f64) -> Self {
        let rate = sample_rate * FACTOR as f64;
        let stages = [Stage2x::new(&HalfbandDesign::steep()), Stage2x::new(&HalfbandDesign::relaxed())];
        let companion = |c: f64| 2.0 * c * rate;
        let mut circuit = Self {
            two_over_t: 2.0 * rate,
            up: stages,
            down: stages,
            tight: [Coupling { g_cap: companion(C_TIGHT), r: 1.0, cap: Capacitor::default() }; 2],
            clip: ClipStage {
                g_feedback_cap: companion(C_FEEDBACK),
                r_gain_cap: 1.0,
                g_leg: 0.0,
                g_feedback: 0.0,
                diodes: DiodePair::new(Diode::SILICON, 1, 1),
                gain_cap: Capacitor::default(),
                feedback_cap: Capacitor::default(),
                v_before: 0.0,
            },
            tone: ToneStage::new(companion(C_TONE_LP), companion(C_TONE_WIPER)),
            dc: DcBlocker::new(sample_rate, COUPLING_HZ),
        };
        circuit.set_voicing(Voicing::Classic);
        circuit.set_drive(0.5);
        circuit.set_tone(0.5);
        circuit.set_tight(TIGHT_MIN_HZ);
        circuit
    }

    /// Capacitors discharged, filters at rest
    pub fn reset(&mut self) {
        for s in self.up.iter_mut().chain(self.down.iter_mut()) {
            s.reset();
        }
        for c in &mut self.tight {
            c.cap = Capacitor::default();
        }
        self.clip.gain_cap = Capacitor::default();
        self.clip.feedback_cap = Capacitor::default();
        self.clip.v_before = 0.0;
        self.tone.lp_cap = Capacitor::default();
        self.tone.wiper_cap = Capacitor::default();
        self.dc.reset();
    }

    pub fn set_voicing(&mut self, voicing: Voicing) {
        let v = Values::of(voicing);
        self.clip.set_gain_cap(v.c_gain * self.two_over_t);
        self.clip.diodes = DiodePair::new(Diode::SILICON, v.forward, v.reverse);
    }

    /// Drive control, 0 to 1
    pub fn set_drive(&mut self, drive: f32) {
        self.clip.set_drive(drive_resistance(drive));
    }

    /// Tight control: the corner of each section, Hz
    pub fn set_tight(&mut self, hz: f32) {
        let r = tight_resistance(hz);
        for c in &mut self.tight {
            c.r = r;
        }
    }

    /// Tone control, 0 to 1
    pub fn set_tone(&mut self, tone: f32) {
        self.tone.set(tone);
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        // A non-finite input would poison the solver's state
        let [a, b] = self.up[0].upsample(finite_or_zero(x));
        let mut half = [0.0f32; 2];
        for (h, u) in half.iter_mut().zip([a, b]) {
            let quad = self.up[1].upsample(u);
            let out = quad.map(|v| {
                let v = self.tight.iter_mut().fold(f64::from(v), |v, c| c.process(v));
                self.tone.process(self.clip.process(v)) as f32
            });
            *h = self.down[1].downsample(out);
        }
        self.dc.process(self.down[0].downsample(half))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_spans_the_pot() {
        assert_eq!(drive_resistance(0.0), R_DRIVE);
        assert!((drive_resistance(1.0) - R_DRIVE - POT_DRIVE).abs() < 1.0);
    }

    #[test]
    fn a_coupling_section_is_a_first_order_high_pass() {
        // At the corner, a sine settles at 1/√2 of its amplitude
        let rate = 192_000.0;
        let hz = 200.0;
        let mut c = Coupling {
            g_cap: Capacitor::conductance(C_TIGHT, rate),
            r: tight_resistance(hz),
            cap: Capacitor::default(),
        };
        let w = core::f64::consts::TAU * f64::from(hz) / rate;
        let peak = (0..2 * rate as usize)
            .map(|n| c.process((w * n as f64).sin()))
            .skip(rate as usize)
            .fold(0.0f64, |m, y| m.max(y.abs()));
        assert!((peak - core::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3, "{peak}");
    }
}
