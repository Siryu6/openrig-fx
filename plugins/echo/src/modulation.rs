//! Movements of the read heads: wow, flutter and drift of a tape
//! transport, a slow sweep for the analog and digital voicings.
//!
//! The four components always run; each mode only sets their depths,
//! which glide (100 ms), so a change of mode or amount never makes a head
//! jump. Depths are delay offsets: a sine of depth `D` seconds at `f` Hz
//! bends the pitch by `2π·f·D`, so the tape's 1 ms of wow at 0.55 Hz is a
//! 0.35 % wow, its 40 µs of flutter at 6.3 Hz a 0.16 % flutter, at full
//! amount (heavy, worn-machine values).

use crate::info::Mode;
use orx_dsp::filter::onepole::OnePole;
use orx_dsp::lfo::Phasor;
use orx_dsp::math::sin_turns;
use orx_dsp::noise::Rng;
use orx_dsp::{Smoother, Smoothing};

const WOW_HZ: f64 = 0.55;
const FLUTTER_HZ: f64 = 6.3;
/// Drift: a new random level every so often, smoothed by two one-poles
const DRIFT_PERIOD_S: f64 = 0.4;
const DRIFT_HZ: f64 = 0.35;
const DRIFT_SEED: u32 = 0x5eed_0ec0;
/// Sweep rate of the analog and digital voicings
const SWEEP_HZ: [f64; 3] = [0.9, 0.0, 0.6];

/// Depths in seconds at full amount: wow, flutter, drift, sweep
const DEPTHS: [[f32; 4]; 3] = [
    // Digital: a gentle sweep only
    [0.0, 0.0, 0.0, 0.6e-3],
    // Tape
    [1.0e-3, 40e-6, 0.8e-3, 0.0],
    // Analog
    [0.0, 0.0, 0.0, 1.5e-3],
];

/// Largest offset any mode may ask for, seconds
pub(crate) const MAX_DEPTH_S: f64 = 2.0e-3;

/// Phase of the right channel's sweep after the left, turns
const SWEEP_SPREAD: f32 = 0.25;

#[derive(Debug, Clone)]
pub(crate) struct Modulation {
    sample_rate: f64,
    wow: Phasor,
    flutter: Phasor,
    sweep: Phasor,
    rng: Rng,
    drift_target: f32,
    drift_count: u32,
    drift_period: u32,
    drift: [OnePole; 2],
    /// Depths in samples
    depths: [Smoother; 4],
}

impl Modulation {
    pub(crate) fn new(sample_rate: f64) -> Self {
        let depth = Smoother::new(Smoothing::Linear { ms: 100.0 }, sample_rate as f32, 0.0);
        let drift = OnePole::new(sample_rate, DRIFT_HZ);
        Self {
            sample_rate,
            wow: Phasor::new(WOW_HZ, sample_rate),
            flutter: Phasor::new(FLUTTER_HZ, sample_rate),
            sweep: Phasor::new(SWEEP_HZ[0], sample_rate),
            rng: Rng::new(DRIFT_SEED),
            drift_target: 0.0,
            drift_count: 0,
            drift_period: (DRIFT_PERIOD_S * sample_rate) as u32,
            drift: [drift; 2],
            depths: [depth.clone(), depth.clone(), depth.clone(), depth],
        }
    }

    pub(crate) fn reset(&mut self) {
        // Wow and sweep start at their centre crossing, flutter too
        self.wow.set_phase(0.0);
        self.flutter.set_phase(0.0);
        self.sweep.set_phase(0.0);
        self.rng = Rng::new(DRIFT_SEED);
        self.drift_target = 0.0;
        self.drift_count = 0;
        for f in self.drift.iter_mut() {
            f.reset();
        }
    }

    /// Depths for `mode` at `amount` (0 to 1); `snap` skips the glide
    pub(crate) fn set(&mut self, mode: Mode, amount: f32, snap: bool) {
        let m = mode as usize;
        let samples = self.sample_rate as f32;
        for (s, d) in self.depths.iter_mut().zip(DEPTHS[m]) {
            let target = d * amount.clamp(0.0, 1.0) * samples;
            if snap {
                s.reset(target);
            } else {
                s.set_target(target);
            }
        }
        if SWEEP_HZ[m] > 0.0 {
            self.sweep.set_rate(SWEEP_HZ[m], self.sample_rate);
        }
    }

    /// Head offsets of the next sample, samples, for the left (or only)
    /// and the right channel
    #[inline]
    pub(crate) fn next(&mut self) -> [f32; 2] {
        let [wow, flutter, drift, sweep] =
            [self.depths[0].next(), self.depths[1].next(), self.depths[2].next(), self.depths[3].next()];
        let pw = self.wow.next() as f32;
        let pf = self.flutter.next() as f32;
        let ps = self.sweep.next() as f32;
        if self.drift_count == 0 {
            self.drift_target = self.rng.bipolar();
            self.drift_count = self.drift_period.max(1);
        }
        self.drift_count -= 1;
        let first = self.drift[0].lowpass(self.drift_target);
        let d = self.drift[1].lowpass(first);
        let transport = wow * sin_turns(pw) + flutter * sin_turns(pf) + drift * d;
        [transport + sweep * sin_turns(ps), transport + sweep * sin_turns(ps + SWEEP_SPREAD)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_stay_within_the_largest_depth() {
        let sr = 48_000.0;
        for mode in Mode::ALL {
            let mut m = Modulation::new(sr);
            m.set(mode, 1.0, true);
            let mut largest = 0.0f32;
            for _ in 0..(sr as usize * 20) {
                let [l, r] = m.next();
                largest = largest.max(l.abs()).max(r.abs());
            }
            let limit = (MAX_DEPTH_S * sr) as f32;
            assert!(largest > 0.2 * limit && largest <= limit, "{mode:?}: {largest} of {limit}");
        }
    }

    #[test]
    fn zero_amount_is_still() {
        let mut m = Modulation::new(48_000.0);
        m.set(Mode::Tape, 0.0, true);
        assert!((0..10_000).all(|_| m.next() == [0.0, 0.0]));
    }
}
