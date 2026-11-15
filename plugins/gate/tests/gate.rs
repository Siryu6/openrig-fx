//! OR Gate: conformance, transparency while open, hysteresis, hold and
//! decay timing, range, golden files, and the LV2 build of the same code.
#![allow(clippy::expect_used)]

use orx_gate::{GATE, Gate};
use orx_plugin::Config;
use orx_plugin::process::NoStreams;
use orx_plugin::runner::Runner;
use orx_test::{metrics, signal};
use std::path::PathBuf;

orx_test::install_rt_check!();

const SR: f64 = 48_000.0;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn at(symbol: &str) -> usize {
    GATE.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<Gate> {
    let mut r = Runner::<Gate>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

fn process(
    r: &mut Runner<Gate>,
    input: &[f32],
    block: usize,
    mut before: impl FnMut(&mut Runner<Gate>, usize),
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

fn ms(t: f64) -> usize {
    (t * 0.001 * SR) as usize
}

/// A constant level per span of milliseconds: the output over the input
/// is the gate's gain, sample by sample
fn steps(spans: &[(f64, f32)]) -> Vec<f32> {
    spans.iter().flat_map(|(t, level)| vec![*level; ms(*t)]).collect()
}

fn gains(input: &[f32], out: &[f32]) -> Vec<f32> {
    input.iter().zip(out).map(|(x, y)| y / x).collect()
}

/// A guitar note over a noise floor at −70 dBFS
fn noisy_guitar(len: usize) -> Vec<f32> {
    let note = signal::pluck(110.0, 0.3, 0.4, SR, len);
    let hiss = signal::noise(7, 0.0003, len);
    note.iter().zip(&hiss).map(|(a, b)| a + b).collect()
}

#[test]
fn conformance() {
    orx_test::conformance::check::<Gate>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&GATE, dir().join("ports.lock"));
}

#[test]
fn open_is_bit_transparent() {
    let input = signal::sine(220.0, 0.1, SR, 24_000);
    let out = render(&input, &[], 128);
    assert_eq!(out[ms(2.0)..], input[ms(2.0)..]);
}

#[test]
fn opens_in_a_millisecond_without_a_jump() {
    let input = steps(&[(50.0, 1e-5), (50.0, 0.1)]);
    let g = gains(&input, &render(&input, &[], 64));
    let onset = ms(50.0);
    assert!(g[onset - 1] < 1e-3);
    // Linear rise: 1 ms from closed to open, steps of 1/48
    assert!(g[onset + ms(0.5)] > 0.4 && g[onset + ms(0.5)] < 0.6, "{}", g[onset + ms(0.5)]);
    assert_eq!(g[onset + ms(1.0) + 1], 1.0);
    assert!(g[onset..].windows(2).all(|w| w[1] - w[0] <= 1.0 / 47.0));
}

#[test]
fn hysteresis_keeps_a_fading_note_open() {
    // Threshold −40 dB: −43 dB keeps it open once open, but does not open
    // it from closed
    let between = 10f32.powf(-43.0 / 20.0);
    let input = steps(&[(50.0, 0.1), (300.0, between)]);
    let g = gains(&input, &render(&input, &[("threshold", -40.0)], 64));
    assert!(g[ms(50.0)..].iter().all(|g| *g == 1.0));
    let input = steps(&[(300.0, between)]);
    let g = gains(&input, &render(&input, &[("threshold", -40.0)], 64));
    assert!(g.iter().all(|g| *g < 1e-3));
}

#[test]
fn holds_then_decays_over_the_range() {
    let (hold, decay) = (40.0f32, 200.0f32);
    let input = steps(&[(100.0, 0.1), (800.0, 1e-4)]);
    let out = render(&input, &[("threshold", -50.0), ("hold", hold), ("decay", decay)], 64);
    let g = gains(&input, &out);
    let drop = ms(100.0);
    // The detector falls below the closing level (−56 dB from 0.1, 10 ms
    // release) after about 72 ms; the gain is 1 until then plus the hold
    let below = drop + ms(10.0 * (0.1f64 / 10f64.powf(-56.0 / 20.0)).ln());
    assert!(g[drop..below + ms(f64::from(hold)) - 1].iter().all(|g| *g == 1.0));
    // Then −90 dB after the decay, falling in a straight line in dB
    let end = below + ms(f64::from(hold)) + ms(f64::from(decay));
    let db = |i: usize| metrics::db(g[i]);
    assert!((db(end - ms(f64::from(decay) / 2.0)) + 45.0).abs() < 1.0, "{}", db(end - ms(f64::from(decay) / 2.0)));
    assert!((db(end + ms(2.0)) + 90.0).abs() < 0.1, "{}", db(end + ms(2.0)));
    assert!(g[end + ms(2.0)..].iter().all(|v| (metrics::db(*v) + 90.0).abs() < 0.1));
}

#[test]
fn range_sets_the_floor_and_the_meter_reads_it() {
    let input = steps(&[(50.0, 0.1), (600.0, 1e-4)]);
    let mut r = runner(&[("range", -18.0), ("decay", 100.0), ("hold", 0.0)]);
    let out = process(&mut r, &input, 64, |_, _| {});
    let g = gains(&input, &out);
    assert!((metrics::db(*g.last().expect("output")) + 18.0).abs() < 0.01);
    assert!((r.meter(at("reduction")) - 18.0).abs() < 0.01);
}

#[test]
fn removes_the_noise_between_notes() {
    let input = noisy_guitar(48_000 * 2);
    let out = render(&input, &[("threshold", -55.0), ("decay", 100.0)], 128);
    // The note passes untouched at its start; the hiss after it is gone
    assert_eq!(out[ms(5.0)..ms(200.0)], input[ms(5.0)..ms(200.0)]);
    let tail = ms(1800.0)..ms(2000.0);
    assert!(metrics::rms(&out[tail.clone()]) < 1e-3 * metrics::rms(&input[tail]));
}

/// Controls moved every 960 samples
fn automate(r: &mut Runner<Gate>, start: usize) {
    if !start.is_multiple_of(960) {
        return;
    }
    let k = start / 960;
    for (j, symbol) in ["threshold", "decay", "hold", "range"].iter().enumerate() {
        let p = GATE.ports[at(symbol)].as_param().expect("param");
        let t = ((k * 7 + j * 3) % 11) as f32 / 10.0;
        r.set_param(at(symbol), p.min + (p.max - p.min) * t);
    }
}

#[test]
fn output_is_block_size_independent_under_automation() {
    let input = noisy_guitar(48_000);
    let reference = process(&mut runner(&[]), &input, 960, automate);
    for block in [1, 32, 96, 480] {
        assert_eq!(process(&mut runner(&[]), &input, block, automate), reference, "block {block}");
    }
}

#[test]
fn golden_presets() {
    let input = noisy_guitar(48_000 * 2);
    for preset in GATE.presets {
        let out = render(&input, preset.values, 128);
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<Gate>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of Gate, described by GATE
    let mut p = unsafe { host.instantiate(&d.raw, &GATE, true) }.expect("instance");
    let params = [("threshold", -50.0), ("decay", 80.0), ("hold", 10.0)];
    for (s, v) in params {
        p.set(at(s), v);
    }
    p.activate();
    let input = noisy_guitar(256 * 200);
    let mut out = Vec::new();
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        orx_test::rt::no_alloc(|| p.run(256));
        out.extend_from_slice(p.output(0));
    }
    assert_eq!(out, render(&input, &params, 256));
}
