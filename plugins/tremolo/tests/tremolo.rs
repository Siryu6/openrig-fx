//! OR Tremolo: conformance, exact nulls, analytic checks, golden files,
//! and the LV2 build of the same code.
#![allow(clippy::expect_used)]

use orx_dsp::filter::Shape;
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_plugin::process::NoStreams;
use orx_plugin::runner::Runner;
use orx_plugin::{Config, Event, EventKind, Plugin, Position};
use orx_test::{metrics, signal};
use orx_tremolo::{MONO, Mono, STEREO, Stereo};
use std::path::PathBuf;

orx_test::install_rt_check!();

const SR: f64 = 48_000.0;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn at<P: Plugin>(symbol: &str) -> usize {
    P::INFO.port_index(symbol).expect("port symbol")
}

/// Process `inputs` (one per channel) in blocks of `block`
fn render<P: Plugin>(inputs: &[Vec<f32>], params: &[(&str, f32)], block: usize, events: &[Event]) -> Vec<Vec<f32>> {
    let mut r = Runner::<P>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at::<P>(s), *v);
    }
    r.activate();
    for e in events {
        r.push_event(*e);
    }
    let len = inputs[0].len();
    let mut outs = vec![vec![0.0; len]; P::INFO.audio_outputs()];
    let mut start = 0;
    while start < len {
        let n = block.min(len - start);
        let ins: Vec<&[f32]> = inputs.iter().map(|i| &i[start..start + n]).collect();
        let mut o: Vec<&mut [f32]> = outs.iter_mut().map(|o| &mut o[start..start + n]).collect();
        r.process(n, &ins, &mut o, &mut NoStreams);
        start += n;
    }
    outs
}

fn guitar(len: usize) -> Vec<f32> {
    let a = signal::pluck(110.0, 0.6, 1.5, SR, len);
    let b = signal::pluck(164.8, 0.4, 1.2, SR, len);
    a.iter().zip(&b).map(|(x, y)| x + y).collect()
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
    let input = guitar(20_000);
    for mode in 0..4 {
        let params = [("mode", mode as f32), ("depth", 90.0), ("shape", 60.0), ("spread", 90.0)];
        let stereo = [input.clone(), input.iter().map(|x| -x).collect()];
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
fn zero_depth_is_transparent() {
    let input = guitar(24_000);
    for mode in [0.0, 1.0] {
        let out =
            render::<Mono>(std::slice::from_ref(&input), &[("mode", mode), ("depth", 0.0), ("shape", 100.0)], 128, &[]);
        assert_eq!(out[0], input, "mode {mode}");
    }
    let stereo = [input.clone(), signal::noise(3, 0.5, 24_000)];
    let out = render::<Stereo>(&stereo, &[("mode", 3.0), ("depth", 0.0)], 128, &[]);
    assert_eq!(out[0], stereo[0]);
    assert_eq!(out[1], stereo[1]);
}

#[test]
fn harmonic_bands_sum_to_an_allpass() {
    // Linkwitz-Riley low + high = Butterworth-Q allpass, exactly in theory;
    // here to float rounding
    let input = guitar(48_000);
    let out =
        render::<Mono>(std::slice::from_ref(&input), &[("mode", 2.0), ("depth", 0.0), ("crossover", 800.0)], 128, &[]);
    let mut ap = Svf::new(SvfCoeffs::new(Shape::Allpass, SR, 800.0, core::f64::consts::FRAC_1_SQRT_2, 0.0));
    let reference: Vec<f32> = input.iter().map(|x| ap.process(*x)).collect();
    let null = metrics::null_db(&reference, &out[0]);
    assert!(null < -110.0, "null depth {null} dB");
}

#[test]
fn harmonic_bands_move_in_antiphase() {
    // A low and a high tone: their envelopes peak half a cycle apart
    let len = 48_000;
    let low = signal::sine(100.0, 0.5, SR, len);
    let high = signal::sine(4000.0, 0.5, SR, len);
    let mix: Vec<f32> = low.iter().zip(&high).map(|(a, b)| a + b).collect();
    let params = [("mode", 2.0), ("depth", 100.0), ("rate", 2.0), ("crossover", 630.0)];
    let out = &render::<Mono>(&[mix], &params, 128, &[])[0];
    // At LFO phase 0.25 (sine peak, t = 1/8 s) the low band is full and the
    // high band silent; at phase 0.75 the reverse
    let window = |t: f64| &out[(t * SR) as usize - 600..(t * SR) as usize + 600];
    let amp = |w: &[f32], f| metrics::tone_amplitude(w, f, SR);
    let (peak, trough) = (window(0.125 + 0.5), window(0.375 + 0.5));
    assert!(amp(peak, 100.0) > 0.45 && amp(peak, 4000.0) < 0.03, "{} {}", amp(peak, 100.0), amp(peak, 4000.0));
    assert!(amp(trough, 4000.0) > 0.45 && amp(trough, 100.0) < 0.03);
}

#[test]
fn classic_follows_the_analytic_lfo() {
    // DC in, full depth, sine LFO at 3 Hz: out = (1 + sin 2πft) / 2
    let len = 48_000;
    let out = &render::<Mono>(&[signal::dc(1.0, len)], &[("depth", 100.0), ("rate", 3.0), ("shape", 0.0)], 100, &[])[0];
    let worst = out
        .iter()
        .enumerate()
        .map(|(i, y)| {
            let t = i as f64 / SR;
            (f64::from(*y) - 0.5 * (1.0 + (std::f64::consts::TAU * 3.0 * t).sin())).abs()
        })
        .fold(0.0, f64::max);
    assert!(worst < 2e-6, "largest error {worst}");
}

#[test]
fn shape_morphs_to_a_rounded_square() {
    let len = 48_000;
    let out =
        &render::<Mono>(&[signal::dc(1.0, len)], &[("depth", 100.0), ("rate", 2.0), ("shape", 100.0)], 128, &[])[0];
    // Mostly near 0 or 1, but no step larger than a smooth edge allows
    let extremes = out.iter().filter(|y| **y < 0.02 || **y > 0.98).count();
    assert!(extremes as f32 > 0.8 * len as f32, "{extremes}");
    let step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
    assert!(step < 0.01, "largest step {step}");
}

#[test]
fn opto_lags_and_softens() {
    // Square LFO at 2 Hz: the gain falls around 0.75 s and rises around
    // 0.5 s. Count the samples spent between 10 % and 90 % around each edge:
    // the lamp stretches both, its slow release far more
    let edges = |mode: f32| {
        let params = [("mode", mode), ("depth", 100.0), ("rate", 2.0), ("shape", 100.0)];
        let out = &render::<Mono>(&[signal::dc(1.0, 48_000)], &params, 128, &[])[0];
        let between =
            |centre: usize| out[centre - 6_000..centre + 6_000].iter().filter(|y| **y > 0.1 && **y < 0.9).count();
        (between(36_000), between(24_000))
    };
    let (classic_fall, classic_rise) = edges(0.0);
    let (opto_fall, opto_rise) = edges(1.0);
    println!("fall/rise: classic {classic_fall}/{classic_rise}, opto {opto_fall}/{opto_rise} samples");
    assert!(opto_fall > classic_fall && opto_rise > classic_rise + 1_000);
    assert!(opto_rise > opto_fall + 1_000);
}

#[test]
fn sync_follows_the_tempo() {
    // 120 BPM, one cycle per quarter note: 2 Hz, so whole cycles in 1 s
    let events = [Event {
        frame: 0,
        kind: EventKind::Position(Position { bpm: Some(120.0), speed: Some(1.0), ..Position::default() }),
    }];
    let len = 48_000;
    let mut r = Runner::<Mono>::new(Config { sample_rate: SR, max_block: 128 });
    r.set_param(at::<Mono>("sync"), 4.0);
    r.activate();
    r.push_event(events[0]);
    let input = signal::dc(1.0, len);
    let mut out = vec![0.0; len];
    for (i, o) in input.chunks(128).zip(out.chunks_mut(128)) {
        r.process(i.len(), &[i], &mut [o], &mut NoStreams);
    }
    let phase = r.meter(at::<Mono>("lfo_phase"));
    assert!(phase.min(1.0 - phase) < 1e-6, "phase after 2 cycles: {phase}");
    // Without a tempo, the Rate knob applies (5 Hz default: 5 cycles)
    let out = render::<Mono>(&[input], &[("sync", 4.0), ("depth", 100.0)], 128, &[]);
    let peaks = out[0].windows(3).filter(|w| w[1] > w[0] && w[1] >= w[2] && w[1] > 0.99).count();
    assert_eq!(peaks, 5);
}

#[test]
fn mode_changes_do_not_click() {
    let len = 24_000;
    let input = signal::sine(220.0, 0.5, SR, len);
    let mut r = Runner::<Mono>::new(Config { sample_rate: SR, max_block: 128 });
    r.set_param(at::<Mono>("depth"), 100.0);
    r.set_param(at::<Mono>("rate"), 0.5);
    r.activate();
    let mut out = vec![0.0; len];
    for (k, (i, o)) in input.chunks(128).zip(out.chunks_mut(128)).enumerate() {
        r.set_param(at::<Mono>("mode"), (k / 20 % 3) as f32);
        r.process(i.len(), &[i], &mut [o], &mut NoStreams);
    }
    // The largest sample step stays that of the 220 Hz sine itself
    let sine_step = (std::f32::consts::TAU * 220.0 / SR as f32) * 0.5;
    let step = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
    assert!(step < 1.1 * sine_step, "step {step}, sine {sine_step}");
}

fn golden<P: Plugin>(name: &str, params: &[(&str, f32)]) {
    let input = guitar(48_000);
    let inputs: Vec<Vec<f32>> = (0..P::INFO.audio_inputs())
        .map(|c| if c == 0 { input.clone() } else { input.iter().map(|x| x * 0.8).collect() })
        .collect();
    let out = render::<P>(&inputs, params, 128, &[]);
    let channels: Vec<&[f32]> = out.iter().map(|c| c.as_slice()).collect();
    orx_test::golden::check(dir().join("tests/golden").join(format!("{name}.wav")), &channels, SR as u32, 2e-5);
}

#[test]
fn golden_presets() {
    for preset in MONO.presets {
        let values: Vec<(&str, f32)> = preset.values.to_vec();
        golden::<Mono>(&format!("mono-{}", preset.id), &values);
    }
    for preset in STEREO.presets.iter().filter(|p| p.id == "wide-pan" || p.id == "spread-chop") {
        golden::<Stereo>(&format!("stereo-{}", preset.id), preset.values);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<Stereo>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of Stereo, described by STEREO
    let mut p = unsafe { host.instantiate(&d.raw, &STEREO, true) }.unwrap();
    let params = [("mode", 2.0), ("depth", 70.0), ("shape", 30.0), ("spread", 120.0)];
    for (s, v) in params {
        p.set(at::<Stereo>(s), v);
    }
    p.activate();
    let input = guitar(256 * 40);
    let mut out = [Vec::new(), Vec::new()];
    let mut frames = 0;
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        p.input(1).iter_mut().zip(chunk).for_each(|(b, x)| *b = -x);
        p.push_position(0, 100.0, 1.0);
        orx_test::rt::no_alloc(|| p.run(256));
        for (k, o) in out.iter_mut().enumerate() {
            o.extend_from_slice(p.output(k));
        }
        for f in p.frames(at::<Stereo>("cycle")) {
            assert_eq!((f.channels, f.values.len()), (2, 128));
            assert!(f.values.iter().all(|v| (0.0..=1.0).contains(v)));
            frames += 1;
        }
    }
    // 40 runs of 5.3 ms: one frame per 33 ms at most
    assert!((5..=8).contains(&frames), "{frames} frames");
    let stereo = [input.clone(), input.iter().map(|x| -x).collect()];
    let reference = render::<Stereo>(&stereo, &params, 256, &[]);
    assert_eq!(out[0], reference[0]);
    assert_eq!(out[1], reference[1]);
}
