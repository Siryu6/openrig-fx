//! OR Green Drive: conformance, the small-signal response against the
//! analytic response of the circuit, level and distortion against drive,
//! aliasing, the voicings, click-free changes, golden files, and the LV2
//! build of the same code.
#![allow(clippy::expect_used)]

use orx_dsp::circuit::{Diode, DiodePair};
use orx_green_drive::circuit;
use orx_green_drive::{GREEN_DRIVE, GreenDrive, MAKEUP_DB, Voicing};
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
    GREEN_DRIVE.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<GreenDrive> {
    let mut r = Runner::<GreenDrive>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

fn process(
    r: &mut Runner<GreenDrive>,
    input: &[f32],
    block: usize,
    mut before: impl FnMut(&mut Runner<GreenDrive>, usize),
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

/// Complex amplitude of `f` (whole hertz) in one second of `x`
fn bin(x: &[f32], f: f64) -> (f64, f64) {
    assert_eq!(x.len(), SR as usize);
    let w = TAU * f / SR;
    let (re, im) = x.iter().enumerate().fold((0.0, 0.0), |(re, im), (i, v)| {
        let p = w * i as f64;
        (re + f64::from(*v) * p.cos(), im - f64::from(*v) * p.sin())
    });
    let n = x.len() as f64;
    (2.0 * re / n, 2.0 * im / n)
}

/// Amplitude of each harmonic of `f0` (whole hertz) below Nyquist in one
/// second of `x`, and the power of what is neither a harmonic nor DC,
/// relative to the total, dB
fn spectrum(x: &[f32], f0: u32) -> (Vec<f64>, f64) {
    let n = x.len() as f64;
    let mean = x.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let total: f64 = x.iter().map(|v| (f64::from(*v) - mean).powi(2)).sum::<f64>() / n;
    let mut amps = Vec::new();
    let mut k = 1;
    while f64::from(k * f0) < SR / 2.0 {
        let (re, im) = bin(x, f64::from(k * f0));
        amps.push((re * re + im * im).sqrt());
        k += 1;
    }
    let harmonic: f64 = amps.iter().map(|a| a * a / 2.0).sum();
    let rest = (total - harmonic).max(1e-30);
    (amps, 10.0 * (rest / total).log10())
}

/// Power of the harmonics above the fundamental relative to it, dB
fn thd(out: &[f32], f0: u32) -> f64 {
    let (amps, _) = spectrum(out, f0);
    let above: f64 = amps[1..].iter().map(|a| a * a).sum();
    10.0 * (above / (amps[0] * amps[0])).log10()
}

/// One second of `params` on a sine at `f0`, after half a second
fn steady(f0: u32, amp: f32, params: &[(&str, f32)]) -> Vec<f32> {
    let tone = signal::sine(f64::from(f0), amp, SR, 72_000);
    render(&tone, params, 128)[24_000..].to_vec()
}

/// Complex numbers, enough for the analytic response
#[derive(Debug, Clone, Copy)]
struct C(f64, f64);

impl C {
    fn re(x: f64) -> Self {
        C(x, 0.0)
    }
    fn add(self, o: C) -> C {
        C(self.0 + o.0, self.1 + o.1)
    }
    fn sub(self, o: C) -> C {
        C(self.0 - o.0, self.1 - o.1)
    }
    fn mul(self, o: C) -> C {
        C(self.0 * o.0 - self.1 * o.1, self.0 * o.1 + self.1 * o.0)
    }
    fn inv(self) -> C {
        let d = self.0 * self.0 + self.1 * self.1;
        C(self.0 / d, -self.1 / d)
    }
    fn div(self, o: C) -> C {
        self.mul(o.inv())
    }
    fn abs(self) -> f64 {
        self.0.hypot(self.1)
    }
}

/// Small-signal response of the circuit at `f` from its nodal equations:
/// the clipping stage (the diodes as their conductance at rest), the tone
/// network solved as a linear system, the output coupling, the level
fn analytic(f: f64, drive: f32, tone: f32, voicing: Voicing, tight: f32) -> f64 {
    use circuit::*;
    let jw = C(0.0, TAU * f);
    let v = circuit::Values::of(voicing);
    // Clipping stage: 1 + Z(feedback) / Z(gain leg)
    let g_diodes = DiodePair::new(Diode::SILICON, v.forward, v.reverse).rest_conductance();
    let y_feedback = C::re(1.0 / drive_resistance(drive) + g_diodes).add(jw.mul(C::re(C_FEEDBACK)));
    let z_leg = C::re(R_GAIN).add(jw.mul(C::re(v.c_gain)).inv());
    let clip = C::re(1.0).add(y_feedback.mul(z_leg).inv());
    // Tone network: unknowns A (fixed low pass), P (both op-amp inputs),
    // W (wiper); Gaussian elimination of a 3×3 system
    let t = f64::from(tone);
    let (ga, gb) = (1.0 / (t * POT_TONE).max(1.0), 1.0 / ((1.0 - t) * POT_TONE).max(1.0));
    let (g7, g8) = (1.0 / R_TONE_LP, 1.0 / R_TONE_IN);
    let y6 = C::re(R_TONE_WIPER).add(jw.mul(C::re(C_TONE_WIPER)).inv()).inv();
    let mut m = [
        [C::re(g7 + g8).add(jw.mul(C::re(C_TONE_LP))), C::re(-g8), C::re(0.0), C::re(g7)],
        [C::re(-g8), C::re(g8 + ga), C::re(-ga), C::re(0.0)],
        [C::re(0.0), C::re(-(ga + gb)), C::re(ga + gb).add(y6), C::re(0.0)],
    ];
    for col in 0..3 {
        for row in col + 1..3 {
            let k = m[row][col].div(m[col][col]);
            let pivot = m[col];
            for (a, b) in m[row].iter_mut().zip(pivot).skip(col) {
                *a = a.sub(k.mul(b));
            }
        }
    }
    let mut x = [C::re(0.0); 3];
    for row in (0..3).rev() {
        let mut acc = m[row][3];
        for c in row + 1..3 {
            acc = acc.sub(m[row][c].mul(x[c]));
        }
        x[row] = acc.div(m[row][row]);
    }
    let (p, w) = (x[1], x[2]);
    let tone_out = p.add(C::re(R_TONE_FEEDBACK * gb).mul(p.sub(w)));
    // Output coupling: a first-order high pass; Tight: two
    let coupling = jw.div(jw.add(C::re(TAU * COUPLING_HZ)));
    let rc = jw.mul(C::re(tight_resistance(tight) * C_TIGHT));
    let section = rc.div(rc.add(C::re(1.0)));
    clip.mul(tone_out).mul(coupling).mul(section).mul(section).abs() * 10f64.powf(f64::from(MAKEUP_DB) / 20.0)
}

#[test]
fn conformance() {
    orx_test::conformance::check::<GreenDrive>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&GREEN_DRIVE, dir().join("ports.lock"));
}

#[test]
fn small_signals_follow_the_analytic_circuit() {
    // A quiet multitone keeps the diodes at their rest conductance: the
    // measured response is the circuit's linear response, within the
    // bilinear transform's warping at the top (at four times the rate)
    let freqs = [60.0, 100.0, 250.0, 500.0, 720.0, 1000.0, 1500.0, 2000.0, 3000.0, 5000.0, 8000.0];
    let amp = 1e-4;
    let input: Vec<f32> =
        (0..72_000).map(|i| freqs.iter().map(|f| (amp * (TAU * f * i as f64 / SR).sin()) as f32).sum()).collect();
    let cases = [
        (0.0, 0.0, Voicing::Classic, 5.0),
        (100.0, 50.0, Voicing::Classic, 5.0),
        (50.0, 100.0, Voicing::Classic, 5.0),
        (70.0, 30.0, Voicing::Fat, 5.0),
        (60.0, 70.0, Voicing::Asymmetric, 5.0),
        (0.0, 55.0, Voicing::Classic, 250.0),
        (80.0, 50.0, Voicing::Classic, 700.0),
    ];
    for (drive, tone, voicing, tight) in cases {
        let params = [("drive", drive), ("tone", tone), ("voicing", voicing as i32 as f32), ("tight", tight)];
        let out = render(&input, &params, 128);
        let tail = &out[24_000..];
        let mut worst = 0.0f64;
        for f in freqs {
            let (re, im) = bin(tail, f);
            let measured = 20.0 * (re.hypot(im) / amp).log10();
            let expected = 20.0 * analytic(f, 0.01 * drive, 0.01 * tone, voicing, tight).log10();
            let err = measured - expected;
            worst = worst.max(err.abs());
            assert!(
                err.abs() < 0.1,
                "{voicing:?} drive {drive} tone {tone} tight {tight} at {f} Hz: {measured:.2} dB, circuit {expected:.2}"
            );
        }
        eprintln!("{voicing:?} drive {drive} tone {tone} tight {tight}: worst {worst:.4} dB");
    }
}

#[test]
fn the_response_humps_in_the_mids() {
    // The 720 Hz gain leg against the 723 Hz low pass: at full drive and
    // the middle of Tone, a peak between 500 Hz and 1.5 kHz, 6 dB above
    // 100 Hz
    let curve: Vec<(f64, f64)> = (0..60)
        .map(|k| 50.0 * 1.1f64.powi(k))
        .map(|f| (f, analytic(f, 1.0, 0.5, Voicing::Classic, circuit::TIGHT_MIN_HZ)))
        .collect();
    let (peak_f, peak) = curve.iter().fold((0.0, 0.0), |m, (f, a)| if *a > m.1 { (*f, *a) } else { m });
    let low = analytic(100.0, 1.0, 0.5, Voicing::Classic, circuit::TIGHT_MIN_HZ);
    assert!((500.0..1500.0).contains(&peak_f), "{peak_f}");
    assert!(20.0 * (peak / low).log10() > 6.0);
}

#[test]
fn tight_takes_the_lows_out_of_the_boost() {
    // The boost in front of a high-gain amp: drive at 0. At 250 Hz, a low
    // E (82 Hz) loses about 20 dB more than with Tight at its minimum,
    // 1 kHz under 1 dB
    let min = circuit::TIGHT_MIN_HZ;
    let rel = |f: f64, tight: f32| {
        20.0 * (analytic(f, 0.0, 0.55, Voicing::Classic, tight) / analytic(f, 0.0, 0.55, Voicing::Classic, min)).log10()
    };
    assert!(rel(82.0, 250.0) < -19.0, "{}", rel(82.0, 250.0));
    assert!(rel(1000.0, 250.0) > -1.0, "{}", rel(1000.0, 250.0));
    // At its minimum, out of the way of a drop-tuned low note (41 Hz):
    // the two sections take under 0.2 dB
    let x = (41.0 / f64::from(min)).powi(2);
    assert!(10.0 * (x / (1.0 + x)).log10() > -0.2);
    // Loud, through the clipping stage at the middle of Drive: a low note
    // with a high one; Tight lowers the low note's share of the output
    let input: Vec<f32> = (0..72_000)
        .map(|i| (0.2 * ((TAU * 82.0 * i as f64 / SR).sin() + (TAU * 660.0 * i as f64 / SR).sin())) as f32)
        .collect();
    let share = |tight: f32| {
        let out = render(&input, &[("drive", 50.0), ("tight", tight)], 128);
        let tail = &out[24_000..];
        let (a, b) = (bin(tail, 82.0), bin(tail, 660.0));
        20.0 * (a.0.hypot(a.1) / b.0.hypot(b.1)).log10()
    };
    let (open, tight) = (share(5.0), share(250.0));
    eprintln!("82 Hz against 660 Hz: {open:.1} dB open, {tight:.1} dB tight");
    assert!(tight < open - 15.0);
}

#[test]
fn level_matches_the_bypass_at_the_middle_of_drive() {
    let input = guitar(48_000 * 2);
    let gain = metrics::db(metrics::rms(&render(&input, &[], 128)) / metrics::rms(&input));
    assert!(gain.abs() < 1.0, "{gain:.2} dB");
}

#[test]
fn drive_raises_distortion_and_level() {
    let runs: Vec<(f64, f32)> = [0.0, 25.0, 50.0, 75.0, 100.0]
        .iter()
        .map(|d| {
            let out = steady(220, 0.1, &[("drive", *d)]);
            (thd(&out, 220), metrics::rms(&out))
        })
        .collect();
    for w in runs.windows(2) {
        assert!(w[0].0 < w[1].0 && w[0].1 < w[1].1, "{runs:?}");
    }
    eprintln!("thd and rms against drive: {runs:?}");
    // Soft clipping: the level grows far slower than the 20 dB of gain
    let span = metrics::db(runs[4].1 / runs[0].1);
    assert!(span < 8.0, "{span}");
}

#[test]
fn aliases_stay_low() {
    // 1979 Hz at full drive and the brightest tone, loud
    for voicing in Voicing::ALL {
        let out = steady(1979, 0.5, &[("drive", 100.0), ("tone", 100.0), ("voicing", voicing as i32 as f32)]);
        let (_, aliases) = spectrum(&out, 1979);
        eprintln!("{voicing:?}: aliases {aliases:.1} dB");
        assert!(aliases < -60.0, "{voicing:?}: {aliases:.1} dB");
    }
}

#[test]
fn only_the_asymmetric_voicing_has_even_harmonics() {
    for voicing in Voicing::ALL {
        let out = steady(220, 0.3, &[("voicing", voicing as i32 as f32)]);
        let (amps, _) = spectrum(&out, 220);
        let second = 20.0 * (amps[1] / amps[0]).log10();
        if voicing == Voicing::Asymmetric {
            assert!(second > -30.0, "{voicing:?}: {second:.1} dB");
        } else {
            assert!(second < -100.0, "{voicing:?}: {second:.1} dB");
        }
    }
}

#[test]
fn silence_stays_silent() {
    let zeros = vec![0.0f32; 24_000];
    let out = process(&mut runner(&[("voicing", 2.0), ("drive", 100.0)]), &zeros, 64, |r, start| {
        if start == 6400 {
            r.set_param(at("voicing"), 1.0);
        }
    });
    assert!(out.iter().all(|y| *y == 0.0));
}

/// Every control moved every 960 samples, the voicing every third time
fn automate(r: &mut Runner<GreenDrive>, start: usize) {
    if !start.is_multiple_of(960) {
        return;
    }
    let k = start / 960;
    for (j, symbol) in ["drive", "tone", "level", "tight"].iter().enumerate() {
        let p = GREEN_DRIVE.ports[at(symbol)].as_param().expect("param");
        let t = ((k * 7 + j * 3) % 11) as f32 / 10.0;
        r.set_param(at(symbol), p.min + (p.max - p.min) * t);
    }
    if k.is_multiple_of(3) {
        r.set_param(at("voicing"), (k / 3 % 3) as f32);
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
    // A quiet note through nearly linear settings, a control thrown from
    // one end to the other at three phases of the note (the circuit's
    // filters shift it, so one of them is near a peak): the output stays
    // a smooth sine
    let freq = 221.25;
    type Case = (&'static [(&'static str, f32)], (&'static str, f32));
    let cases: [Case; 6] = [
        (&[("drive", 0.0)], ("drive", 100.0)),
        (&[("tight", 5.0)], ("tight", 700.0)),
        (&[("tone", 0.0)], ("tone", 100.0)),
        (&[("level", -24.0)], ("level", 12.0)),
        (&[("voicing", 0.0)], ("voicing", 1.0)),
        (&[("voicing", 0.0)], ("voicing", 2.0)),
    ];
    for (before, (symbol, value)) in cases {
        for at_sample in [9600, 9618, 9636] {
            let input = signal::sine(freq, 0.002, SR, 24_000);
            let out = process(&mut runner(before), &input, 6, |r, start| {
                if start == at_sample {
                    r.set_param(at(symbol), value);
                }
            });
            let rough = roughness(&out, freq, 4800);
            assert!(rough < 1.2, "{symbol} to {value} at {at_sample}: {rough}");
        }
    }
}

#[test]
fn golden_presets() {
    let mut input = guitar(48_000 * 2);
    input[72_000..].fill(0.0);
    for preset in GREEN_DRIVE.presets {
        let out = render(&input, preset.values, 128);
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<GreenDrive>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of GreenDrive, described by GREEN_DRIVE
    let mut p = unsafe { host.instantiate(&d.raw, &GREEN_DRIVE, true) }.expect("instance");
    let params = [("drive", 70.0), ("tone", 40.0), ("voicing", 2.0), ("level", -3.0)];
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
