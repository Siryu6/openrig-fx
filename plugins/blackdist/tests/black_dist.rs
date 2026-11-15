//! OR Black Dist: conformance, the small-signal response against the
//! analytic response of the circuit, level and distortion against the
//! Distortion control, aliasing, slew limiting, the voicings, click-free
//! changes, golden files, and the LV2 build of the same code.
#![allow(clippy::expect_used)]

use orx_black_dist::circuit;
use orx_black_dist::{BLACK_DIST, BlackDist, MAKEUP_DB, Voicing};
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
    BLACK_DIST.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<BlackDist> {
    let mut r = Runner::<BlackDist>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

fn process(
    r: &mut Runner<BlackDist>,
    input: &[f32],
    block: usize,
    mut before: impl FnMut(&mut Runner<BlackDist>, usize),
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

/// Small-signal response of the circuit at `f` from its equations: the
/// op-amp's closed loop with its single-pole open loop, the clipper node
/// with the diodes at their rest conductance, the filter, the output
/// coupling
fn analytic(f: f64, distortion: f32, tone: f32, voicing: Voicing) -> f64 {
    use circuit::*;
    let jw = C(0.0, TAU * f);
    let series = |r: f64, c: f64| C::re(r).add(jw.mul(C::re(c)).inv());
    // Gain: 1 + Z(feedback) / Z(legs), limited by the open loop ωt/s
    let y_feedback = C::re(1.0 / distortion_resistance(distortion)).add(jw.mul(C::re(C_FEEDBACK)));
    let y_legs = LEGS.iter().fold(C::re(0.0), |acc, (r, c)| acc.add(series(*r, *c).inv()));
    let ideal = C::re(1.0).add(y_legs.div(y_feedback));
    let gain = jw.div(C::re(OPAMP_GBW)).add(ideal.inv()).inv();
    // Clipper node: 1 kΩ in, diodes and the filter branch out; the output
    // across the filter capacitor
    let branch = series(filter_resistance(tone), C_FILTER);
    let g_clip = 1.0 / R_CLIP;
    let node =
        C::re(g_clip).div(C::re(g_clip + diodes(voicing).map_or(0.0, |d| d.rest_conductance())).add(branch.inv()));
    let cap = jw.mul(C::re(C_FILTER)).inv().div(branch);
    let coupling = jw.div(jw.add(C::re(TAU * COUPLING_HZ)));
    gain.mul(node).mul(cap).mul(coupling).abs() * 10f64.powf(f64::from(MAKEUP_DB + voicing.compensation_db()) / 20.0)
}

#[test]
fn conformance() {
    orx_test::conformance::check::<BlackDist>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&BLACK_DIST, dir().join("ports.lock"));
}

#[test]
fn small_signals_follow_the_analytic_circuit() {
    // A quiet multitone keeps the diodes at their rest conductance and
    // the op-amp below its slew rate: the measured response is the
    // circuit's linear response, within the warping of the trapezoidal
    // rule at the top (at eight times the rate)
    let freqs = [100.0, 250.0, 500.0, 1000.0, 1500.0, 2000.0, 3000.0, 5000.0, 8000.0];
    let amp = 1e-6;
    let input: Vec<f32> =
        (0..72_000).map(|i| freqs.iter().map(|f| (amp * (TAU * f * i as f64 / SR).sin()) as f32).sum()).collect();
    let cases = [
        (0.0, 0.0, Voicing::Silicon),
        (100.0, 50.0, Voicing::Silicon),
        (50.0, 100.0, Voicing::Silicon),
        (70.0, 30.0, Voicing::Led),
        (40.0, 80.0, Voicing::None),
    ];
    for (distortion, tone, voicing) in cases {
        let params = [("distortion", distortion), ("tone", tone), ("voicing", voicing as i32 as f32)];
        let out = render(&input, &params, 128);
        let tail = &out[24_000..];
        let mut worst = 0.0f64;
        for f in freqs {
            let (re, im) = bin(tail, f);
            let measured = 20.0 * (re.hypot(im) / amp).log10();
            let expected = 20.0 * analytic(f, 0.01 * distortion, 0.01 * tone, voicing).log10();
            let err = measured - expected;
            worst = worst.max(err.abs());
            assert!(
                err.abs() < 0.2,
                "{voicing:?} distortion {distortion} tone {tone} at {f} Hz: {measured:.2} dB, circuit {expected:.2}"
            );
        }
        eprintln!("{voicing:?} distortion {distortion} tone {tone}: worst {worst:.4} dB");
    }
}

#[test]
fn the_bandwidth_closes_as_the_gain_rises() {
    // Gain-bandwidth: the closed loop's upper corner falls with the gain
    let corner = |distortion: f32| {
        let gain = |f: f64| analytic(f, distortion, 1.0, Voicing::Silicon);
        let peak = (0..80).map(|k| gain(100.0 * 1.06f64.powi(k))).fold(0.0, f64::max);
        (0..120).map(|k| 100.0 * 1.06f64.powi(k)).find(|f| *f > 1000.0 && gain(*f) < peak / 2f64.sqrt()).unwrap_or(1e9)
    };
    let (low, high) = (corner(0.5), corner(1.0));
    assert!(high < low && high < 5000.0, "{low} Hz at half, {high} Hz at full");
}

#[test]
fn level_matches_the_bypass_at_the_middle_of_distortion() {
    // Every diode choice, with its compensation
    let input = guitar(48_000 * 2);
    for voicing in Voicing::ALL {
        let out = render(&input, &[("voicing", voicing as i32 as f32)], 128);
        let gain = metrics::db(metrics::rms(&out) / metrics::rms(&input));
        eprintln!("{voicing:?}: {gain:.2} dB");
        assert!(gain.abs() < 1.0, "{voicing:?}: {gain:.2} dB");
    }
}

#[test]
fn distortion_raises_harmonics_then_the_level_saturates() {
    let runs: Vec<(f64, f32)> = [0.0, 25.0, 50.0, 75.0, 100.0]
        .iter()
        .map(|d| {
            let out = steady(220, 0.1, &[("distortion", *d)]);
            (thd(&out, 220), metrics::rms(&out))
        })
        .collect();
    eprintln!("thd and rms against distortion: {runs:?}");
    for w in runs.windows(2) {
        assert!(w[0].0 < w[1].0 && w[0].1 < w[1].1, "{runs:?}");
    }
    // Unity gain at 0: clean
    assert!(runs[0].0 < -60.0, "{runs:?}");
    // Hard clipping: past the middle, the level barely moves
    assert!(metrics::db(runs[4].1 / runs[2].1) < 0.5, "{runs:?}");
}

#[test]
fn aliases_stay_low() {
    // 1979 Hz at full distortion, loud: a square wave with edges shorter
    // than a sample at the diodes, whose harmonics fold back at the
    // oversampled rate already (−45 dB at four times the rate, hence
    // eight)
    for voicing in Voicing::ALL {
        for (tone, limit) in [(50.0, -60.0), (100.0, -55.0)] {
            let params = [("distortion", 100.0), ("tone", tone), ("voicing", voicing as i32 as f32)];
            let (_, aliases) = spectrum(&steady(1979, 0.5, &params), 1979);
            eprintln!("{voicing:?} tone {tone}: aliases {aliases:.1} dB");
            assert!(aliases < limit, "{voicing:?} tone {tone}: {aliases:.1} dB");
        }
    }
}

#[test]
fn the_voicings_are_symmetric() {
    for voicing in Voicing::ALL {
        let out = steady(220, 0.3, &[("voicing", voicing as i32 as f32)]);
        let (amps, _) = spectrum(&out, 220);
        let second = 20.0 * (amps[1] / amps[0]).log10();
        assert!(second < -100.0, "{voicing:?}: {second:.1} dB");
    }
}

/// Peak of the circuit's output (before the plugin's level) for a loud
/// note at full distortion and the brightest filter
fn clipped_peak(voicing: Voicing) -> f32 {
    let mut c = circuit::Circuit::new(SR);
    c.set_voicing(voicing);
    c.set_distortion(1.0);
    c.set_tone(1.0);
    signal::sine(220.0, 0.3, SR, 24_000).iter().skip(12_000).fold(0.0f32, |m, x| m.max(c.process(*x).abs()))
}

#[test]
fn each_diode_choice_clips_at_its_own_threshold() {
    // Silicon about 0.6 V, LEDs about three times higher, no diodes at
    // the op-amp's rails (3.5 V; the decimation filter's ringing on the
    // clipped edges adds up to a tenth or so to each peak)
    let peaks = Voicing::ALL.map(clipped_peak);
    eprintln!("clipped peaks: {peaks:?}");
    let [silicon, led, none] = peaks;
    assert!((0.5..1.0).contains(&silicon), "{silicon}");
    assert!((2.0..4.0).contains(&(led / silicon)), "{peaks:?}");
    assert!((3.0..1.2 * circuit::OPAMP_RAIL as f32).contains(&none), "{none}");
}

#[test]
fn leds_are_more_open() {
    // Level-matched, the LEDs keep more of the playing dynamics: a quiet
    // note comes out further below a loud one than through silicon
    let span = |v: Voicing| {
        let level = |amp: f32| metrics::rms(&steady(220, amp, &[("voicing", v as i32 as f32)]));
        metrics::db(level(0.3) / level(0.01))
    };
    let (silicon, led) = (span(Voicing::Silicon), span(Voicing::Led));
    eprintln!("loud over quiet: silicon {silicon:.1} dB, LED {led:.1} dB");
    assert!(led > silicon + 3.0);
}

#[test]
fn silence_stays_silent() {
    let zeros = vec![0.0f32; 24_000];
    let out = process(&mut runner(&[("voicing", 1.0), ("distortion", 100.0)]), &zeros, 64, |r, start| {
        if start == 6400 {
            r.set_param(at("voicing"), 0.0);
        }
    });
    assert!(out.iter().all(|y| *y == 0.0));
}

/// Every control moved every 960 samples, the voicing every third time
fn automate(r: &mut Runner<BlackDist>, start: usize) {
    if !start.is_multiple_of(960) {
        return;
    }
    let k = start / 960;
    for (j, symbol) in ["distortion", "tone", "level"].iter().enumerate() {
        let p = BLACK_DIST.ports[at(symbol)].as_param().expect("param");
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
        (&[("distortion", 0.0)], ("distortion", 100.0)),
        (&[("tone", 0.0)], ("tone", 100.0)),
        (&[("level", -24.0)], ("level", 12.0)),
        (&[("voicing", 0.0)], ("voicing", 1.0)),
        (&[("voicing", 0.0)], ("voicing", 2.0)),
        (&[("voicing", 2.0)], ("voicing", 1.0)),
    ];
    for (before, (symbol, value)) in cases {
        for at_sample in [9600, 9618, 9636] {
            let input = signal::sine(freq, 5e-5, SR, 24_000);
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
    for preset in BLACK_DIST.presets {
        let out = render(&input, preset.values, 128);
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<BlackDist>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of BlackDist, described by BLACK_DIST
    let mut p = unsafe { host.instantiate(&d.raw, &BLACK_DIST, true) }.expect("instance");
    let params = [("distortion", 70.0), ("tone", 40.0), ("voicing", 1.0), ("level", -3.0)];
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
