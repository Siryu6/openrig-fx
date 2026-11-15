use super::*;
use crate::desc::{Class, Param, Port, Stream, Version};
use crate::process::NoStreams;
use crate::view::StreamContent;

/// Pure delay of `delay` frames times a gain; outputs NaN when `poison` is on
struct Delay {
    line: DelayLine,
    seen_events: usize,
    last_event_frame: u32,
}

const DELAY: usize = 37;
const INFO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/test-delay",
    name: "Test delay",
    description: "",
    class: Class::Utility,
    version: Version::parse("0.2"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param("gain", "Gain", Param::float(1.0, 0.0, 2.0)),
        Port::param("poison", "Poison", Param::toggle(false)),
        Port::param("steps", "Steps", Param::int(2, 0, 4)),
        Port::enabled(),
        Port::latency(),
        Port::events("events", "Events", true, true),
        Port::stream(
            "scope",
            "Scope",
            Stream { content: StreamContent::LevelHistory, length: 4, channels: 1, max_rate: 30.0 },
        ),
    ],
    presets: &[],
    view: None,
    face: crate::view::Face::basic("A test", &[]),
    hard_rt: true,
};

impl Plugin for Delay {
    const INFO: &'static PluginInfo = &INFO;

    fn new(_config: &Config) -> Self {
        Self { line: DelayLine::new(DELAY), seen_events: 0, last_event_frame: 0 }
    }

    fn reset(&mut self) {
        self.line.reset();
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let gain = ctx.params.get(2);
        let poison = ctx.params.on(3);
        for e in ctx.events {
            assert!((e.frame as usize) < ctx.frames);
            self.seen_events += 1;
            self.last_event_frame = e.frame;
        }
        let (input, output) = (ctx.inputs[0], &mut ctx.outputs[0]);
        for (o, x) in output.iter_mut().zip(input) {
            self.line.write(*x);
            *o = if poison { f32::NAN } else { self.line.read(DELAY) * gain };
        }
        ctx.meters.set(1, f32::NAN);
        if ctx.streams.due(8) {
            ctx.streams.send(8, 0, 0.0, &[1.0; 16]);
        }
    }

    fn latency(&self) -> u32 {
        DELAY as u32
    }

    fn max_latency(_config: &Config) -> u32 {
        DELAY as u32
    }
}

fn ramp(n: usize) -> Vec<f32> {
    (0..n).map(|i| (i as f32 * 0.37).sin()).collect()
}

fn run(r: &mut Runner<Delay>, input: &[f32], block: usize, sink: &mut dyn StreamSink) -> Vec<f32> {
    let mut out = vec![0.0; input.len()];
    for (i, o) in input.chunks(block).zip(out.chunks_mut(block)) {
        r.process(i.len(), &[i], &mut [o], sink);
    }
    out
}

fn delayed(input: &[f32], i: usize) -> f32 {
    if i >= DELAY { input[i - DELAY] } else { 0.0 }
}

#[test]
fn long_blocks_are_split() {
    let mut r = Runner::<Delay>::new(Config { sample_rate: 48_000.0, max_block: 64 });
    r.activate();
    let input = ramp(1000);
    let out = run(&mut r, &input, 1000, &mut NoStreams);
    for (i, y) in out.iter().enumerate() {
        assert_eq!(*y, delayed(&input, i));
    }
    assert_eq!(r.meter(6), DELAY as f32);
}

#[test]
fn events_follow_their_piece() {
    let mut r = Runner::<Delay>::new(Config { sample_rate: 48_000.0, max_block: 64 });
    r.activate();
    for frame in [0, 10, 64, 200] {
        assert!(r.push_event(Event { frame, kind: EventKind::Midi { data: [0x90, 60, 100], len: 3 } }));
    }
    let input = ramp(256);
    run(&mut r, &input, 256, &mut NoStreams);
    assert_eq!(r.plugin().seen_events, 4);
    assert_eq!(r.plugin().last_event_frame, 200 - 192);
}

#[test]
fn params_are_sanitised() {
    let mut r = Runner::<Delay>::new(Config { sample_rate: 48_000.0, max_block: 64 });
    r.set_param(2, f32::NAN);
    assert_eq!(r.param(2), 1.0);
    r.set_param(2, 9.0);
    assert_eq!(r.param(2), 2.0);
    r.set_param(4, 2.6);
    assert_eq!(r.param(4), 3.0);
    r.set_param(3, 0.7);
    assert_eq!(r.param(3), 1.0);
    r.set_param(0, 5.0);
    assert_eq!(r.param(0), 0.0);
    r.set_param(99, 5.0);
}

#[test]
fn bypass_is_latency_aligned() {
    // The plugin is a pure delay at gain 1: wet and aligned dry are the
    // same, so bypassing must not change a single sample. Re-enabling
    // resets the plugin, whose delay line restarts empty: only the first
    // DELAY frames of the fade-in may differ, by at most the fade gain.
    let mut r = Runner::<Delay>::new(Config { sample_rate: 48_000.0, max_block: 128 });
    r.activate();
    let input = ramp(48_000);
    let mut out = vec![0.0; input.len()];
    let mut enables = Vec::new();
    for (k, (i, o)) in input.chunks(100).zip(out.chunks_mut(100)).enumerate() {
        if k % 37 == 5 {
            let on = (k / 37) % 2 == 1;
            r.set_param(5, if on { 1.0 } else { 0.0 });
            if on {
                enables.push(k * 100);
            }
        }
        r.process(i.len(), &[i], &mut [o], &mut NoStreams);
    }
    assert!(enables.len() > 4);
    for (i, y) in out.iter().enumerate() {
        let expected = delayed(&input, i);
        let tolerance = match enables.iter().find(|s| (**s..**s + DELAY).contains(&i)) {
            Some(s) => (i - s + 1) as f32 / 480.0 * expected.abs() + 1e-6,
            None => 1e-6,
        };
        assert!((y - expected).abs() <= tolerance, "frame {i}: {y} vs {expected}");
    }
}

#[test]
fn bypass_fades() {
    let mut r = Runner::<Delay>::new(Config { sample_rate: 48_000.0, max_block: 128 });
    r.set_param(2, 0.0); // wet is silence
    r.activate();
    let input = vec![1.0; 4800];
    let out = run(&mut r, &input, 128, &mut NoStreams);
    assert!(out.iter().all(|y| *y == 0.0));
    r.set_param(5, 0.0);
    let out = run(&mut r, &input, 128, &mut NoStreams);
    // 10 ms linear fade to the dry signal
    let fade = 480;
    assert!((out[fade / 2] - 0.5).abs() < 0.01);
    assert!(out.windows(2).all(|w| w[1] >= w[0]));
    assert!(out[fade + 1..].iter().all(|y| *y == 1.0));
}

#[test]
fn non_finite_output_is_silenced() {
    let mut r = Runner::<Delay>::new(Config { sample_rate: 48_000.0, max_block: 64 });
    r.activate();
    r.set_param(3, 1.0);
    let out = run(&mut r, &ramp(256), 64, &mut NoStreams);
    assert!(out.iter().all(|y| *y == 0.0));
    assert_eq!(r.faults(), 4);
    assert_eq!(r.meter(1), 0.0);
}

struct Counter {
    sent: usize,
    last_len: usize,
}

impl StreamSink for Counter {
    fn connected(&self, index: usize) -> bool {
        index == 8
    }
    fn send(&mut self, _: usize, _: u32, channels: u32, _: f32, values: &[f32]) -> bool {
        assert_eq!(channels, 1);
        self.sent += 1;
        self.last_len = values.len();
        true
    }
}

#[test]
fn streams_are_rate_limited() {
    let mut r = Runner::<Delay>::new(Config { sample_rate: 48_000.0, max_block: 128 });
    r.activate();
    let mut sink = Counter { sent: 0, last_len: 0 };
    run(&mut r, &vec![0.0; 48_000], 128, &mut sink);
    // 30 Hz at most: one frame each 1600 frames, checked per block
    assert!((27..=30).contains(&sink.sent), "{} frames", sink.sent);
    assert_eq!(sink.last_len, 4);
}

/// Dry plus a feedback echo of `ECHO` frames, ringing out when bypassed
struct Echo {
    line: DelayLine,
    /// Frames since a sample above the silence threshold went in the line
    quiet: usize,
}

const ECHO: usize = 100;
const ECHO_INFO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/test-echo",
    name: "Test echo",
    description: "",
    class: Class::Delay,
    version: Version::parse("0.1"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param("feedback", "Feedback", Param::float(0.5, 0.0, 1.0)),
        Port::enabled(),
    ],
    presets: &[],
    view: None,
    face: crate::view::Face::basic("A test", &[]),
    hard_rt: true,
};

impl Plugin for Echo {
    const INFO: &'static PluginInfo = &ECHO_INFO;
    const TRAILS: bool = true;

    fn new(_config: &Config) -> Self {
        Self { line: DelayLine::new(ECHO), quiet: usize::MAX }
    }

    fn reset(&mut self) {
        self.line.reset();
        self.quiet = usize::MAX;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let fb = ctx.params.get(2);
        let (input, output) = (ctx.inputs[0], &mut ctx.outputs[0]);
        for (o, x) in output.iter_mut().zip(input) {
            let echo = self.line.read(ECHO - 1);
            let w = x + fb * echo;
            self.line.write(w);
            self.quiet = if w.abs() > 1e-6 { 0 } else { self.quiet.saturating_add(1) };
            *o = x + echo;
        }
    }

    fn is_ringing(&self) -> bool {
        self.quiet < ECHO
    }
}

fn run_echo(r: &mut Runner<Echo>, input: &[f32]) -> Vec<f32> {
    let mut out = vec![0.0; input.len()];
    for (i, o) in input.chunks(64).zip(out.chunks_mut(64)) {
        r.process(i.len(), &[i], &mut [o], &mut NoStreams);
    }
    out
}

#[test]
fn trails_ring_out_over_the_dry_signal() {
    let mut r = Runner::<Echo>::new(Config { sample_rate: 48_000.0, max_block: 64 });
    r.activate();
    // A burst, then a steady signal the bypass lets through
    let burst: Vec<f32> = (0..64).map(|i| (i as f32 * 0.3).sin()).collect();
    let mut input = burst.clone();
    input.resize(ECHO * 2, 0.0);
    run_echo(&mut r, &input);
    r.set_param(3, 0.0);
    // Silence while the input fades, then a signal the bypass lets through
    let mut steady = vec![0.0; 600];
    steady.extend(ramp(ECHO * 40));
    let out = run_echo(&mut r, &steady);
    // Bypassed: the dry signal plus the echoes of the burst at their
    // places, halved at each repeat
    for (i, (y, x)) in out.iter().zip(&steady).enumerate() {
        let k = i + ECHO * 2;
        let (n, j) = (k / ECHO, k % ECHO);
        let echo = if j < burst.len() && n >= 1 { burst[j] * 0.5f32.powi(n as i32 - 1) } else { 0.0 };
        let expected = x + if echo.abs() > 1e-6 || !r.is_idle() { echo } else { 0.0 };
        assert!((y - expected).abs() < 1e-5, "frame {i}: {y} vs {expected}");
    }
    // Rung out: the plugin stopped, the output is the dry signal
    assert!(r.is_idle());
    let out = run_echo(&mut r, &steady);
    assert_eq!(out, steady);
}

#[test]
fn trails_fade_the_input_not_the_output() {
    let mut r = Runner::<Echo>::new(Config { sample_rate: 48_000.0, max_block: 64 });
    r.set_param(2, 0.0);
    r.activate();
    let input = vec![1.0; 2000];
    run_echo(&mut r, &input);
    r.set_param(3, 0.0);
    let out = run_echo(&mut r, &input);
    // Dry stays whole across the fade; the echo of the faded input follows
    // one echo later and stops
    assert!(out[..ECHO].iter().all(|y| (y - 2.0).abs() < 1e-6));
    let fade = 480;
    for (i, y) in out.iter().enumerate().skip(ECHO) {
        let g = (1.0 - ((i - ECHO + 1) as f32 / fade as f32)).max(0.0);
        assert!((y - (1.0 + g)).abs() < 1e-4, "frame {i}: {y}");
    }
    assert!(r.is_idle());
}

#[test]
fn trails_resume_without_a_reset() {
    let mut r = Runner::<Echo>::new(Config { sample_rate: 48_000.0, max_block: 64 });
    r.set_param(2, 0.9);
    r.activate();
    let mut input = vec![0.0; ECHO * 4];
    input[0] = 1.0;
    run_echo(&mut r, &input);
    r.set_param(3, 0.0);
    run_echo(&mut r, &vec![0.0; ECHO * 3]);
    r.set_param(3, 1.0);
    let out = run_echo(&mut r, &vec![0.0; ECHO]);
    // Eighth repeat of the impulse, still there
    let peak = out.iter().fold(0.0f32, |m, y| m.max(y.abs()));
    assert!((peak - 0.9f32.powi(6)).abs() < 1e-5, "{peak}");
}

#[test]
fn trails_stop_at_the_limit() {
    let mut r = Runner::<Echo>::new(Config { sample_rate: 8_000.0, max_block: 64 });
    r.set_param(2, 1.0);
    r.activate();
    let mut input = vec![0.0; 1000];
    input[0] = 1.0;
    run_echo(&mut r, &input);
    r.set_param(3, 0.0);
    let limit = (TRAIL_LIMIT_S * 8_000.0) as usize;
    let out = run_echo(&mut r, &vec![0.0; limit - 1000]);
    assert!(!r.is_idle() && out.contains(&1.0));
    // The limit counts from the end of the input fade (80 frames)
    let out = run_echo(&mut r, &vec![0.0; 3000]);
    assert!(r.is_idle());
    // Faded, not cut: the last repeat comes out between 0 and 1
    assert!(out.iter().any(|y| *y > 0.0 && *y < 0.99), "the tail faded");
    let last = out.iter().rposition(|y| *y != 0.0);
    assert!(last.is_some_and(|i| (1000..1000 + 80 + 80 + 64).contains(&i)), "{last:?}");
}
