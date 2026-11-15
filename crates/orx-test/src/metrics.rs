//! Signal measurements.

use std::f64::consts::TAU;

pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>() / x.len() as f64).sqrt() as f32
}

pub fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// Level in dB relative to 1 (−300 for silence)
pub fn db(level: f32) -> f32 {
    if level > 0.0 { 20.0 * level.log10() } else { -300.0 }
}

/// Largest absolute difference
pub fn max_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "lengths differ");
    a.iter().zip(b).fold(0.0f32, |m, (x, y)| m.max((x - y).abs()))
}

/// Energy of the difference relative to the energy of `reference`, dB
/// (the "null depth": −∞ for identical signals, clamped to −300)
pub fn null_db(reference: &[f32], other: &[f32]) -> f32 {
    assert_eq!(reference.len(), other.len(), "lengths differ");
    let e: f64 = reference.iter().map(|x| f64::from(*x).powi(2)).sum();
    let d: f64 = reference.iter().zip(other).map(|(x, y)| f64::from(x - y).powi(2)).sum();
    if d == 0.0 {
        -300.0
    } else if e == 0.0 {
        300.0
    } else {
        (10.0 * (d / e).log10()) as f32
    }
}

/// Amplitude of the `freq` component of `x` (single-bin DFT with a Hann
/// window, amplitude-corrected)
pub fn tone_amplitude(x: &[f32], freq: f64, sample_rate: f64) -> f32 {
    let n = x.len();
    if n < 2 {
        return 0.0;
    }
    let (mut re, mut im, mut wsum) = (0.0f64, 0.0f64, 0.0f64);
    for (i, v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / (n - 1) as f64).cos();
        let p = TAU * freq * i as f64 / sample_rate;
        re += w * f64::from(*v) * p.cos();
        im -= w * f64::from(*v) * p.sin();
        wsum += w;
    }
    (2.0 * (re * re + im * im).sqrt() / wsum) as f32
}
