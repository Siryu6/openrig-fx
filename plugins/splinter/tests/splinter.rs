//! OR Splinter: conformance, the bit-transparent idle path, captures on
//! the host grid and in free time, repeats (count, decay, pitch step,
//! reverse), the tape stop and start, the chop, the crush, chaos, the
//! grid stream, click-free edges, golden files and the LV2 build.
#![allow(clippy::expect_used)]

use orx_plugin::process::{NoStreams, StreamSink};
use orx_plugin::runner::Runner;
use orx_plugin::{Config, Event, EventKind, Position};
use orx_splinter::{GRID_CELLS, SPLINTER, Splinter};
use orx_test::{metrics, signal};
use std::path::PathBuf;

orx_test::install_rt_check!();

const SR: f64 = 48_000.0;
const BLOCK: usize = 128;
/// Samples per quarter note at 120 BPM
const Q120: usize = 24_000;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn at(symbol: &str) -> usize {
    SPLINTER.port_index(symbol).expect("port symbol")
}

fn runner(params: &[(&str, f32)]) -> Runner<Splinter> {
    let mut r = Runner::<Splinter>::new(Config { sample_rate: SR, max_block: 512 });
    for (s, v) in params {
        r.set_param(at(s), *v);
    }
    r.activate();
    r
}

/// The transport a rolling host sends at sample `start`
fn rolling(bpm: f32, start: usize) -> Event {
    let quarters = start as f64 * f64::from(bpm) / 60.0 / SR;
    let position = Position {
        bpm: Some(bpm),
        speed: Some(1.0),
        bar_beat: Some(quarters.rem_euclid(4.0) as f32),
        beats_per_bar: Some(4.0),
        beat_unit: Some(4),
    };
    Event { frame: 0, kind: EventKind::Position(position) }
}

/// The transport OpenRig sends: a tempo, and a bar position that never
/// moves
fn openrig(bpm: f32) -> Event {
    let position = Position {
        bpm: Some(bpm),
        speed: Some(1.0),
        bar_beat: Some(0.0),
        beats_per_bar: Some(4.0),
        beat_unit: Some(4),
    };
    Event { frame: 0, kind: EventKind::Position(position) }
}

fn process_with(
    r: &mut Runner<Splinter>,
    input: &[f32],
    block: usize,
    sink: &mut dyn StreamSink,
    mut before: impl FnMut(&mut Runner<Splinter>, usize),
) -> Vec<f32> {
    let mut out = vec![0.0; input.len()];
    let mut start = 0;
    while start < input.len() {
        let n = block.min(input.len() - start);
        before(r, start);
        r.process(n, &[&input[start..start + n]], &mut [&mut out[start..start + n]], sink);
        start += n;
    }
    out
}

fn process(r: &mut Runner<Splinter>, input: &[f32], before: impl FnMut(&mut Runner<Splinter>, usize)) -> Vec<f32> {
    process_with(r, input, BLOCK, &mut NoStreams, before)
}

/// HOLD down at `on` and up at `off` (block starts)
fn hold(on: usize, off: usize) -> impl FnMut(&mut Runner<Splinter>, usize) {
    assert!(on.is_multiple_of(BLOCK) && off.is_multiple_of(BLOCK), "HOLD moves on block starts");
    move |r, start| {
        if start == on {
            r.set_param(at("hold"), 1.0);
        }
        if start == off {
            r.set_param(at("hold"), 0.0);
        }
    }
}

/// Noise: every slice is unique, so a repeat is found where it plays
fn noise(len: usize) -> Vec<f32> {
    signal::noise(11, 0.5, len)
}

fn assert_close(a: &[f32], b: &[f32], tol: f32, what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: lengths");
    let d = metrics::max_diff(a, b);
    assert!(d <= tol, "{what}: differs by {d:e}");
}

/// Largest step between neighbouring samples
fn max_step(x: &[f32]) -> f32 {
    x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
}

#[test]
fn conformance() {
    orx_test::conformance::check::<Splinter>();
}

#[test]
fn ports_are_locked() {
    orx_test::ports_lock::check(&SPLINTER, dir().join("ports.lock"));
}

#[test]
fn idle_is_the_input_bit_for_bit() {
    let input = signal::pluck(110.0, 0.8, 1.0, SR, 48_000);
    // Chop and crush set to apply while held: nothing until HOLD
    for params in [
        &[][..],
        &[("pattern", 6.0), ("bits", 4.0), ("rate", 2000.0), ("chaos", 100.0)][..],
        &[("action", 2.0), ("tempo", 300.0)][..],
    ] {
        let mut r = runner(params);
        let out = process(&mut r, &input, |r, s| {
            r.push_event(rolling(133.0, s));
        });
        assert_eq!(out, input, "{params:?}");
    }
}

#[test]
fn a_late_press_starts_on_the_line_just_passed() {
    let input = noise(Q120 * 6);
    let len = Q120 / 2; // 1/8
    // 640 samples (13 ms) after beat 3
    let press = 2 * Q120 + 640;
    let c = 2 * Q120;
    let mut r = runner(&[("slice", 7.0)]);
    let out = process(&mut r, &input, |r, s| {
        r.push_event(rolling(120.0, s));
        hold(press, 5 * Q120 + 64)(r, s);
    });
    // The slice itself plays live, then repeats from its start
    assert_eq!(out[..c + len], input[..c + len]);
    let fade = 144;
    for n in 1..5 {
        let at = c + n * len;
        assert_close(&out[at + fade..at + len], &input[c + fade..c + len], 1e-6, &format!("repeat {n}"));
    }
}

#[test]
fn an_early_press_waits_for_the_next_line() {
    let input = noise(Q120 * 6);
    let len = Q120 / 4; // 1/16
    // 3136 samples into a 1/16 of 6000: waits for the next line
    let press = Q120 + 3_136;
    let c = Q120 + len;
    let mut r = runner(&[("slice", 10.0)]);
    let out = process(&mut r, &input, |r, s| {
        r.push_event(rolling(120.0, s));
        hold(press, 4 * Q120)(r, s);
    });
    assert_eq!(out[..c + len], input[..c + len]);
    assert_close(&out[c + len + 144..c + 2 * len], &input[c + 144..c + len], 1e-6, "first repeat");
    assert_close(&out[c + 3 * len + 144..c + 4 * len], &input[c + 144..c + len], 1e-6, "third repeat");
}

#[test]
fn without_a_host_bar_the_press_is_beat_one_at_the_host_tempo() {
    let input = noise(Q120 * 4);
    // OpenRig's transport at 150 BPM: a 1/8 is 9600 samples
    let len = 9_600;
    let press = 10_240;
    for quantize in [1.0, 0.0] {
        let mut r = runner(&[("slice", 7.0), ("quantize", quantize)]);
        let out = process(&mut r, &input, |r, s| {
            r.push_event(openrig(150.0));
            hold(press, 3 * Q120 + 64)(r, s);
        });
        assert_eq!(out[..press + len], input[..press + len]);
        let at = press + len;
        assert_close(&out[at + 144..at + len], &input[press + 144..press + len], 1e-6, "repeat");
    }
    // No transport at all: the Tempo control (100 BPM: a 1/4 is 28800)
    let mut r = runner(&[("slice", 4.0), ("tempo", 100.0)]);
    let out = process(&mut r, &input, hold(press, 3 * Q120 + 64));
    let len = 28_800;
    assert_close(&out[press + len + 144..press + 2 * len], &input[press + 144..press + len], 1e-6, "free tempo");
}

#[test]
fn repeats_stop_after_their_count_and_on_release() {
    let input = noise(Q120 * 6);
    let len = Q120 / 2;
    let press = 1_024;
    // Two repeats: back to the input after them, exactly
    let mut r = runner(&[("slice", 7.0), ("repeats", 2.0)]);
    let out = process(&mut r, &input, |r, s| {
        r.push_event(openrig(120.0));
        hold(press, 5 * Q120 + 64)(r, s);
    });
    let back = press + 3 * len + 144;
    assert_close(&out[press + 2 * len + 144..press + 3 * len], &input[press + 144..press + len], 1e-6, "repeat 2");
    assert_eq!(out[back..], input[back..]);

    // Released in the middle of the second repeat: back at once
    let release = press + len + len / 2 + 64;
    let release = release - release % BLOCK;
    let mut r = runner(&[("slice", 7.0)]);
    let out = process(&mut r, &input, |r, s| {
        r.push_event(openrig(120.0));
        hold(press, release)(r, s);
    });
    assert_eq!(out[release + 144..], input[release + 144..]);
}

#[test]
fn decay_and_pitch_step_shape_each_repeat() {
    // A 1/4 slice of a 300 Hz sine at 120 BPM
    let input = signal::sine(300.0, 0.5, SR, Q120 * 6);
    let len = Q120;
    let press = 1_024;
    let mut r = runner(&[("slice", 4.0), ("decay", 6.0), ("step", 12.0), ("repeats", 3.0)]);
    let out = process(&mut r, &input, hold(press, 5 * Q120 + 64));
    let repeat = |n: usize| &out[press + n * len + 500..press + (n + 1) * len - 500];
    // Repeat 1 at full level and pitch, repeat 2 6 dB down an octave up
    let level = |x: &[f32]| metrics::db(metrics::rms(x));
    assert!((level(repeat(1)) - level(&input[..len])).abs() < 0.05);
    assert!((level(repeat(2)) - level(repeat(1)) + 6.0).abs() < 0.1, "{}", level(repeat(2)) - level(repeat(1)));
    let a1 = metrics::tone_amplitude(repeat(1), 300.0, SR);
    let a2 = metrics::tone_amplitude(repeat(2), 600.0, SR);
    assert!(a1 > 0.45 && (a2 / a1 - 0.5).abs() < 0.03, "{a1} {a2}");
    assert!(metrics::tone_amplitude(repeat(3), 1200.0, SR) > 0.1);
}

#[test]
fn reverse_plays_the_slice_backward() {
    let input = noise(Q120 * 4);
    let len = Q120 / 2;
    let press = 2_048;
    let mut r = runner(&[("slice", 7.0), ("action", 1.0)]);
    let out = process(&mut r, &input, hold(press, 3 * Q120 + 64));
    let start = press + len;
    for k in 200..len - 200 {
        let want = input[press + len - k];
        assert!((out[start + k] - want).abs() < 1e-6, "sample {k}: {} against {want}", out[start + k]);
    }
}

/// Zero crossings per second over `x`
fn crossings(x: &[f32]) -> f64 {
    let n = x.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    n as f64 / 2.0 * SR / x.len() as f64
}

#[test]
fn the_tape_stops_in_its_beats_and_spins_back_up() {
    let input = signal::sine(440.0, 0.5, SR, Q120 * 8);
    let press = 4_096;
    let stop = Q120; // 1 beat at 120 BPM
    for tape_start in [0.0, 1.0] {
        let release = press + 2 * Q120;
        let mut r = runner(&[("action", 2.0), ("stop_time", 1.0), ("tape_start", tape_start)]);
        let out = process(&mut r, &input, hold(press, release));
        // Seamless start: the input itself
        assert_eq!(out[..press + 1], input[..press + 1]);
        // The pitch falls: about 440 Hz, then 220 Hz half way
        let early = crossings(&out[press..press + 2_400]);
        let mid = crossings(&out[press + stop / 2 - 1_200..press + stop / 2 + 1_200]);
        // (a crossing count over 50 ms reads within 10 Hz)
        assert!(early > 405.0 && early < 450.0, "{early}");
        assert!((mid - 220.0).abs() < 15.0, "{mid}");
        // The level follows the square root of the speed
        let half = metrics::peak(&out[press + stop / 2 - 1_200..press + stop / 2 + 1_200]);
        assert!((half - 0.5 * 0.5f32.sqrt()).abs() < 0.02, "{half}");
        // Silence once stopped
        assert!(out[press + stop..release].iter().all(|x| *x == 0.0));
        // Back to the input after a 10 ms crossfade: at once, or after a
        // spin-up of half a beat
        let back = if tape_start > 0.0 { release + stop / 2 + 481 } else { release + 480 };
        assert_eq!(out[back..], input[back..], "tape start {tape_start}");
        if tape_start > 0.0 {
            let rising = crossings(&out[release + stop / 4 - 600..release + stop / 4 + 600]);
            assert!((rising - 220.0).abs() < 20.0, "{rising}");
        }
    }
}

#[test]
fn the_chop_follows_its_pattern_and_duty() {
    let input = signal::dc(0.5, Q120 * 4);
    let mut r = runner(&[("pattern", 1.0), ("scope", 1.0), ("duty", 50.0)]);
    // A rolling host from bar 1: an eighth is 12000 samples
    let out = process(&mut r, &input, |r, s| {
        r.push_event(rolling(120.0, s));
    });
    for eighth in 0..8 {
        let s = eighth * 12_000;
        assert!(out[s + 200..s + 5_800].iter().all(|x| *x == 0.5), "eighth {eighth} open");
        assert!(out[s + 6_200..s + 11_800].iter().all(|x| *x == 0.0), "eighth {eighth} closed");
    }
    // Breakdown: open on the sixteenths 0, 3, 6, 8, 11 and 14
    let mut r = runner(&[("pattern", 6.0), ("scope", 1.0), ("duty", 100.0)]);
    let out = process(&mut r, &input, |r, s| {
        r.push_event(rolling(120.0, s));
    });
    let open: Vec<usize> = (0..16).filter(|k| out[k * 6_000 + 3_000] > 0.25).collect();
    assert_eq!(open, [0, 3, 6, 8, 11, 14]);
}

#[test]
fn chop_and_crush_while_held_only() {
    let input = signal::sine(220.0, 0.6, SR, Q120 * 4);
    let press = 24_064;
    let release = 48_000;
    let mut r = runner(&[("action", 3.0), ("bits", 4.0), ("rate", 6000.0)]);
    let out = process(&mut r, &input, hold(press, release));
    assert_eq!(out[..press], input[..press]);
    // 4 bits: steps of 1/8, values held 8 samples at 6 kHz
    let held = &out[press + 400..release];
    let bad = held.iter().position(|x| (x * 8.0).fract() != 0.0);
    assert!(bad.is_none(), "{bad:?} {:?}", bad.map(|b| &held[b.saturating_sub(4)..b + 4]));
    assert!(held.windows(2).filter(|w| w[0] != w[1]).count() <= held.len() / 8 + 1);
    assert_eq!(out[release + 400..], input[release + 400..]);
}

#[test]
fn in_free_time_the_chop_starts_on_the_press() {
    let input = signal::dc(0.5, Q120 * 3);
    // 120 BPM from the Tempo control: an eighth is 12000 samples
    let press = 10_240;
    let mut r = runner(&[("action", 3.0), ("pattern", 1.0), ("duty", 25.0)]);
    let out = process(&mut r, &input, hold(press, 2 * Q120 + 128));
    assert_eq!(out[..press], input[..press]);
    for eighth in 0..3 {
        let s = press + eighth * 12_000;
        assert!(out[s + 200..s + 2_800].iter().all(|x| *x == 0.5), "eighth {eighth} open");
        assert!(out[s + 3_200..s + 11_800].iter().all(|x| *x == 0.0), "eighth {eighth} closed");
    }
}

#[test]
fn mix_zero_is_dry_whatever_plays() {
    let input = noise(Q120 * 2);
    let mut r = runner(&[("mix", 0.0), ("pattern", 4.0), ("scope", 1.0), ("bits", 3.0)]);
    let out = process(&mut r, &input, hold(1_024, 40_064));
    assert_eq!(out, input);
}

#[test]
fn chaos_changes_the_repeats_and_is_repeatable() {
    let input = noise(Q120 * 6);
    let run = |chaos: f32| {
        let mut r = runner(&[("slice", 10.0), ("chaos", chaos)]);
        process(&mut r, &input, hold(1_024, 5 * Q120 + 64))
    };
    let plain = run(0.0);
    let wild = run(100.0);
    assert_eq!(wild, run(100.0));
    assert!(metrics::max_diff(&plain, &wild) > 0.1);
    assert!(wild.iter().all(|x| x.is_finite() && x.abs() < 1.5));
}

/// Fine edges: on a sine, every repeat seam, tape move and chop edge
/// stays near the sine's own slope
#[test]
fn edges_do_not_click() {
    type Case = (&'static str, f32, &'static [(&'static str, f32)]);
    let input = signal::sine(220.0, 0.5, SR, Q120 * 8);
    let slope = max_step(&input);
    // The pitch steps raise the slope by their speed (four times at most)
    let cases: [Case; 6] = [
        ("repeat 1/16T", 1.0, &[("slice", 11.0)]),
        ("repeat 1/32 step up", 4.0, &[("slice", 12.0), ("step", 7.0)]),
        ("reverse 1/8", 1.0, &[("slice", 7.0), ("action", 1.0)]),
        ("chaos", 2.0, &[("slice", 10.0), ("chaos", 100.0)]),
        ("tape", 1.0, &[("action", 2.0), ("stop_time", 0.5)]),
        ("chop", 1.0, &[("action", 3.0), ("pattern", 2.0), ("duty", 30.0)]),
    ];
    for (name, speed, params) in cases {
        let mut r = runner(params);
        let out = process(&mut r, &input, hold(10_112, 6 * Q120 + 640));
        let step = max_step(&out);
        assert!(step < 1.25 * speed * slope, "{name}: step {step} against {slope}");
    }
}

/// Records the frames of the grid stream
struct Frames {
    frames: Vec<(u32, f32, Vec<f32>)>,
}

impl StreamSink for Frames {
    fn connected(&self, _index: usize) -> bool {
        true
    }

    fn send(&mut self, _index: usize, _frame: u32, channels: u32, position: f32, values: &[f32]) -> bool {
        self.frames.push((channels, position, values.to_vec()));
        true
    }
}

#[test]
fn the_grid_shows_the_slice_its_repeats_and_the_playhead() {
    let input = noise(Q120 * 4);
    let mut r = runner(&[("slice", 10.0), ("decay", 3.0)]);
    let mut sink = Frames { frames: Vec::new() };
    let out = process_with(&mut r, &input, BLOCK, &mut sink, |r, s| {
        r.push_event(rolling(120.0, s));
        hold(Q120 + 64, 3 * Q120 + 64)(r, s);
    });
    assert_eq!(out.len(), input.len());
    assert!(sink.frames.len() >= 50 && sink.frames.len() <= 61, "{}", sink.frames.len());
    let (channels, position, values) = sink.frames.last().expect("a frame");
    assert_eq!(*channels, 3);
    assert_eq!(values.len(), 3 * GRID_CELLS);
    assert!((0.0..1.0).contains(position));
    // The bar is two seconds: beat 2 (cells 24 to 29) is the slice, the
    // next cells its repeats, 3 dB down each
    let [level, source, speed] = [0, 1, 2].map(|k| &values[k * GRID_CELLS..(k + 1) * GRID_CELLS]);
    assert_eq!(source[..24], [0.0; 24]);
    assert_eq!(source[24..30], [1.0; 6]);
    assert_eq!(source[30..72], [2.0; 42]);
    assert!(speed.iter().all(|s| *s == 1.0), "{speed:?}");
    assert!(level[30] > 0.99 && (level[36] - 0.708).abs() < 0.01, "{} {}", level[30], level[36]);
}

#[test]
fn golden_presets() {
    // A palm-muted riff: plucks on every eighth at 120 BPM
    let len = 4 * Q120;
    let mut input = vec![0.0f32; len];
    for (k, f) in [82.4, 82.4, 87.3, 82.4, 98.0, 82.4, 92.5, 110.0].iter().cycle().take(32).enumerate() {
        let note = signal::pluck(*f, 0.4, 0.2, SR, 12_000);
        for (o, x) in input[k * 3_000..].iter_mut().zip(note) {
            *o += x;
        }
    }
    for preset in SPLINTER.presets {
        let mut r = runner(preset.values);
        let out = process(&mut r, &input, |r, s| {
            r.push_event(rolling(120.0, s));
            hold(Q120 + 64, 3 * Q120 + 64)(r, s);
        });
        assert!(metrics::max_diff(&out, &input) > 0.05, "{} does nothing", preset.id);
        let path = dir().join("tests/golden").join(format!("{}.wav", preset.id));
        orx_test::golden::check(path, &[&out], SR as u32, 2e-5);
    }
}

#[test]
fn lv2_build_matches_the_runner_and_streams_the_grid() {
    use orx_lv2::host::Host;
    let d = orx_lv2::descriptor::<Splinter>();
    let host = Host::new(SR, 256);
    // SAFETY: the descriptor of Splinter, described by SPLINTER
    let mut p = unsafe { host.instantiate(&d.raw, &SPLINTER, true) }.expect("instance");
    let params = [("slice", 10.0), ("step", -2.0), ("pattern", 4.0), ("bits", 10.0)];
    for (s, v) in params {
        p.set(at(s), v);
    }
    p.activate();
    let input = noise(256 * 60);
    let mut out = Vec::new();
    let mut frames = 0;
    for (k, chunk) in input.chunks(256).enumerate() {
        p.set(at("hold"), if (10..40).contains(&k) { 1.0 } else { 0.0 });
        p.input(0).copy_from_slice(chunk);
        p.push_position(0, 140.0, 1.0);
        orx_test::rt::no_alloc(|| p.run(256));
        out.extend_from_slice(p.output(0));
        frames += p.frames(at("grid")).len();
    }
    let mut r = runner(&params);
    let reference = process_with(&mut r, &input, 256, &mut NoStreams, |r, s| {
        let k = s / 256;
        r.set_param(at("hold"), if (10..40).contains(&k) { 1.0 } else { 0.0 });
        let position = Position { bpm: Some(140.0), speed: Some(1.0), ..Position::default() };
        r.push_event(Event { frame: 0, kind: EventKind::Position(position) });
    });
    assert_eq!(out, reference);
    assert!(frames > 0, "no grid frame");
}
