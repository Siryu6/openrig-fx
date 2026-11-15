//! OR Plate: conformance, decay times, pre-delay, freeze, stereo image,
//! trails, golden files, and the LV2 build of the same code.
#![allow(clippy::expect_used)]

use orx_dsp::filter::Shape;
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_plate::{PLATE, Plate};
use orx_plugin::Config;
use orx_plugin::Plugin;
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
    Plate::INFO.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<Plate> {
    let mut r = Runner::<Plate>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

/// Process `inputs` (left, right) in blocks of `block`, calling `before`
/// ahead of each block with its index
fn process(
    r: &mut Runner<Plate>,
    inputs: &[Vec<f32>; 2],
    block: usize,
    mut before: impl FnMut(&mut Runner<Plate>, usize),
) -> [Vec<f32>; 2] {
    let len = inputs[0].len();
    let mut outs = [vec![0.0; len], vec![0.0; len]];
    let mut start = 0;
    let mut k = 0;
    while start < len {
        let n = block.min(len - start);
        before(r, k);
        let ins: Vec<&[f32]> = inputs.iter().map(|i| &i[start..start + n]).collect();
        let [l, rr] = &mut outs;
        let mut o: [&mut [f32]; 2] = [&mut l[start..start + n], &mut rr[start..start + n]];
        r.process(n, &ins, &mut o, &mut NoStreams);
        start += n;
        k += 1;
    }
    outs
}

fn render(inputs: &[Vec<f32>; 2], params: &[(&str, f32)], block: usize) -> [Vec<f32>; 2] {
    process(&mut runner(params), inputs, block, |_, _| {})
}

fn guitar(len: usize) -> Vec<f32> {
    let a = signal::pluck(110.0, 0.6, 1.5, SR, len);
    let b = signal::pluck(164.8, 0.4, 1.2, SR, len);
    a.iter().zip(&b).map(|(x, y)| x + y).collect()
}

fn both(x: Vec<f32>) -> [Vec<f32>; 2] {
    [x.clone(), x]
}

/// Reverb only, no movement, no filters
const WET: [(&str, f32); 4] = [("mix", 100.0), ("modulation", 0.0), ("low_cut", 20.0), ("high_cut", 20_000.0)];

fn wet(extra: &[(&str, f32)]) -> Vec<(&'static str, f32)> {
    let mut p: Vec<(&str, f32)> = WET.to_vec();
    for (s, v) in extra {
        // Port symbols are 'static in the description
        let symbol = Plate::INFO.ports[at(s)].symbol;
        p.retain(|(k, _)| *k != symbol);
        p.push((symbol, *v));
    }
    p
}

/// Energy decay of `x` between `t0` and `t1` seconds, dB per second
fn slope(x: &[[f32; 2]], t0: f64, t1: f64) -> f64 {
    let window = |from: f64| {
        let a = (from * SR) as usize;
        x[a..a + (0.1 * SR) as usize].iter().map(|[l, r]| f64::from(l * l + r * r)).sum::<f64>()
    };
    10.0 * (window(t1) / window(t0)).log10() / (t1 - t0)
}

fn frames(out: &[Vec<f32>; 2]) -> Vec<[f32; 2]> {
    out[0].iter().zip(&out[1]).map(|(l, r)| [*l, *r]).collect()
}

#[test]
fn conformance() {
    orx_test::conformance::check::<Plate>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&PLATE, dir().join("ports.lock"));
}

#[test]
fn output_is_block_size_independent() {
    let input = [guitar(48_000), guitar(48_000).iter().map(|x| -0.5 * x).collect()];
    let params = [("modulation", 70.0), ("low_cut", 150.0), ("high_cut", 7000.0), ("predelay", 33.0)];
    let reference = render(&input, &params, 64);
    for block in [1, 7, 128, 500] {
        assert_eq!(render(&input, &params, block), reference, "block {block}");
    }
}

#[test]
fn zero_mix_is_transparent() {
    let input = [guitar(24_000), signal::noise(3, 0.3, 24_000)];
    let out = render(&input, &[("mix", 0.0)], 128);
    assert_eq!(out, input);
}

#[test]
fn the_decay_is_the_set_reverberation_time() {
    for decay in [1.0f32, 4.0] {
        let input = both(signal::impulse(0, (f64::from(decay) * 1.5 * SR) as usize));
        let out = frames(&render(&input, &wet(&[("decay", decay), ("low_decay", 1.0), ("high_decay", 1.0)]), 128));
        let d = f64::from(decay);
        let s = slope(&out, 0.3 * d, 1.2 * d);
        assert!((s + 60.0 / d).abs() < 0.03 * 60.0 / d, "{decay} s: {s} dB/s");
    }
}

#[test]
fn bands_decay_at_their_own_rates() {
    // Highs at a quarter of the decay, lows at twice: measured in bands
    // well away from the crossovers
    let decay = 2.0;
    let len = (decay * 1.5 * SR) as usize;
    let out = render(
        &both(signal::impulse(0, len)),
        &wet(&[("decay", decay as f32), ("low_decay", 2.0), ("high_decay", 0.25)]),
        128,
    );
    // Steep (48 dB/oct) analysis filters, so the slower bands do not leak
    // into the faster ones' measurement
    let band = |stages: &[(Shape, f64)]| {
        let mut filters: Vec<[Svf; 2]> =
            stages.iter().map(|&(shape, f)| [0, 1].map(|_| Svf::new(SvfCoeffs::new(shape, SR, f, 0.7, 0.0)))).collect();
        let filtered: Vec<[f32; 2]> = out[0]
            .iter()
            .zip(&out[1])
            .map(|(l, r)| {
                let mut v = [*l, *r];
                for f in filters.iter_mut() {
                    v = [f[0].process(v[0]), f[1].process(v[1])];
                }
                v
            })
            .collect();
        filtered
    };
    let hp = |f| [(Shape::Highpass, f); 4];
    let lp = |f| [(Shape::Lowpass, f); 4];
    let mid = slope(&band(&[hp(800.0), lp(1250.0)].concat()), 0.3, 1.5);
    // (past the first 100 ms, where the tank is still filling up)
    let high = slope(&band(&hp(12_000.0)), 0.1, 0.4);
    let low = slope(&band(&lp(50.0)), 0.6, 2.4);
    assert!((mid + 60.0 / decay).abs() < 0.1 * 60.0 / decay, "mid {mid}");
    assert!((high + 60.0 / (0.25 * decay)).abs() < 0.05 * 60.0 / (0.25 * decay), "high {high}");
    assert!((low + 60.0 / (2.0 * decay)).abs() < 0.15 * 60.0 / (2.0 * decay), "low {low}");
}

#[test]
fn pre_delay_holds_back_the_reverb() {
    let onset = |ms: f32| {
        let out = render(&both(signal::impulse(0, 24_000)), &wet(&[("predelay", ms)]), 128);
        out[0].iter().zip(&out[1]).position(|(l, r)| l.abs().max(r.abs()) > 1e-4).expect("reverb")
    };
    // Without pre-delay the reverb builds within 10 ms (diffusion and the
    // first taps); a pre-delay moves it by exactly its length
    let zero = onset(0.0);
    assert!(zero < 480, "{zero}");
    for ms in [60.0f32, 200.0] {
        assert_eq!(onset(ms) - zero, (f64::from(ms) * 0.001 * SR) as usize, "{ms} ms");
    }
}

#[test]
fn freeze_holds_and_lets_nothing_in() {
    let len = 48_000 * 8;
    let mut input = guitar(len);
    let quiet = {
        let mut q = input.clone();
        q[48_000..].fill(0.0);
        q
    };
    // Different input once frozen
    for (i, x) in input.iter_mut().enumerate().skip(48_000) {
        *x = 0.5 * (i as f32 * 0.03).sin();
    }
    let params = wet(&[("decay", 1.5)]);
    let freeze = at("freeze");
    let run = |input: &Vec<f32>| {
        let mut r = runner(&params);
        process(&mut r, &both(input.clone()), 128, |r, k| {
            if k == 24_000 / 128 {
                r.set_param(freeze, 1.0);
            }
        })
    };
    let a = run(&input);
    let b = run(&quiet);
    // From the end of the fade on, the frozen reverb ignores the input
    let settled = 24_000 + 2 * 128 + (0.06 * SR) as usize + (0.015 * SR) as usize;
    assert_eq!(a[0][settled..], b[0][settled..]);
    assert_eq!(a[1][settled..], b[1][settled..]);
    // and holds: seconds 2 and 7 within 1 dB, where a 1.5 s decay would
    // have fallen 200 dB
    let level = |from: usize| metrics::rms(&b[0][from..from + 48_000]) + metrics::rms(&b[1][from..from + 48_000]);
    let db = metrics::db(level(48_000 * 7 - 1) / level(48_000 * 2));
    assert!(db.abs() < 1.0 && level(48_000 * 2) > 1e-3, "{db} dB");
}

#[test]
fn a_left_source_stays_on_the_left() {
    let len = 4800;
    let out = render(&[signal::impulse(0, len), vec![0.0; len]], &wet(&[]), 128);
    let early = 0..(0.04 * SR) as usize;
    let (l, r) = (metrics::rms(&out[0][early.clone()]), metrics::rms(&out[1][early]));
    assert!(l > 1.5 * r, "left {l}, right {r}");
}

#[test]
fn changes_do_not_click() {
    // A sustained tone through the plate while every control sweeps: the
    // output moves no faster than the same tone without the changes
    let len = 48_000 * 3;
    let tone = signal::sine(330.0, 0.5, SR, len);
    let params = [("mix", 50.0)];
    let steady = render(&both(tone.clone()), &params, 128);
    let sweeps = [
        ("decay", 0.4, 15.0),
        ("predelay", 0.0, 200.0),
        ("high_decay", 0.1, 1.0),
        ("low_decay", 2.0, 0.25),
        ("low_cut", 20.0, 600.0),
        ("high_cut", 20_000.0, 1500.0),
        ("modulation", 0.0, 100.0),
        ("mix", 50.0, 80.0),
    ];
    let mut r = runner(&params);
    let moving = process(&mut r, &both(tone), 128, |r, k| {
        let t = (k as f32 / 250.0).min(1.0);
        for (s, from, to) in sweeps {
            r.set_param(at(s), from + (to - from) * t);
        }
    });
    // Only the tone's frequency (spread a little by the modulation) is
    // there: no sample moves faster than a 330 Hz sine of the window's
    // peak could, with a margin for the chorus. A click would.
    let slope = 2.0 * std::f32::consts::PI * 330.0 / SR as f32;
    for (c, out) in moving.iter().chain(&steady).enumerate() {
        for (w, x) in out.chunks(1024).enumerate().skip(4) {
            let step = x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
            let peak = metrics::peak(x);
            assert!(step < 1.2 * slope * peak, "output {c}, window {w}: step {step}, peak {peak}");
        }
    }
}

#[test]
fn bypass_lets_the_reverb_ring_out() {
    let len = 48_000 * 6;
    let mut input = guitar(len);
    input[24_064..].fill(0.0);
    let params = [("decay", 1.5), ("mix", 40.0)];
    let reference = render(&both(input.clone()), &params, 128);
    let mut r = runner(&params);
    let enabled = at("enabled");
    let out = process(&mut r, &both(input), 128, |r, k| {
        if k == 24_064 / 128 {
            r.set_param(enabled, 0.0);
        }
    });
    // Bypassed on silence: the tail goes on as if still on, until less
    // than -100 dBFS is left in the tank
    let idle = out[0].iter().zip(&reference[0]).position(|(a, b)| a != b).expect("the plugin stopped");
    assert!(idle > 48_000 * 2, "stopped at {idle}");
    assert!(metrics::max_diff(&out[0], &reference[0]) < 1e-5);
    assert!(metrics::rms(&out[0][36_000..48_000]) > 1e-3);
    assert!(r.is_idle());
}

#[test]
fn tails_end() {
    let mut r = runner(&[("decay", 2.0)]);
    let mut input = guitar(48_000);
    input.resize(48_000 * 10, 0.0);
    let out = process(&mut r, &both(input), 256, |_, _| {});
    assert!(!r.plugin().is_ringing());
    assert!(metrics::peak(&out[0][48_000 * 9..]) < 1e-5);
}

#[test]
fn golden_presets() {
    for preset in PLATE.presets {
        let mut input = guitar(48_000 * 3);
        input[48_000..].fill(0.0);
        let inputs = [input.clone(), input.iter().map(|x| x * 0.8).collect()];
        let out = render(&inputs, preset.values, 128);
        let channels: Vec<&[f32]> = out.iter().map(|c| c.as_slice()).collect();
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &channels, SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<Plate>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of Plate, described by PLATE
    let mut p = unsafe { host.instantiate(&d.raw, &PLATE, true) }.expect("instance");
    let params = [("decay", 3.0), ("predelay", 40.0), ("modulation", 60.0), ("high_cut", 8000.0)];
    for (s, v) in params {
        p.set(at(s), v);
    }
    p.activate();
    let input = guitar(256 * 60);
    let mut out = [Vec::new(), Vec::new()];
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        p.input(1).iter_mut().zip(chunk).for_each(|(b, x)| *b = -x);
        orx_test::rt::no_alloc(|| p.run(256));
        for (k, o) in out.iter_mut().enumerate() {
            o.extend_from_slice(p.output(k));
        }
    }
    let reference = render(&[input.clone(), input.iter().map(|x| -x).collect()], &params, 256);
    assert_eq!(out, reference);
}
