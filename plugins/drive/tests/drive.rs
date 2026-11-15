//! OR Drive: conformance, levels across modes, aliasing, the behaviour of
//! each circuit, click-free changes, golden files, and the LV2 build of
//! the same code.
#![allow(clippy::expect_used)]

use orx_drive::{DRIVE, Drive, Mode};
use orx_dsp::filter::Shape;
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_plugin::Config;
use orx_plugin::process::NoStreams;
use orx_plugin::runner::Runner;
use orx_test::{metrics, signal};
use std::f64::consts::TAU;
use std::path::PathBuf;

orx_test::install_rt_check!();

const SR: f64 = 48_000.0;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn at(symbol: &str) -> usize {
    DRIVE.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<Drive> {
    let mut r = Runner::<Drive>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

/// Process `input` in blocks of `block`, calling `before` ahead of each
/// block with the index of its first sample
fn process(
    r: &mut Runner<Drive>,
    input: &[f32],
    block: usize,
    mut before: impl FnMut(&mut Runner<Drive>, usize),
) -> Vec<f32> {
    let mut out = vec![0.0; input.len()];
    let mut start = 0;
    while start < input.len() {
        let n = block.min(input.len() - start);
        before(r, start);
        r.process(n, &[&input[start..start + n]], &mut [&mut out[start..start + n]], &mut NoStreams);
        start += n;
    }
    out
}

fn render(input: &[f32], params: &[(&str, f32)], block: usize) -> Vec<f32> {
    process(&mut runner(params), input, block, |_, _| {})
}

/// A guitar at a typical interface level (peaks around −10 dBFS)
fn guitar(len: usize) -> Vec<f32> {
    let a = signal::pluck(110.0, 0.6, 1.5, SR, len);
    let b = signal::pluck(164.8, 0.4, 1.2, SR, len);
    a.iter().zip(&b).map(|(x, y)| 0.3 * (x + y)).collect()
}

fn mode(m: Mode) -> (&'static str, f32) {
    ("mode", m as i32 as f32)
}

/// Amplitude of each harmonic of `f0` (whole hertz) below Nyquist in one
/// second of `x` (whole periods, so the bins are exact), and the power of
/// what is neither a harmonic nor DC, relative to the total, dB
fn spectrum(x: &[f32], f0: u32) -> (Vec<f64>, f64) {
    assert_eq!(x.len(), SR as usize);
    let n = x.len() as f64;
    let mean = x.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let total: f64 = x.iter().map(|v| (f64::from(*v) - mean).powi(2)).sum::<f64>() / n;
    let mut amps = Vec::new();
    let mut k = 1;
    while f64::from(k * f0) < SR / 2.0 {
        let w = TAU * f64::from(k * f0) / SR;
        let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (i, v)| {
            let p = w * i as f64;
            (re + f64::from(*v) * p.cos(), im + f64::from(*v) * p.sin())
        });
        amps.push(2.0 * (re * re + im * im).sqrt() / n);
        k += 1;
    }
    let harmonic: f64 = amps.iter().map(|a| a * a / 2.0).sum();
    let rest = (total - harmonic).max(1e-30);
    (amps, 10.0 * (rest / total).log10())
}

/// Harmonic distortion of a sine at `f0`: power of the harmonics above
/// the fundamental relative to it, dB
fn thd(out: &[f32], f0: u32) -> f64 {
    let (amps, _) = spectrum(out, f0);
    let above: f64 = amps[1..].iter().map(|a| a * a).sum();
    10.0 * (above / (amps[0] * amps[0])).log10()
}

/// One second of `params` on a sine at `f0`, after a second to settle
fn steady(f0: u32, amp: f32, params: &[(&str, f32)]) -> Vec<f32> {
    let tone = signal::sine(f64::from(f0), amp, SR, 96_000);
    render(&tone, params, 128)[48_000..].to_vec()
}

#[test]
fn conformance() {
    orx_test::conformance::check::<Drive>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&DRIVE, dir().join("ports.lock"));
}

#[test]
fn levels_match_across_modes() {
    let input = guitar(48_000 * 2);
    let rms = metrics::rms(&input);
    for m in Mode::ALL {
        let gain = metrics::db(metrics::rms(&render(&input, &[mode(m)], 128)) / rms);
        // The boost adds its Drive (+10 dB at the middle), the others
        // sit at the bypassed level
        let expected = if m == Mode::Boost { 10.0 } else { 0.0 };
        assert!((gain - expected).abs() < 1.5, "{m:?}: {gain:.2} dB");
    }
}

#[test]
fn aliases_stay_low() {
    // 1979 Hz at full drive and the brightest tone: square-like waves
    // whose harmonics fold back between the real ones (−17 dB for the
    // distortion's curve applied plainly at the sample rate)
    let limits = [(Mode::Boost, -100.0), (Mode::Overdrive, -55.0), (Mode::Distortion, -50.0), (Mode::Fuzz, -45.0)];
    for (m, limit) in limits {
        let out = steady(1979, 0.3, &[mode(m), ("drive", 100.0), ("tone", 100.0)]);
        let (_, aliases) = spectrum(&out, 1979);
        assert!(aliases < limit, "{m:?}: {aliases:.1} dB");
    }
}

#[test]
fn drive_adds_harmonics() {
    for m in Mode::ALL {
        let thds: Vec<f64> =
            [0.0, 50.0, 100.0].iter().map(|d| thd(&steady(220, 0.3, &[mode(m), ("drive", *d)]), 220)).collect();
        assert!(thds[0] < thds[1] && thds[1] < thds[2], "{m:?}: {thds:?}");
    }
    // A boost at unity stays clean
    let clean = thd(&steady(220, 0.1, &[mode(Mode::Boost), ("drive", 0.0)]), 220);
    assert!(clean < -60.0, "{clean}");
}

#[test]
fn only_the_fuzz_is_asymmetric() {
    for m in Mode::ALL {
        let out = steady(220, 0.3, &[mode(m)]);
        let (amps, _) = spectrum(&out, 220);
        let second = 20.0 * (amps[1] / amps[0]).log10();
        if m == Mode::Fuzz {
            // Even harmonics, 40 dB above the symmetric circuits'
            assert!(second > -45.0, "{m:?}: {second:.1} dB");
        } else {
            assert!(second < -80.0, "{m:?}: {second:.1} dB");
        }
    }
}

#[test]
fn tone_brightens() {
    let input = guitar(48_000);
    // Level above 2 kHz against the level below 500 Hz
    let band = |out: &[f32], shape: Shape, freq: f64| {
        let mut f = Svf::new(SvfCoeffs::new(shape, SR, freq, 0.707, 0.0));
        let band: Vec<f32> = out.iter().map(|x| f.process(*x)).collect();
        metrics::rms(&band)
    };
    let bright = |out: &[f32]| metrics::db(band(out, Shape::Highpass, 2000.0) / band(out, Shape::Lowpass, 500.0));
    for m in Mode::ALL {
        let b: Vec<f32> =
            [0.0, 50.0, 100.0].iter().map(|t| bright(&render(&input, &[mode(m), ("tone", *t)], 128))).collect();
        assert!(b[0] < b[1] && b[1] < b[2] && b[2] - b[0] > 10.0, "{m:?}: {b:?}");
    }
}

#[test]
fn silence_stays_silent() {
    let zeros = vec![0.0f32; 24_000];
    let out = process(&mut runner(&[mode(Mode::Fuzz), ("bass", 6.0)]), &zeros, 64, |r, start| {
        if start == 6400 {
            r.set_param(at("mode"), 2.0);
        }
        if start == 12_800 {
            r.set_param(at("mode"), 0.0);
        }
    });
    assert!(out.iter().all(|y| *y == 0.0));
}

/// Every control moved every 960 samples, the mode every third time
fn automate(r: &mut Runner<Drive>, start: usize) {
    if !start.is_multiple_of(960) {
        return;
    }
    let k = start / 960;
    for (j, symbol) in ["drive", "tone", "bass", "level"].iter().enumerate() {
        let p = DRIVE.ports[at(symbol)].as_param().expect("param");
        let t = ((k * 7 + j * 3) % 11) as f32 / 10.0;
        r.set_param(at(symbol), p.min + (p.max - p.min) * t);
    }
    if k.is_multiple_of(3) {
        r.set_param(at("mode"), (k / 3 % 4) as f32);
    }
}

#[test]
fn output_is_block_size_independent_under_automation() {
    let input = guitar(48_000);
    let reference = process(&mut runner(&[]), &input, 960, automate);
    for block in [1, 32, 96, 480] {
        assert_eq!(process(&mut runner(&[]), &input, block, automate), reference, "block {block}");
    }
}

/// Largest step between samples of `x` after `from`, relative to the
/// largest step of a sine at `freq` of the peak level of `x`
fn roughness(x: &[f32], freq: f64, from: usize) -> f32 {
    let peak = x[from..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let step = x[from..].windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    step / (peak * (TAU * freq / SR) as f32)
}

#[test]
fn changes_do_not_click() {
    // A quiet note through the nearly linear settings, a control thrown
    // from one end to the other at a peak of the note (200 ms is a whole
    // number of periods and a quarter): the output stays a smooth sine
    let freq = 221.25;
    type Case = (f32, &'static [(&'static str, f32)], (&'static str, f32));
    let cases: [Case; 4] = [
        (0.05, &[("mode", 0.0), ("drive", 0.0)], ("mode", 1.0)),
        (0.02, &[("mode", 0.0), ("drive", 0.0)], ("drive", 100.0)),
        (0.05, &[("mode", 0.0), ("drive", 0.0), ("level", -24.0)], ("level", 12.0)),
        (0.05, &[("mode", 0.0), ("drive", 0.0), ("tone", 0.0)], ("tone", 100.0)),
    ];
    for (amp, before, (symbol, value)) in cases {
        let input = signal::sine(freq, amp, SR, 24_000);
        let out = process(&mut runner(before), &input, 64, |r, start| {
            if start == 9600 {
                r.set_param(at(symbol), value);
            }
        });
        let rough = roughness(&out, freq, 4800);
        assert!(rough < 1.2, "{symbol} to {value}: {rough}");
    }
}

#[test]
fn golden_presets() {
    let mut input = guitar(48_000 * 2);
    input[72_000..].fill(0.0);
    for preset in DRIVE.presets {
        let out = render(&input, preset.values, 128);
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<Drive>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of Drive, described by DRIVE
    let mut p = unsafe { host.instantiate(&d.raw, &DRIVE, true) }.expect("instance");
    let params = [("mode", 2.0), ("drive", 70.0), ("tone", 40.0), ("bass", -3.0)];
    for (s, v) in params {
        p.set(at(s), v);
    }
    p.activate();
    let input = guitar(256 * 40);
    let mut out = Vec::new();
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        orx_test::rt::no_alloc(|| p.run(256));
        out.extend_from_slice(p.output(0));
    }
    assert_eq!(out, render(&input, &params, 256));
}
