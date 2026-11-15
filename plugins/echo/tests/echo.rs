//! OR Echo: conformance, exact echoes, click-free changes, trails, golden
//! files, and the LV2 build of the same code.
#![allow(clippy::expect_used)]

use orx_echo::{MONO, Mono, STEREO, Stereo};
use orx_plugin::process::NoStreams;
use orx_plugin::runner::Runner;
use orx_plugin::{Config, Event, EventKind, Plugin, Position};
use orx_test::{metrics, signal};
use std::path::PathBuf;

orx_test::install_rt_check!();

const SR: f64 = 48_000.0;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn at<P: Plugin>(symbol: &str) -> usize {
    P::INFO.port_index(symbol).expect("port symbol")
}

fn tempo(bpm: f32) -> Event {
    Event { frame: 0, kind: EventKind::Position(Position { bpm: Some(bpm), speed: Some(1.0), ..Position::default() }) }
}

fn runner<P: Plugin>(params: &[(&str, f32)]) -> Runner<P> {
    let mut r = Runner::<P>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at::<P>(s), *v);
    }
    r.activate();
    r
}

/// Process `inputs` (one per channel) in blocks of `block`, calling
/// `before` ahead of each block with its index
fn process<P: Plugin>(
    r: &mut Runner<P>,
    inputs: &[Vec<f32>],
    block: usize,
    mut before: impl FnMut(&mut Runner<P>, usize),
) -> Vec<Vec<f32>> {
    let len = inputs[0].len();
    let mut outs = vec![vec![0.0; len]; P::INFO.audio_outputs()];
    let mut start = 0;
    let mut k = 0;
    while start < len {
        let n = block.min(len - start);
        before(r, k);
        let ins: Vec<&[f32]> = inputs.iter().map(|i| &i[start..start + n]).collect();
        let mut o: Vec<&mut [f32]> = outs.iter_mut().map(|o| &mut o[start..start + n]).collect();
        r.process(n, &ins, &mut o, &mut NoStreams);
        start += n;
        k += 1;
    }
    outs
}

fn render<P: Plugin>(inputs: &[Vec<f32>], params: &[(&str, f32)], block: usize, events: &[Event]) -> Vec<Vec<f32>> {
    let mut r = runner::<P>(params);
    for e in events {
        r.push_event(*e);
    }
    process(&mut r, inputs, block, |_, _| {})
}

fn guitar(len: usize) -> Vec<f32> {
    let a = signal::pluck(110.0, 0.6, 1.5, SR, len);
    let b = signal::pluck(164.8, 0.4, 1.2, SR, len);
    a.iter().zip(&b).map(|(x, y)| x + y).collect()
}

/// Wet only, one echo, no movement: the output is the line itself
const WET_ONLY: [(&str, f32); 4] = [("dry", 0.0), ("level", 100.0), ("feedback", 0.0), ("modulation", 0.0)];

fn largest_step(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

#[test]
fn conformance_mono() {
    orx_test::conformance::check::<Mono>();
}

#[test]
fn conformance_stereo() {
    orx_test::conformance::check::<Stereo>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&MONO, dir().join("ports-mono.lock"));
    orx_test::ports_lock::check(&STEREO, dir().join("ports-stereo.lock"));
}

#[test]
fn every_mode_is_block_size_independent() {
    let input = guitar(30_000);
    for mode in 0..3 {
        let params = [
            ("mode", mode as f32),
            ("time", 90.0),
            ("feedback", 70.0),
            ("modulation", 60.0),
            ("low_cut", 150.0),
            ("high_cut", 4000.0),
            ("duck", 12.0),
            ("ping_pong", 1.0),
        ];
        let stereo = [input.clone(), input.iter().map(|x| -0.5 * x).collect()];
        let reference = render::<Stereo>(&stereo, &params, 512, &[]);
        for block in [1, 13, 128] {
            let other = render::<Stereo>(&stereo, &params, block, &[]);
            for (a, b) in reference.iter().zip(&other) {
                assert_eq!(a, b, "mode {mode}, blocks of {block}");
            }
        }
    }
}

#[test]
fn zero_level_is_transparent() {
    let input = guitar(24_000);
    for mode in [0.0, 1.0, 2.0] {
        let out = render::<Mono>(
            std::slice::from_ref(&input),
            &[("mode", mode), ("level", 0.0), ("feedback", 80.0), ("modulation", 100.0)],
            128,
            &[],
        );
        assert_eq!(out[0], input, "mode {mode}");
    }
}

#[test]
fn digital_echoes_are_exact() {
    // 100 ms = 4800 samples, feedback 1/2, level 1: echo n of an impulse
    // is 2^(1−n), n·4800 samples later, and nothing else
    let len = 4800 * 6 + 10;
    let params = [("mode", 0.0), ("time", 100.0), ("feedback", 50.0), ("level", 100.0), ("modulation", 0.0)];
    let out = &render::<Mono>(&[signal::impulse(0, len)], &params, 64, &[])[0];
    for (i, y) in out.iter().enumerate() {
        let expected = match (i % 4800, i / 4800) {
            (0, 0) => 1.0,
            (0, n) => 0.5f32.powi(n as i32 - 1),
            _ => 0.0,
        };
        assert_eq!(*y, expected, "sample {i}");
    }
}

#[test]
fn sync_follows_the_tempo() {
    // 120 BPM: a quarter note is 500 ms, a dotted eighth 375 ms
    for (division, ms) in [(4.0, 500.0), (8.0, 375.0), (10.0, 125.0)] {
        let mut r = runner::<Mono>(&[("sync", division), ("modulation", 0.0), ("level", 100.0), ("dry", 0.0)]);
        r.push_event(tempo(120.0));
        let len = 30_000;
        let out = &process(&mut r, &[signal::impulse(0, len)], 128, |_, _| {})[0];
        let first = out.iter().position(|y| y.abs() > 0.5).expect("an echo");
        assert_eq!(first, (ms * 48.0) as usize, "division {division}");
        assert_eq!(r.meter(at::<Mono>("delay_ms")), ms);
    }
    // Without a tempo the Time knob applies
    let mut r = runner::<Mono>(&[("sync", 4.0), ("time", 250.0)]);
    process(&mut r, &[vec![0.0; 256]], 128, |_, _| {});
    assert_eq!(r.meter(at::<Mono>("delay_ms")), 250.0);
}

#[test]
fn time_changes_do_not_click() {
    // The delayed sine itself moves by at most `sine_step` per sample; a
    // crossfade or a glide must not add a step of its own
    let len = 96_000;
    let amplitude = 0.5;
    let input = signal::sine(220.0, amplitude, SR, len);
    let sine_step = (std::f32::consts::TAU * 220.0 / SR as f32) * amplitude;
    let times = [300.0, 47.0, 1200.0, 512.0, 80.0];
    // Tape and analog glide up to half again faster: a sine bent half a
    // step higher in pitch moves 1.5 times as fast
    for (mode, allowed) in [(0.0, 1.05), (1.0, 1.55), (2.0, 1.55)] {
        let mut r = runner::<Mono>(&[("mode", mode), ("drive", 0.0)]);
        for (s, v) in WET_ONLY {
            r.set_param(at::<Mono>(s), v);
        }
        r.activate();
        let out = &process(&mut r, std::slice::from_ref(&input), 128, |r, k| {
            r.set_param(at::<Mono>("time"), times[k / 120 % times.len()]);
        })[0];
        // From the first echo on (the line starts empty)
        let step = largest_step(&out[24_000..]);
        // The saturation of the analog and tape lines lowers the level
        // a little: compare with the steps of the sine at its own level
        assert!(step < allowed * sine_step, "mode {mode}: step {step}, sine {sine_step}");
    }
}

#[test]
fn mode_changes_do_not_click() {
    let len = 72_000;
    let input = signal::sine(220.0, 0.5, SR, len);
    let sine_step = (std::f32::consts::TAU * 220.0 / SR as f32) * 0.5;
    let mut r = runner::<Mono>(&[("time", 150.0), ("drive", 0.0)]);
    for (s, v) in WET_ONLY {
        r.set_param(at::<Mono>(s), v);
    }
    r.activate();
    let out = &process(&mut r, std::slice::from_ref(&input), 128, |r, k| {
        r.set_param(at::<Mono>("mode"), (k / 40 % 3) as f32);
    })[0];
    let step = largest_step(&out[8_000..]);
    assert!(step < 1.1 * sine_step, "step {step}, sine {sine_step}");
}

#[test]
fn runaway_stays_bounded_and_rings() {
    // Feedback above 100 %: the loop builds up to the saturation or the
    // limiter and holds there
    for mode in [0.0, 1.0, 2.0] {
        let mut input = guitar(48_000 * 4);
        input[48_000..].fill(0.0);
        let params = [("mode", mode), ("time", 200.0), ("feedback", 110.0), ("level", 100.0), ("dry", 0.0)];
        let out = &render::<Mono>(&[input], &params, 256, &[])[0];
        // The line stays within ±1; reading between its samples (the
        // modulation moves the heads) may overshoot by a cubic's bulge
        let peak = metrics::peak(out);
        assert!(peak <= 1.25, "mode {mode}: peak {peak}");
        let last_second = metrics::rms(&out[48_000 * 3..]);
        assert!(last_second > 0.05, "mode {mode}: rms {last_second}");
    }
}

#[test]
fn ducking_lowers_the_echoes_while_playing() {
    // A steady -10 dBFS tone, ducked by 12 dB from -30 dBFS: echoes at a
    // quarter of their level; when the tone stops, the tail comes back
    let len = 96_000;
    let mut input = signal::sine(440.0, 0.316, SR, len);
    input[48_000..].fill(0.0);
    let params = [("time", 100.0), ("feedback", 70.0), ("level", 100.0), ("modulation", 0.0), ("dry", 0.0)];
    let plain = &render::<Mono>(std::slice::from_ref(&input), &params, 128, &[])[0];
    let mut ducked_params = params.to_vec();
    ducked_params.extend([("duck", 12.0), ("duck_threshold", -30.0)]);
    let mut r = runner::<Mono>(&ducked_params);
    let ducked = &process(&mut r, std::slice::from_ref(&input), 128, |_, _| {})[0];
    let window = 30_000..46_000;
    let ratio = metrics::rms(&ducked[window.clone()]) / metrics::rms(&plain[window]);
    assert!((metrics::db(ratio) + 12.0).abs() < 0.2, "{} dB", metrics::db(ratio));
    // The envelope falls from -10 to below -33 dBFS in 0.66 s (250 ms
    // release); 0.75 s after the tone stops the tail is the same as
    // without ducking
    let tail = 84_000..96_000;
    let null = metrics::null_db(&plain[tail.clone()], &ducked[tail]);
    assert!(null < -40.0, "{null} dB");
    assert_eq!(r.meter(at::<Mono>("ducking")), 0.0);
}

#[test]
fn ping_pong_alternates() {
    // An impulse on the left: half of it (the inputs are summed) on the
    // left after one delay, then right, left..., each times the feedback
    let d = 4800;
    let len = d * 5 + 10;
    let params = [("time", 100.0), ("feedback", 50.0), ("level", 100.0), ("modulation", 0.0), ("ping_pong", 1.0)];
    let out = render::<Stereo>(&[signal::impulse(0, len), vec![0.0; len]], &params, 64, &[]);
    for n in 1..=5 {
        let (left, right) = (out[0][n * d], out[1][n * d]);
        let expected = 0.5 * 0.5f32.powi(n as i32 - 1);
        let (on, off) = if n % 2 == 1 { (left, right) } else { (right, left) };
        assert_eq!((on, off), (expected, 0.0), "echo {n}");
    }
    let total: f32 = out.iter().flat_map(|c| c.iter()).map(|y| y.abs()).sum();
    let expected: f32 = 1.0 + (1..=5).map(|n| 0.5 * 0.5f32.powi(n - 1)).sum::<f32>();
    assert!((total - expected).abs() < 1e-6, "{total} vs {expected}");
}

#[test]
fn analog_darkens_as_the_time_grows() {
    // The first echo of white noise: share of energy above 4 kHz
    let len = 48_000 * 2;
    let noise = signal::noise(7, 0.3, len);
    let bright = |time: f32| {
        let params = [("mode", 2.0), ("time", time), ("feedback", 0.0), ("level", 100.0), ("dry", 0.0)];
        let out = &render::<Mono>(std::slice::from_ref(&noise), &params, 256, &[])[0];
        let tail = &out[48_000..];
        let mut hp = orx_dsp::filter::svf::Svf::new(orx_dsp::filter::svf::SvfCoeffs::new(
            orx_dsp::filter::Shape::Highpass,
            SR,
            4000.0,
            std::f64::consts::FRAC_1_SQRT_2,
            0.0,
        ));
        let high: Vec<f32> = tail.iter().map(|x| hp.process(*x)).collect();
        metrics::rms(&high) / metrics::rms(tail)
    };
    let (short, long) = (bright(80.0), bright(900.0));
    assert!(short > 2.0 * long, "short {short}, long {long}");
}

#[test]
fn bypass_lets_the_echoes_ring_out() {
    let len = 48_000 * 5;
    let mut input = guitar(len);
    input[24_064..].fill(0.0);
    let params = [("time", 250.0), ("feedback", 40.0), ("level", 80.0)];
    let mut r = runner::<Mono>(&params);
    let reference = render::<Mono>(std::slice::from_ref(&input), &params, 128, &[]);
    let enabled = at::<Mono>("enabled");
    let out = &process(&mut r, std::slice::from_ref(&input), 128, |r, k| {
        if k == 24_064 / 128 {
            r.set_param(enabled, 0.0);
        }
    })[0];
    // Bypassed on silence: the tail goes on exactly as if still on, until
    // the plugin stops with less than -100 dBFS left in its line
    let idle = out.iter().zip(&reference[0]).position(|(a, b)| a != b).expect("the plugin stopped");
    assert!(idle > 48_000 * 2, "stopped at {idle}");
    assert!(metrics::max_diff(out, &reference[0]) < 1e-5);
    assert!(metrics::rms(&out[30_000..40_000]) > 1e-3);
    // Then the line goes quiet and the plugin stops
    assert!(r.is_idle());
}

#[test]
fn tails_end() {
    // The plugin reports silence once its line holds nothing audible
    let mut r = runner::<Mono>(&[("time", 500.0), ("feedback", 50.0), ("mode", 1.0)]);
    let mut input = guitar(48_000);
    input.resize(48_000 * 12, 0.0);
    let out = &process(&mut r, &[input], 256, |_, _| {})[0];
    assert!(!r.plugin().is_ringing());
    assert!(metrics::peak(&out[48_000 * 11..]) < 1e-5);
}

fn golden<P: Plugin>(name: &str, params: &[(&str, f32)]) {
    let mut input = guitar(48_000 * 2);
    input[48_000..].fill(0.0);
    let inputs: Vec<Vec<f32>> = (0..P::INFO.audio_inputs())
        .map(|c| if c == 0 { input.clone() } else { input.iter().map(|x| x * 0.8).collect() })
        .collect();
    let out = render::<P>(&inputs, params, 128, &[tempo(120.0)]);
    let channels: Vec<&[f32]> = out.iter().map(|c| c.as_slice()).collect();
    orx_test::golden::check(dir().join("tests/golden").join(format!("{name}.wav")), &channels, SR as u32, 2e-5);
}

#[test]
fn golden_presets() {
    for preset in MONO.presets {
        golden::<Mono>(&format!("mono-{}", preset.id), preset.values);
    }
    for preset in STEREO.presets.iter().filter(|p| p.id == "ping-pong" || p.id == "wide-analog") {
        golden::<Stereo>(&format!("stereo-{}", preset.id), preset.values);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<Stereo>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of Stereo, described by STEREO
    let mut p = unsafe { host.instantiate(&d.raw, &STEREO, true) }.expect("instance");
    let params = [("mode", 1.0), ("sync", 7.0), ("feedback", 60.0), ("ping_pong", 1.0), ("duck", 6.0)];
    for (s, v) in params {
        p.set(at::<Stereo>(s), v);
    }
    p.activate();
    let input = guitar(256 * 60);
    let mut out = [Vec::new(), Vec::new()];
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        p.input(1).iter_mut().zip(chunk).for_each(|(b, x)| *b = -x);
        p.push_position(0, 100.0, 1.0);
        orx_test::rt::no_alloc(|| p.run(256));
        for (k, o) in out.iter_mut().enumerate() {
            o.extend_from_slice(p.output(k));
        }
    }
    let stereo = [input.clone(), input.iter().map(|x| -x).collect()];
    let reference = render::<Stereo>(&stereo, &params, 256, &[tempo(100.0)]);
    assert_eq!(out[0], reference[0]);
    assert_eq!(out[1], reference[1]);
}
