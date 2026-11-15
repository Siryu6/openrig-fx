//! OR EQ and OR Graphic EQ: conformance, the response against the curves
//! a host draws, transparency, automation without zipper noise, golden
//! files, and the LV2 builds of the same code.
#![allow(clippy::expect_used)]

use orx_dsp::filter::Shape;
use orx_dsp::filter::biquad::Coeffs;
use orx_eq::{GRAPHIC, Graphic, PARAMETRIC, Parametric};
use orx_plugin::process::NoStreams;
use orx_plugin::runner::Runner;
use orx_plugin::view::{CurveShape, FilterShape, Value};
use orx_plugin::{Config, Plugin, PluginInfo};
use orx_test::signal;
use std::path::PathBuf;

orx_test::install_rt_check!();

const SR: f64 = 48_000.0;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn at(info: &PluginInfo, symbol: &str) -> usize {
    info.port_index(symbol).expect("port symbol")
}

fn runner<P: Plugin>(params: &[(&str, f32)]) -> Runner<P> {
    let mut r = Runner::<P>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(P::INFO, s), *v);
    }
    r.activate();
    r
}

/// Process `input` in blocks of `block`, calling `before` ahead of each
/// block with the index of its first sample
fn process<P: Plugin>(
    r: &mut Runner<P>,
    input: &[f32],
    block: usize,
    mut before: impl FnMut(&mut Runner<P>, usize),
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

fn render<P: Plugin>(input: &[f32], params: &[(&str, f32)], block: usize) -> Vec<f32> {
    process(&mut runner::<P>(params), input, block, |_, _| {})
}

fn guitar(len: usize) -> Vec<f32> {
    let a = signal::pluck(110.0, 0.6, 1.5, SR, len);
    let b = signal::pluck(164.8, 0.4, 1.2, SR, len);
    a.iter().zip(&b).map(|(x, y)| x + y).collect()
}

fn shape(s: FilterShape) -> Shape {
    match s {
        FilterShape::Lowpass => Shape::Lowpass,
        FilterShape::Highpass => Shape::Highpass,
        FilterShape::Bandpass => Shape::Bandpass,
        FilterShape::Notch => Shape::Notch,
        FilterShape::Allpass => Shape::Allpass,
        FilterShape::Peak => Shape::Peak,
        FilterShape::LowShelf => Shape::LowShelf,
        FilterShape::HighShelf => Shape::HighShelf,
    }
}

/// The response a host draws: the product of the cookbook responses of
/// the declared curves at the parameter values, times the output level, dB
fn drawn_db(info: &PluginInfo, params: &[(&str, f32)], freq: f64) -> f64 {
    let value = |v: Value| match v {
        Value::Fixed(x) => f64::from(x),
        Value::Port(s) => {
            let p = params.iter().find(|(k, _)| *k == s).map(|(_, x)| *x);
            f64::from(p.unwrap_or_else(|| info.ports[at(info, s)].as_param().expect("param").default))
        }
    };
    let view = info.view.as_ref().expect("EQ view");
    let mut db = value(Value::Port("level"));
    for c in view.curves {
        if c.enable.is_some_and(|e| value(Value::Port(e)) < 0.5) {
            continue;
        }
        let CurveShape::Fixed(s) = c.shape else { panic!("fixed shapes only") };
        let gain = c.gain.map_or(0.0, value);
        let q = c.q.map_or(core::f64::consts::FRAC_1_SQRT_2, value);
        db += Coeffs::design(shape(s), SR, value(c.frequency), q, gain).magnitude_db(freq, SR);
    }
    db
}

/// Magnitude of `h` at `freq`, dB
fn measured_db(h: &[f32], freq: f64) -> f64 {
    let w = 2.0 * core::f64::consts::PI * freq / SR;
    let (re, im) = h.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, x)| {
        let p = w * n as f64;
        (re + f64::from(*x) * p.cos(), im - f64::from(*x) * p.sin())
    });
    10.0 * (re * re + im * im).log10()
}

/// The impulse response of the plugin matches the drawn curve
fn check_response<P: Plugin>(params: &[(&str, f32)]) {
    let h = render::<P>(&signal::impulse(0, 1 << 15), params, 256);
    let mut f = 25.0;
    while f < 20_000.0 {
        let drawn = drawn_db(P::INFO, params, f);
        let measured = measured_db(&h, f);
        if drawn > -60.0 {
            assert!(
                (drawn - measured).abs() < 0.02,
                "{} {params:?} at {f:.0} Hz: drawn {drawn:.3} dB, measured {measured:.3} dB",
                P::INFO.name
            );
        }
        f *= 1.09;
    }
}

#[test]
fn conformance() {
    orx_test::conformance::check::<Parametric>();
    orx_test::conformance::check::<Graphic>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&PARAMETRIC, dir().join("ports-parametric.lock"));
    orx_test::ports_lock::check(&GRAPHIC, dir().join("ports-graphic.lock"));
}

#[test]
fn the_response_is_the_drawn_curve() {
    check_response::<Parametric>(&[]);
    for preset in PARAMETRIC.presets {
        check_response::<Parametric>(preset.values);
    }
    check_response::<Parametric>(&[
        ("low_cut_on", 1.0),
        ("low_cut", 250.0),
        ("low_freq", 300.0),
        ("low_gain", -12.0),
        ("low_mid_freq", 150.0),
        ("low_mid_gain", 15.0),
        ("low_mid_q", 6.0),
        ("mid_freq", 3000.0),
        ("mid_gain", -15.0),
        ("mid_q", 0.3),
        ("high_mid_freq", 9000.0),
        ("high_mid_gain", 8.0),
        ("high_mid_q", 8.0),
        ("high_freq", 1500.0),
        ("high_gain", 10.0),
        ("high_cut_on", 1.0),
        ("high_cut", 1500.0),
        ("level", -6.0),
    ]);
    for preset in GRAPHIC.presets {
        check_response::<Graphic>(preset.values);
    }
    let extremes = [
        ("g100", 15.0),
        ("g200", -15.0),
        ("g400", 15.0),
        ("g800", -15.0),
        ("g1600", 15.0),
        ("g3200", -15.0),
        ("g6400", 15.0),
    ];
    check_response::<Graphic>(&extremes);
}

#[test]
fn flat_is_transparent() {
    let input = guitar(24_000);
    assert_eq!(render::<Parametric>(&input, &[], 128), input);
    assert_eq!(render::<Graphic>(&input, &[], 128), input);
}

/// Every control of `P` swept to a new value every 960 samples
fn automate<P: Plugin>(r: &mut Runner<P>, start: usize) {
    if !start.is_multiple_of(960) {
        return;
    }
    let k = start / 960;
    let ports = P::INFO.ports.iter().enumerate().filter_map(|(i, p)| p.as_param().map(|par| (i, *par)));
    for (j, (i, p)) in ports.enumerate() {
        if P::INFO.ports[i].symbol == "enabled" {
            continue;
        }
        // A different fraction of the range per port and step
        let t = ((k * 7 + j * 3) % 11) as f32 / 10.0;
        r.set_param(i, p.min + (p.max - p.min) * t);
    }
}

#[test]
fn output_is_block_size_independent_under_automation() {
    let input = guitar(48_000);
    let reference = process(&mut runner::<Parametric>(&[]), &input, 960, automate);
    for block in [1, 32, 96, 480] {
        assert_eq!(process(&mut runner::<Parametric>(&[]), &input, block, automate), reference, "block {block}");
    }
    let reference = process(&mut runner::<Graphic>(&[]), &input, 960, automate);
    for block in [1, 64, 320] {
        assert_eq!(process(&mut runner::<Graphic>(&[]), &input, block, automate), reference, "block {block}");
    }
}

/// Largest step between samples of `x` after `from`, relative to the
/// largest step of a sine at `freq` of the peak level of `x`
fn roughness(x: &[f32], freq: f64, from: usize) -> f32 {
    let peak = x[from..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let step = x[from..].windows(2).fold(0.0f32, |m, w| m.max((w[1] - w[0]).abs()));
    step / (peak * (2.0 * core::f64::consts::PI * freq / SR) as f32)
}

#[test]
fn jumps_do_not_click() {
    // A tone at the band, one control thrown end to end at a time, at a
    // peak of the tone (100 ms is a whole number of periods and a quarter)
    type Case = (f64, &'static [(&'static str, f32)], (&'static str, f32));
    let cases: [Case; 6] = [
        (902.5, &[], ("mid_gain", 15.0)),
        (902.5, &[("mid_gain", 15.0)], ("mid_freq", 4000.0)),
        (902.5, &[("mid_gain", 15.0), ("mid_q", 0.3)], ("mid_q", 8.0)),
        (122.5, &[("low_cut", 400.0)], ("low_cut_on", 1.0)),
        (3002.5, &[("high_cut_on", 1.0), ("high_cut", 20_000.0)], ("high_cut", 1500.0)),
        (502.5, &[], ("level", 15.0)),
    ];
    for (freq, before, (symbol, value)) in cases {
        let input = signal::sine(freq, 0.25, SR, 24_000);
        let out = process(&mut runner::<Parametric>(before), &input, 64, |r, start| {
            if start == 4800 {
                r.set_param(at(&PARAMETRIC, symbol), value);
            }
        });
        let name = symbol;
        let rough = roughness(&out, freq, 2400);
        assert!(rough < 1.2, "{name}: {rough}");
    }
    let input = signal::sine(802.5, 0.25, SR, 24_000);
    let out = process(&mut runner::<Graphic>(&[]), &input, 64, |r, start| {
        if start == 4800 {
            r.set_param(at(&GRAPHIC, "g800"), -15.0);
        }
    });
    let rough = roughness(&out, 802.5, 2400);
    assert!(rough < 1.2, "graphic: {rough}");
}

#[test]
fn golden_presets() {
    let input = guitar(48_000 * 2);
    let parametric = PARAMETRIC.presets.iter().map(|p| (p.id, render::<Parametric>(&input, p.values, 128)));
    let graphic = GRAPHIC.presets.iter().map(|p| (p.id, render::<Graphic>(&input, p.values, 128)));
    for (id, out) in parametric.chain(graphic) {
        let path = dir().join("tests/golden").join(format!("{id}.wav"));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

fn check_lv2<P: Plugin>(params: &[(&str, f32)]) {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<P>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of P, described by P::INFO
    let mut p = unsafe { host.instantiate(&d.raw, P::INFO, true) }.expect("instance");
    for (s, v) in params {
        p.set(at(P::INFO, s), *v);
    }
    p.activate();
    let input = guitar(256 * 40);
    let mut out = Vec::new();
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        orx_test::rt::no_alloc(|| p.run(256));
        out.extend_from_slice(p.output(0));
    }
    assert_eq!(out, render::<P>(&input, params, 256));
}

#[test]
fn lv2_builds_match_the_runner() {
    check_lv2::<Parametric>(&[("low_cut_on", 1.0), ("mid_gain", 6.0), ("high_gain", -4.0), ("level", -3.0)]);
    check_lv2::<Graphic>(&[("g400", 4.0), ("g3200", -6.0)]);
}
