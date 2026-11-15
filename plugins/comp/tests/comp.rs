//! OR Comp: conformance, the static curve, attack and release times,
//! transparency, blend, golden files, and the LV2 build of the same code.
#![allow(clippy::expect_used)]

use orx_comp::{COMP, Comp, curve};
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
    COMP.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<Comp> {
    let mut r = Runner::<Comp>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

fn process(
    r: &mut Runner<Comp>,
    input: &[f32],
    block: usize,
    mut before: impl FnMut(&mut Runner<Comp>, usize),
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

fn level(db: f32, t: f64) -> Vec<f32> {
    vec![10f32.powf(db / 20.0); ms(t)]
}

fn guitar(len: usize) -> Vec<f32> {
    let a = signal::pluck(110.0, 0.3, 1.5, SR, len);
    let b = signal::pluck(164.8, 0.2, 1.2, SR, len);
    a.iter().zip(&b).map(|(x, y)| x + y).collect()
}

#[test]
fn conformance() {
    orx_test::conformance::check::<Comp>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&COMP, dir().join("ports.lock"));
}

#[test]
fn the_curve_is_continuous_with_its_slopes() {
    for knee in [0.0, 6.0, 24.0] {
        let mut prev = curve(-80.0, -30.0, 4.0, knee);
        for i in 1..=800 {
            let x = -80.0 + i as f32 * 0.1;
            let y = curve(x, -30.0, 4.0, knee);
            let slope = (y - prev) / 0.1;
            assert!((0.25 - 1e-3..=1.0 + 1e-3).contains(&slope), "knee {knee} at {x}: {slope}");
            prev = y;
        }
        assert_eq!(curve(-50.0, -30.0, 4.0, knee), -50.0);
        assert!((curve(-10.0, -30.0, 4.0, knee) + 25.0).abs() < 1e-4);
    }
}

#[test]
fn steady_levels_follow_the_static_curve() {
    // A constant input: the gain settles on the static curve exactly
    let params = [("threshold", -30.0), ("ratio", 4.0), ("knee", 6.0), ("level", 3.0)];
    for db in [-50.0f32, -32.0, -30.0, -28.0, -10.0, -1.0] {
        let input = level(db, 1000.0);
        let out = render(&input, &params, 128);
        let got = metrics::db(*out.last().expect("output"));
        let want = curve(db, -30.0, 4.0, 6.0) + 3.0;
        assert!((got - want).abs() < 0.01, "{db} dB in: {got} out, curve {want}");
    }
}

#[test]
fn attack_and_release_take_their_time() {
    // From −40 to −10 dB and back: 15 dB of reduction at ratio 2,
    // reached to 1 − 1/e after the attack, released to 1/e after the
    // release
    let (attack, release) = (10.0, 100.0);
    let input: Vec<f32> = [level(-40.0, 200.0), level(-10.0, 300.0), level(-40.0, 600.0)].concat();
    let mut r = runner(&[
        ("threshold", -40.0),
        ("ratio", 2.0),
        ("knee", 0.0),
        ("attack", attack as f32),
        ("release", release as f32),
    ]);
    let out = process(&mut r, &input, 64, |_, _| {});
    let reduction = |i: usize| metrics::db(input[i]) - metrics::db(out[i]);
    let up = ms(200.0);
    let r_attack = reduction(up + ms(attack));
    assert!((r_attack - 15.0 * (1.0 - (-1.0f32).exp())).abs() < 0.2, "{r_attack}");
    let down = ms(500.0);
    let r_release = reduction(down + ms(release));
    assert!((r_release - 15.0 * (-1.0f32).exp()).abs() < 0.2, "{r_release}");
    assert!(reduction(down - 1) > 14.99);
}

#[test]
fn below_the_threshold_is_bit_transparent() {
    let input = signal::sine(220.0, 0.01, SR, 24_000);
    assert_eq!(render(&input, &[("threshold", -20.0)], 128), input);
}

#[test]
fn blend_mixes_dry_and_compressed() {
    let input = guitar(48_000);
    let params = [("threshold", -40.0), ("ratio", 10.0), ("level", 10.0)];
    let wet = render(&input, &params, 128);
    let mut half = params.to_vec();
    half.push(("mix", 50.0));
    let blend = render(&input, &half, 128);
    for ((b, w), x) in blend.iter().zip(&wet).zip(&input) {
        assert!((b - 0.5 * (w + x)).abs() < 1e-6);
    }
}

#[test]
fn blend_and_level_changes_do_not_click() {
    // A jump at a sine peak (0.5 s is 500 periods and a quarter): the
    // largest step between samples stays that of the sine itself
    let input = signal::sine(1000.5, 0.5, SR, 48_000);
    let natural = input.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
    for (symbol, from, to) in [("mix", 0.0, 100.0), ("level", 0.0, 30.0)] {
        let mut r = runner(&[("threshold", -40.0), ("ratio", 20.0), ("knee", 0.0), (symbol, from)]);
        let out = process(&mut r, &input, 96, |r, start| {
            if start == 24_000 {
                r.set_param(at(symbol), to);
            }
        });
        let step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(step < 1.2 * natural, "{symbol}: {step} vs {natural}");
    }
}

#[test]
fn the_meter_reads_the_reduction() {
    let input = level(-10.0, 1000.0);
    let mut r = runner(&[("threshold", -30.0), ("ratio", 5.0), ("knee", 0.0)]);
    let _ = process(&mut r, &input, 128, |_, _| {});
    assert!((r.meter(at("reduction")) - 16.0).abs() < 0.01);
}

#[test]
fn sustain_evens_a_decaying_note() {
    // A note decaying by 20 dB over a second comes out within a few dB
    let input = signal::pluck(196.0, 0.5, 3.0, SR, 48_000);
    let out = render(&input, COMP.presets[0].values, 128);
    let window = |x: &[f32], t: f64| metrics::db(metrics::rms(&x[ms(t)..ms(t + 50.0)]));
    let span_in = window(&input, 50.0) - window(&input, 800.0);
    let span_out = window(&out, 50.0) - window(&out, 800.0);
    assert!(span_out < 0.4 * span_in, "{span_in} dB in, {span_out} out");
}

fn automate(r: &mut Runner<Comp>, start: usize) {
    if !start.is_multiple_of(960) {
        return;
    }
    let k = start / 960;
    for (j, symbol) in ["threshold", "ratio", "knee", "attack", "release", "level", "mix"].iter().enumerate() {
        let p = COMP.ports[at(symbol)].as_param().expect("param");
        let t = ((k * 7 + j * 3) % 11) as f32 / 10.0;
        r.set_param(at(symbol), p.min + (p.max - p.min) * t);
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

#[test]
fn golden_presets() {
    let input = guitar(48_000 * 2);
    for preset in COMP.presets {
        let out = render(&input, preset.values, 128);
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<Comp>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of Comp, described by COMP
    let mut p = unsafe { host.instantiate(&d.raw, &COMP, true) }.expect("instance");
    let params = [("threshold", -35.0), ("ratio", 6.0), ("level", 8.0), ("mix", 70.0)];
    for (s, v) in params {
        p.set(at(s), v);
    }
    p.activate();
    let input = guitar(256 * 60);
    let mut out = Vec::new();
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        orx_test::rt::no_alloc(|| p.run(256));
        out.extend_from_slice(p.output(0));
    }
    assert_eq!(out, render(&input, &params, 256));
}
