//! Deterministic test signals.

use orx_dsp::noise::Rng;
use std::f64::consts::TAU;

/// Sine of `freq` Hz, amplitude `amp`, `len` samples
pub fn sine(freq: f64, amp: f32, sample_rate: f64, len: usize) -> Vec<f32> {
    (0..len).map(|i| amp * (TAU * freq * i as f64 / sample_rate).sin() as f32).collect()
}

/// Exponential sweep from `f0` to `f1` Hz over `len` samples (Farina)
pub fn log_sweep(f0: f64, f1: f64, amp: f32, sample_rate: f64, len: usize) -> Vec<f32> {
    let t = len as f64 / sample_rate;
    let k = (f1 / f0).ln();
    (0..len)
        .map(|i| {
            let x = i as f64 / sample_rate;
            amp * (TAU * f0 * t / k * ((x / t * k).exp() - 1.0)).sin() as f32
        })
        .collect()
}

/// Unit impulse at `at`
pub fn impulse(at: usize, len: usize) -> Vec<f32> {
    (0..len).map(|i| if i == at { 1.0 } else { 0.0 }).collect()
}

/// White noise in [-amp, amp], reproducible from `seed`
pub fn noise(seed: u32, amp: f32, len: usize) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    (0..len).map(|_| amp * rng.bipolar()).collect()
}

/// Constant value
pub fn dc(value: f32, len: usize) -> Vec<f32> {
    vec![value; len]
}

/// A guitar-like plucked string (Karplus-Strong), `freq` Hz, decaying
/// over about `decay_s` seconds, peak near `amp`
pub fn pluck(freq: f64, amp: f32, decay_s: f64, sample_rate: f64, len: usize) -> Vec<f32> {
    let period = (sample_rate / freq).round().max(2.0) as usize;
    let mut rng = Rng::new(0x5eed);
    let mut ring: Vec<f32> = (0..period).map(|_| rng.bipolar()).collect();
    // Loss per period for a 60 dB decay in decay_s
    let loss = 10f64.powf(-3.0 * period as f64 / (decay_s * sample_rate)) as f32;
    let mut out = Vec::with_capacity(len);
    let mut prev = 0.0f32;
    for i in 0..len {
        let k = i % period;
        let x = ring[k];
        out.push(x);
        ring[k] = loss * 0.5 * (x + prev);
        prev = x;
    }
    let peak = out.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-9);
    out.iter_mut().for_each(|x| *x *= amp / peak);
    out
}

/// Sum of tones (frequency, amplitude)
pub fn chord(tones: &[(f64, f32)], sample_rate: f64, len: usize) -> Vec<f32> {
    let mut out = vec![0.0; len];
    for (f, a) in tones {
        for (o, x) in out.iter_mut().zip(sine(*f, *a, sample_rate, len)) {
            *o += x;
        }
    }
    out
}
