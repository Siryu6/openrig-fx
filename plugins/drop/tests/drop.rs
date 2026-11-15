//! OR Drop: conformance, pitch accuracy from E1 up, artefacts on single
//! notes and on a power chord in both modes, the latency against the
//! reported one, the dry path, the dive, click-free changes, golden
//! files, and the LV2 build of the same code.
#![allow(clippy::expect_used)]

use orx_drop::{DROP, DropPedal};
use orx_plugin::process::NoStreams;
use orx_plugin::runner::Runner;
use orx_plugin::{Config, Plugin};
use orx_test::signal;
use std::f64::consts::TAU;
use std::path::PathBuf;

orx_test::install_rt_check!();

const SR: f64 = 48_000.0;
/// Latency of each mode at 48 kHz, samples
const TIGHT: u32 = 288;
const QUALITY: u32 = 672;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn at(symbol: &str) -> usize {
    DROP.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<DropPedal> {
    let mut r = Runner::<DropPedal>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

fn process(
    r: &mut Runner<DropPedal>,
    input: &[f32],
    block: usize,
    mut before: impl FnMut(&mut Runner<DropPedal>, usize),
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

/// A steady note at `f0` with six harmonics falling as 1/k, peak near 0.3
fn note(f0: f64, len: usize) -> Vec<f32> {
    let h: Vec<(f64, f32)> = (1..=6).map(|k| (f0 * k as f64, 0.13 / k as f32)).collect();
    signal::chord(&h, SR, len)
}

/// A power chord: root and fifth (3:2), each with four harmonics
fn power_chord(root: f64, len: usize) -> Vec<f32> {
    let mut h = Vec::new();
    for k in 1..=4 {
        h.push((root * k as f64, 0.1 / k as f32));
        h.push((1.5 * root * k as f64, 0.08 / k as f32));
    }
    signal::chord(&h, SR, len)
}

/// Hann-windowed amplitude of `f` in `x`
fn amplitude(x: &[f32], f: f64) -> f64 {
    let n = x.len() as f64;
    let (step_re, step_im) = ((TAU * f / SR).cos(), -(TAU * f / SR).sin());
    let (mut c, mut s) = (1.0f64, 0.0f64);
    let (mut re, mut im) = (0.0, 0.0);
    for (i, v) in x.iter().enumerate() {
        let w = 0.5 - 0.5 * (TAU * i as f64 / n).cos();
        re += w * f64::from(*v) * c;
        im += w * f64::from(*v) * s;
        (c, s) = (c * step_re - s * step_im, c * step_im + s * step_re);
    }
    (re * re + im * im).sqrt()
}

/// Frequency of the strongest component of `x` between `lo` and `hi`
fn peak_frequency(x: &[f32], lo: f64, hi: f64) -> f64 {
    let scan = |lo: f64, hi: f64, step: f64| {
        let mut best = (lo, 0.0);
        let mut f = lo;
        while f <= hi {
            let a = amplitude(x, f);
            if a > best.1 {
                best = (f, a);
            }
            f += step;
        }
        best.0
    };
    let coarse = scan(lo, hi, 0.25);
    let fine = scan(coarse - 0.3, coarse + 0.3, 0.02);
    scan(fine - 0.025, fine + 0.025, 0.002)
}

fn cents(f: f64, reference: f64) -> f64 {
    1200.0 * (f / reference).log2()
}

/// Power of `x` outside the harmonics of `f0` relative to all of it, dB:
/// a least-squares fit of the first harmonics (up to 2 kHz), the residual
/// is what the splices added
fn inharmonic_db(x: &[f32], f0: f64) -> f64 {
    inharmonic_multi_db(x, &[f0], 2000.0)
}

/// Like `inharmonic_db`, the harmonics of several notes up to `top` Hz
fn inharmonic_multi_db(x: &[f32], notes: &[f64], top: f64) -> f64 {
    // Partials shared by two notes (an octave) fitted once
    let mut freqs: Vec<f64> = Vec::new();
    for f in notes.iter().flat_map(|f0| (1..=((top / f0) as usize).clamp(1, 24)).map(move |h| f0 * h as f64)) {
        if freqs.iter().all(|g| (g - f).abs() > 0.05) {
            freqs.push(f);
        }
    }
    let k = freqs.len();
    let m = 2 * k;
    let mut ata = vec![0.0f64; m * m];
    let mut atb = vec![0.0f64; m];
    let mut basis = vec![0.0f64; m];
    for (i, v) in x.iter().enumerate() {
        let t = i as f64 / SR;
        for (h, f) in freqs.iter().enumerate() {
            let p = TAU * f * t;
            basis[2 * h] = p.cos();
            basis[2 * h + 1] = p.sin();
        }
        for a in 0..m {
            atb[a] += basis[a] * f64::from(*v);
            for b in a..m {
                ata[a * m + b] += basis[a] * basis[b];
            }
        }
    }
    for a in 0..m {
        for b in 0..a {
            ata[a * m + b] = ata[b * m + a];
        }
    }
    // Gaussian elimination (the matrix is well conditioned: nearly
    // orthogonal sines over many periods)
    for c in 0..m {
        let p = ata[c * m + c];
        for r in c + 1..m {
            let f = ata[r * m + c] / p;
            for j in c..m {
                ata[r * m + j] -= f * ata[c * m + j];
            }
            atb[r] -= f * atb[c];
        }
    }
    let mut coef = vec![0.0f64; m];
    for c in (0..m).rev() {
        let s: f64 = (c + 1..m).map(|j| ata[c * m + j] * coef[j]).sum();
        coef[c] = (atb[c] - s) / ata[c * m + c];
    }
    let (mut total, mut rest) = (0.0, 0.0);
    for (i, v) in x.iter().enumerate() {
        let t = i as f64 / SR;
        let fit: f64 = (0..k)
            .map(|h| {
                let p = TAU * freqs[h] * t;
                coef[2 * h] * p.cos() + coef[2 * h + 1] * p.sin()
            })
            .sum();
        total += f64::from(*v).powi(2);
        rest += (f64::from(*v) - fit).powi(2);
    }
    10.0 * (rest.max(1e-30) / total).log10()
}

fn mode_value(quality: bool) -> f32 {
    if quality { 1.0 } else { 0.0 }
}

/// Shifted output of `input` from 0.5 s on
fn shifted(input: &[f32], shift: i32, quality: bool) -> Vec<f32> {
    let out = render(input, &[("shift", shift as f32), ("mode", mode_value(quality))], 128);
    out[24_000..].to_vec()
}

#[test]
fn conformance() {
    orx_test::conformance::check::<DropPedal>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&DROP, dir().join("ports.lock"));
}

#[test]
fn the_pitch_is_exact_from_e1_up() {
    for (f0, quality) in [(41.2, true), (82.4, true), (329.6, true), (41.2, false), (110.0, false), (329.6, false)] {
        let input = note(f0, 120_000);
        for shift in [-2, -5, -12, 7] {
            let out = shifted(&input, shift, quality);
            let expected = f0 * (f64::from(shift) / 12.0).exp2();
            let f = peak_frequency(&out, expected * 0.95, expected * 1.05);
            let c = cents(f, expected);
            assert!(c.abs() < 3.0, "{f0} Hz, {shift} st, quality {quality}: {f} Hz, {c:.2} cents");
        }
    }
}

#[test]
fn whole_period_splices_add_almost_nothing() {
    // Every period is spliced whole: clean harmonic tones, E1 included
    // (in Tight with more delay)
    let mut report = Vec::new();
    for (f0, quality, most) in
        [(41.2, true, -40.0), (82.4, true, -40.0), (41.2, false, -40.0), (82.4, false, -40.0), (329.6, false, -40.0)]
    {
        let input = note(f0, 120_000);
        for shift in [-2, -12] {
            let out = shifted(&input, shift, quality);
            let f = f0 * (f64::from(shift) / 12.0).exp2();
            let db = inharmonic_db(&out[..48_000], f);
            report.push((f0, quality, shift, db));
            assert!(db < most, "{f0} Hz, {shift} st, quality {quality}: {db:.1} dB");
        }
    }
    println!("{report:?}");
}

#[test]
fn power_chords_splice_on_their_common_period() {
    // A power chord repeats every two periods of its root (the fifth is
    // 3:2): 24 ms on E2, 32 ms on B1, still within the detector's reach
    for (root, most) in [(82.4, -40.0), (61.7, -20.0)] {
        let input = power_chord(root, 120_000);
        for quality in [false, true] {
            for shift in [-2, -12] {
                let common = root / 2.0 * (f64::from(shift) / 12.0).exp2();
                let db = inharmonic_db(&shifted(&input, shift, quality)[..48_000], common);
                assert!(db < most, "{root} Hz, {shift} st, quality {quality}: {db:.1} dB");
            }
        }
    }
}

/// The artefact table of the documentation: `cargo test -p orx-drop
/// --release -- --ignored --nocapture artefacts`
#[test]
#[ignore]
fn artefacts() {
    println!("inharmonic power, dB (single notes, then power chords on the root)");
    for (name, f0, chord) in [
        ("F#1", 46.2, false),
        ("B1", 61.7, false),
        ("D2", 73.4, false),
        ("E2", 82.4, false),
        ("A2", 110.0, false),
        ("E4", 329.6, false),
        ("B1 5th", 61.7, true),
        ("D2 5th", 73.4, true),
        ("E2 5th", 82.4, true),
        ("A2 5th", 110.0, true),
    ] {
        let input = if chord { power_chord(f0, 120_000) } else { note(f0, 120_000) };
        let common = if chord { f0 / 2.0 } else { f0 };
        let mut row = format!("{name:<8}");
        for shift in [-2, -5, -12] {
            let f = common * (f64::from(shift) / 12.0).exp2();
            for quality in [false, true] {
                let db = inharmonic_db(&shifted(&input, shift, quality)[..48_000], f);
                row += &format!(" {db:7.1}");
            }
        }
        println!("{row}");
    }
    println!(
        "equal-tempered chords, inharmonic power against every note's partials up to 1 kHz, dB (0 st: the floor of the fit)"
    );
    let fifth = (7.0f64 / 12.0).exp2();
    let third = (4.0f64 / 12.0).exp2();
    for (name, notes) in [
        ("B1 5th", vec![61.7, 61.7 * fifth]),
        ("E2 5th", vec![82.4, 82.4 * fifth]),
        ("E2 5th+8", vec![82.4, 82.4 * fifth, 164.8]),
        ("E2 maj", vec![82.4, 82.4 * fifth, 164.8, 164.8 * third]),
    ] {
        let tones: Vec<(f64, f32)> =
            notes.iter().flat_map(|f| (1..=4).map(move |k| (f * k as f64, 0.08 / k as f32))).collect();
        let input = signal::chord(&tones, SR, 120_000);
        let mut row = format!("{name:<8}");
        for shift in [0, -2, -5, -12] {
            let r = (f64::from(shift) / 12.0).exp2();
            let out: Vec<f64> = notes.iter().map(|f| f * r).collect();
            for quality in [false, true] {
                let db = inharmonic_multi_db(&shifted(&input, shift, quality)[..48_000], &out, 1000.0);
                row += &format!(" {db:7.1}");
            }
        }
        println!("{row}");
    }
}

#[test]
fn the_latency_is_reported_and_follows_the_mode() {
    let mut r = runner(&[]);
    assert_eq!(r.latency(), TIGHT);
    assert_eq!(DropPedal::max_latency(&Config { sample_rate: SR, max_block: 512 }), QUALITY);
    let input = note(110.0, 4800);
    process(&mut r, &input, 64, |r, start| {
        if start == 1024 {
            r.set_param(at("mode"), 1.0);
        }
    });
    assert_eq!(r.latency(), QUALITY);
}

#[test]
fn dry_and_unshifted_are_the_input_delayed_by_the_latency() {
    let input = signal::noise(7, 0.5, 24_000);
    for (params, latency) in [
        (vec![("mix", 0.0)], TIGHT),
        (vec![("mix", 0.0), ("mode", 1.0), ("shift", -12.0)], QUALITY),
        (vec![("shift", 0.0)], TIGHT),
        (vec![("shift", 0.0), ("mode", 1.0)], QUALITY),
    ] {
        let out = render(&input, &params, 64);
        let n = latency as usize;
        assert_eq!(&out[n..], &input[..input.len() - n], "{params:?}");
        assert!(out[..n].iter().all(|y| *y == 0.0));
    }
}

#[test]
fn the_shifted_signal_lines_up_with_the_dry_one() {
    // A note starting after silence: its shifted onset lands within half
    // a period of the reported latency
    for (quality, latency) in [(false, TIGHT), (true, QUALITY)] {
        let mut input = vec![0.0f32; 12_000];
        input[4800..].copy_from_slice(&note(110.0, 7200));
        let out = render(&input, &[("shift", -5.0), ("mode", mode_value(quality))], 64);
        let onset = out.iter().position(|y| y.abs() > 1e-6).expect("onset");
        let delay = onset as i64 - 4800;
        let half = (SR / 110.0 / 2.0) as i64;
        assert!((delay - i64::from(latency)).abs() <= half, "quality {quality}: {delay}");
    }
}

/// Fundamental of `x` in windows of 0.2 s, starting every 0.1 s
fn pitch_track(x: &[f32], lo: f64, hi: f64) -> Vec<f64> {
    (0..)
        .map(|k| k * 4800)
        .take_while(|s| s + 9600 <= x.len())
        .map(|s| peak_frequency(&x[s..s + 9600], lo, hi))
        .collect()
}

#[test]
fn the_dive_glides_down_and_back() {
    let input = signal::sine(220.0, 0.3, SR, 96_000);
    let params = [("shift", 0.0), ("dive_to", -12.0), ("dive_time", 200.0), ("mode", 1.0)];
    let mut r = runner(&params);
    // Dive from 0.5 s to 1.2 s
    let out = process(&mut r, &input, 64, |r, start| {
        if start == 24_000 {
            r.set_param(at("dive"), 1.0);
        }
        if start == 57_600 {
            r.set_param(at("dive"), 0.0);
        }
    });
    let track = pitch_track(&out, 100.0, 240.0);
    // Windows wholly before, inside the dive after its glide, and after
    // the return
    let before = &track[1..3];
    let down = &track[8..10];
    let after = &track[16..18];
    for f in before.iter().chain(after) {
        assert!(cents(*f, 220.0).abs() < 5.0, "{track:?}");
    }
    for f in down {
        assert!(cents(*f, 110.0).abs() < 5.0, "{track:?}");
    }
    // Halfway through the glide down, between the two
    let mid = peak_frequency(&out[24_000 + 4800 - 1200..24_000 + 4800 + 1200], 100.0, 240.0);
    assert!(mid > 120.0 && mid < 210.0, "{mid}");
}

#[test]
fn silence_stays_silent() {
    let zeros = vec![0.0f32; 24_000];
    let out = process(&mut runner(&[("shift", -12.0)]), &zeros, 64, |r, start| {
        if start == 6400 {
            r.set_param(at("mode"), 1.0);
            r.set_param(at("dive"), 1.0);
        }
    });
    assert!(out.iter().all(|y| *y == 0.0));
}

/// Every control moved every 960 samples
fn automate(r: &mut Runner<DropPedal>, start: usize) {
    if !start.is_multiple_of(960) {
        return;
    }
    let k = start / 960;
    let shifts = [-2.0, -12.0, 7.0, -5.0, -24.0, 0.0, 12.0];
    r.set_param(at("shift"), shifts[k % shifts.len()]);
    r.set_param(at("dive_to"), shifts[(k + 3) % shifts.len()]);
    r.set_param(at("mix"), ((k * 37) % 101) as f32);
    r.set_param(at("dive_time"), [10.0, 80.0, 400.0][k % 3]);
    r.set_param(at("dive"), (k / 2 % 2) as f32);
    if k.is_multiple_of(5) {
        r.set_param(at("mode"), (k / 5 % 2) as f32);
    }
}

#[test]
fn output_is_block_size_independent_under_automation() {
    let a = signal::pluck(82.4, 0.3, 1.5, SR, 48_000);
    let b = signal::pluck(123.5, 0.2, 1.5, SR, 48_000);
    let input: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
    let reference = process(&mut runner(&[]), &input, 960, automate);
    assert!(reference.iter().all(|y| y.is_finite() && y.abs() < 2.0));
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
    // A sine through the shifter, a control thrown at three phases: the
    // output stays as smooth as a sine at the highest pitch it reaches
    let freq = 221.25;
    type Case = (&'static [(&'static str, f32)], (&'static str, f32), f64);
    let cases: [Case; 6] = [
        (&[("shift", -2.0)], ("shift", -12.0), 1.0),
        (&[("shift", -12.0)], ("shift", 7.0), 1.5),
        (&[("shift", 0.0), ("dive_to", -24.0)], ("dive", 1.0), 1.0),
        (&[("shift", -5.0)], ("mode", 1.0), 1.0),
        (&[("shift", -5.0), ("mode", 1.0)], ("mode", 0.0), 1.0),
        (&[("shift", -12.0), ("mix", 0.0)], ("mix", 100.0), 1.0),
    ];
    for (before, (symbol, value), most) in cases {
        for at_sample in [9600, 9618, 9636] {
            let input = signal::sine(freq, 0.3, SR, 24_000);
            let out = process(&mut runner(before), &input, 6, |r, start| {
                if start == at_sample {
                    r.set_param(at(symbol), value);
                }
            });
            let rough = roughness(&out, freq * most, 4800);
            assert!(rough < 1.3, "{symbol} to {value} at {at_sample}: {rough}");
        }
    }
}

#[test]
fn the_splices_do_not_click_on_a_sine() {
    // Splices every few periods on a pure tone, every shift: smooth
    for shift in [-24, -12, -5, -1, 1, 5, 12] {
        let freq = 220.0;
        let input = signal::sine(freq, 0.3, SR, 24_000);
        let out = render(&input, &[("shift", shift as f32), ("mode", 1.0)], 64);
        let f = freq * (f64::from(shift) / 12.0).exp2();
        let rough = roughness(&out, f, 4800);
        assert!(rough < 1.1, "{shift}: {rough}");
    }
}

#[test]
fn golden_presets() {
    let a = signal::pluck(73.4, 0.3, 1.5, SR, 96_000);
    let b = signal::pluck(110.0, 0.2, 1.2, SR, 96_000);
    let mut input: Vec<f32> = a.iter().zip(&b).map(|(x, y)| x + y).collect();
    input[72_000..].fill(0.0);
    for preset in DROP.presets {
        let mut r = runner(preset.values);
        // The dive presets with the dive pressed in the middle
        let out = process(&mut r, &input, 128, |r, start| {
            if start == 24_064 {
                r.set_param(at("dive"), 1.0);
            }
            if start == 48_000 {
                r.set_param(at("dive"), 0.0);
            }
        });
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner_and_reports_the_latency() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<DropPedal>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of DropPedal, described by DROP
    let mut p = unsafe { host.instantiate(&d.raw, &DROP, true) }.expect("instance");
    let params = [("shift", -5.0), ("mix", 80.0), ("mode", 1.0)];
    for (s, v) in params {
        p.set(at(s), v);
    }
    p.activate();
    let input = signal::pluck(55.0, 0.4, 1.5, SR, 256 * 40);
    let mut out = Vec::new();
    for chunk in input.chunks(256) {
        p.input(0).copy_from_slice(chunk);
        orx_test::rt::no_alloc(|| p.run(256));
        out.extend_from_slice(p.output(0));
    }
    assert_eq!(out, render(&input, &params, 256));
    assert_eq!(p.get(at("latency")), QUALITY as f32);
}
