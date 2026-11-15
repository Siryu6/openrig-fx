//! The conformance suite every plugin of the suite passes.
//!
//! [`check`] runs, through the same [`Runner`] the wrappers use:
//!
//! 1. static validation of the description ([`orx_plugin::validate`]);
//! 2. at 44.1, 48 and 96 kHz, a stretch of guitar-like signal processed in
//!    random block sizes with random automation of every parameter, random
//!    bypass toggles and transport events, inside [`rt::no_alloc`] when the
//!    plugin declares `hard_rt` (which then requires the allocation trap to
//!    be installed): outputs stay finite and bounded, streams respect
//!    their size and rate, the latency stays within `max_latency`;
//! 3. hostile inputs: silence, DC, loud noise, denormals, NaN and
//!    infinity (the plugin must come back to finite output);
//! 4. block-size independence: the same input at fixed parameters gives
//!    the same output in blocks of 1, 7, 64 and the maximum;
//! 5. reset determinism: a run after `activate` repeats the first exactly;
//! 6. every preset processes cleanly;
//! 7. with [`Plugin::TRAILS`], the default settings and every preset ring
//!    out after a bypass on silence: the plugin stops within
//!    [`TRAIL_LIMIT_S`], and its output is the dry signal from then on.

use orx_dsp::noise::Rng;
use orx_plugin::process::StreamSink;
use orx_plugin::runner::{Runner, TRAIL_LIMIT_S};
use orx_plugin::{Config, Event, EventKind, Plugin, PortKind, Position};

use crate::{metrics, rt, signal};

/// Tuning of the suite
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Largest output sample allowed (default 16, +24 dBFS)
    pub max_output: f32,
    /// Largest difference between block sizes at fixed parameters
    pub block_tolerance: f32,
    /// Seconds of audio per automation run
    pub seconds: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self { max_output: 16.0, block_tolerance: 1e-6, seconds: 2.0 }
    }
}

const MAX_BLOCK: usize = 512;

/// Run the whole suite on `P` with default options
pub fn check<P: Plugin>() {
    check_with::<P>(Options::default());
}

pub fn check_with<P: Plugin>(options: Options) {
    let info = P::INFO;
    orx_plugin::validate::assert_valid(info);
    if info.hard_rt {
        assert!(
            rt::installed(),
            "{} declares hard_rt: call orx_test::install_rt_check!() in this test binary",
            info.uri
        );
    }
    for (k, rate) in [44_100.0, 48_000.0, 96_000.0].into_iter().enumerate() {
        automation::<P>(rate, 0xc0ffee + k as u32, &options);
    }
    hostile_inputs::<P>(&options);
    block_independence::<P>(&options);
    reset_determinism::<P>();
    presets::<P>(&options);
    if P::TRAILS {
        trails_end::<P>();
    }
}

/// Audio buffers for a run, allocated up front
struct Buffers {
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
}

impl Buffers {
    fn new<P: Plugin>(input: &[f32]) -> Self {
        let info = P::INFO;
        // Stereo plugins get a slightly different right channel
        let inputs = (0..info.audio_inputs())
            .map(|c| input.iter().enumerate().map(|(i, x)| if c % 2 == 1 && i % 3 == 0 { -x } else { *x }).collect())
            .collect();
        Self { inputs, outputs: vec![vec![0.0; input.len()]; info.audio_outputs()] }
    }

    fn process<P: Plugin>(&mut self, r: &mut Runner<P>, start: usize, n: usize, sink: &mut dyn StreamSink) {
        let mut ins: [&[f32]; 8] = [&[]; 8];
        for (s, b) in ins.iter_mut().zip(&self.inputs) {
            *s = &b[start..start + n];
        }
        let n_out = self.outputs.len();
        let mut outs: [&mut [f32]; 8] = Default::default();
        for (s, b) in outs.iter_mut().zip(self.outputs.iter_mut()) {
            *s = &mut b[start..start + n];
        }
        r.process(n, &ins[..self.inputs.len()], &mut outs[..n_out], sink);
    }
}

/// Checks stream frames against the declaration, without allocating
struct CheckingSink {
    info: &'static orx_plugin::PluginInfo,
    sent: [u32; 64],
}

impl StreamSink for CheckingSink {
    fn connected(&self, _index: usize) -> bool {
        true
    }

    fn send(&mut self, index: usize, _frame: u32, channels: u32, position: f32, values: &[f32]) -> bool {
        let Some(PortKind::Stream(s)) = self.info.ports.get(index).map(|p| p.kind) else {
            panic!("stream frame on port {index}, which is not a stream");
        };
        assert_eq!(channels, s.channels);
        assert_eq!(values.len(), s.values(), "stream {index}: frame size");
        assert!(values.iter().all(|v| v.is_finite()) && position.is_finite(), "stream {index}: non-finite value");
        if let Some(c) = self.sent.get_mut(index) {
            *c += 1;
        }
        true
    }
}

fn run_rt<R>(hard_rt: bool, f: impl FnOnce() -> R) -> R {
    if hard_rt { rt::no_alloc(f) } else { f() }
}

fn assert_bounded(outputs: &[Vec<f32>], max: f32, what: &str) {
    for (c, o) in outputs.iter().enumerate() {
        if let Some((i, x)) = o.iter().enumerate().find(|(_, x)| !x.is_finite() || x.abs() > max) {
            panic!("{what}: output {c} frame {i} is {x}");
        }
    }
}

fn automation<P: Plugin>(sample_rate: f64, seed: u32, options: &Options) {
    let info = P::INFO;
    let len = (options.seconds * sample_rate) as usize;
    let input: Vec<f32> = signal::pluck(110.0, 0.8, 1.5, sample_rate, len)
        .iter()
        .zip(signal::noise(seed, 0.05, len))
        .map(|(a, b)| a + b)
        .collect();
    let mut buffers = Buffers::new::<P>(&input);
    let config = Config { sample_rate, max_block: MAX_BLOCK };
    let mut r = Runner::<P>::new(config);
    r.activate();
    let mut sink = CheckingSink { info, sent: [0; 64] };
    let mut rng = Rng::new(seed);
    let max_latency = P::max_latency(&config);
    let params: Vec<usize> = info.indices(|k| matches!(k, PortKind::Param(_) | PortKind::Enabled)).collect();
    let has_time = info.ports.iter().any(|p| matches!(p.kind, PortKind::Events { time: true, .. }));
    let has_midi = info.ports.iter().any(|p| matches!(p.kind, PortKind::Events { midi: true, .. }));

    let mut start = 0;
    while start < len {
        // Mostly host-like sizes, sometimes anything up to twice the max
        let n = match rng.next_u32() % 4 {
            0 => 1 + (rng.next_u32() as usize % (2 * MAX_BLOCK)),
            1 => 1 + (rng.next_u32() as usize % 16),
            _ => 128,
        }
        .min(len - start);
        if rng.next_u32().is_multiple_of(3) {
            for &i in &params {
                if rng.next_u32().is_multiple_of(4) {
                    let v = match info.ports[i].kind {
                        PortKind::Param(p) => p.from_normalized(rng.uniform()),
                        // Bypass rarely, so most of the run is processed
                        _ => {
                            if rng.next_u32().is_multiple_of(8) {
                                0.0
                            } else {
                                1.0
                            }
                        }
                    };
                    r.set_param(i, v);
                }
            }
        }
        if has_time && rng.next_u32().is_multiple_of(16) {
            let bpm = 40.0 + 200.0 * rng.uniform();
            let position = Position { bpm: Some(bpm), speed: Some(1.0), ..Position::default() };
            r.push_event(Event { frame: rng.next_u32() % n as u32, kind: EventKind::Position(position) });
        }
        if has_midi && rng.next_u32().is_multiple_of(8) {
            let data = [0xb0, (rng.next_u32() % 128) as u8, (rng.next_u32() % 128) as u8];
            r.push_event(Event { frame: 0, kind: EventKind::Midi { data, len: 3 } });
        }
        run_rt(info.hard_rt, || buffers.process(&mut r, start, n, &mut sink));
        assert!(r.latency() <= max_latency, "latency {} above max_latency {max_latency}", r.latency());
        start += n;
    }
    assert_bounded(&buffers.outputs, options.max_output, &format!("automation at {sample_rate} Hz"));
    assert_eq!(r.faults(), 0, "non-finite output under automation at {sample_rate} Hz");
    for (i, port) in info.ports.iter().enumerate() {
        if let PortKind::Stream(s) = port.kind {
            let most = (options.seconds * f64::from(s.max_rate)).ceil() as u32 + 1;
            assert!(sink.sent[i] <= most, "stream {}: {} frames, at most {most} allowed", port.symbol, sink.sent[i]);
            assert!(sink.sent[i] > 0, "stream {}: no frame sent", port.symbol);
        }
        if matches!(port.kind, PortKind::Meter(_) | PortKind::Latency) {
            assert!(r.meter(i).is_finite());
        }
    }
}

fn hostile_inputs<P: Plugin>(options: &Options) {
    let info = P::INFO;
    let rate = 48_000.0;
    let len = 48_000;
    let cases: [(&str, Vec<f32>); 5] = [
        ("silence", vec![0.0; len]),
        ("DC", signal::dc(1.0, len)),
        ("loud noise", signal::noise(7, 1.0, len)),
        ("denormals", signal::dc(1e-39, len)),
        ("sub-sonic sine", signal::sine(3.0, 1.0, rate, len)),
    ];
    for (name, input) in cases {
        let mut r = Runner::<P>::new(Config { sample_rate: rate, max_block: MAX_BLOCK });
        r.activate();
        let mut b = Buffers::new::<P>(&input);
        let mut sink = CheckingSink { info, sent: [0; 64] };
        for start in (0..len).step_by(256) {
            run_rt(info.hard_rt, || b.process(&mut r, start, 256.min(len - start), &mut sink));
        }
        assert_bounded(&b.outputs, options.max_output, name);
    }

    // NaN and infinity in the input: the runner silences the block and
    // resets; the plugin must give finite output afterwards
    for bad in [f32::NAN, f32::INFINITY] {
        let mut input = signal::sine(220.0, 0.5, rate, len);
        input[1000] = bad;
        let mut r = Runner::<P>::new(Config { sample_rate: rate, max_block: MAX_BLOCK });
        r.activate();
        let mut b = Buffers::new::<P>(&input);
        let mut sink = CheckingSink { info, sent: [0; 64] };
        for start in (0..len).step_by(256) {
            run_rt(info.hard_rt, || b.process(&mut r, start, 256.min(len - start), &mut sink));
        }
        // From the block after the bad sample on (plus latency), finite
        let from = 1024 + 256 + P::max_latency(r.config()) as usize;
        for o in &b.outputs {
            assert!(o[from..].iter().all(|x| x.is_finite()), "no recovery after {bad} input");
            assert!(o[..1000].iter().all(|x| x.is_finite()));
        }
    }
}

fn render<P: Plugin>(input: &[f32], block: usize, setup: &dyn Fn(&mut Runner<P>)) -> Vec<Vec<f32>> {
    let mut r = Runner::<P>::new(Config { sample_rate: 48_000.0, max_block: MAX_BLOCK });
    setup(&mut r);
    r.activate();
    let mut b = Buffers::new::<P>(input);
    let mut sink = CheckingSink { info: P::INFO, sent: [0; 64] };
    let mut start = 0;
    while start < input.len() {
        let n = block.min(input.len() - start);
        b.process(&mut r, start, n, &mut sink);
        start += n;
    }
    b.outputs
}

fn block_independence<P: Plugin>(options: &Options) {
    let input = signal::pluck(196.0, 0.7, 1.0, 48_000.0, 24_000);
    let reference = render::<P>(&input, MAX_BLOCK, &|_| {});
    for block in [1, 7, 64, 1000] {
        let other = render::<P>(&input, block, &|_| {});
        for (a, b) in reference.iter().zip(&other) {
            let d = metrics::max_diff(a, b);
            assert!(d <= options.block_tolerance, "blocks of {block} differ from blocks of {MAX_BLOCK} by {d:e}");
        }
    }
}

fn reset_determinism<P: Plugin>() {
    let input = signal::pluck(82.4, 0.9, 1.0, 48_000.0, 12_000);
    let mut r = Runner::<P>::new(Config { sample_rate: 48_000.0, max_block: MAX_BLOCK });
    let mut runs = Vec::new();
    for _ in 0..2 {
        r.activate();
        let mut b = Buffers::new::<P>(&input);
        for start in (0..input.len()).step_by(100) {
            b.process(&mut r, start, 100.min(input.len() - start), &mut orx_plugin::process::NoStreams);
        }
        runs.push(b.outputs);
    }
    assert_eq!(runs[0], runs[1], "a run after activate() differs from the first");
}

fn presets<P: Plugin>(options: &Options) {
    let info = P::INFO;
    let input = signal::pluck(146.8, 0.8, 1.0, 48_000.0, 24_000);
    for preset in info.presets {
        let out = render::<P>(&input, 128, &|r| {
            for (i, v) in info.preset_values(preset) {
                r.set_param(i, v);
            }
        });
        assert_bounded(&out, options.max_output, preset.name);
    }
}

fn trails_end<P: Plugin>() {
    let info = P::INFO;
    let Some(enabled) = info.indices(|k| matches!(k, PortKind::Enabled)).next() else {
        panic!("{}: TRAILS without an lv2:enabled port", info.uri)
    };
    let rate = 48_000.0;
    let block = MAX_BLOCK;
    let input = signal::pluck(110.0, 0.8, 0.5, rate, 24_000);
    let silence = vec![0.0; block];
    let settings = core::iter::once(None).chain(info.presets.iter().map(Some));
    for preset in settings {
        let name = preset.map_or("defaults", |p| p.name);
        let mut r = Runner::<P>::new(Config { sample_rate: rate, max_block: MAX_BLOCK });
        if let Some(p) = preset {
            for (i, v) in info.preset_values(p) {
                r.set_param(i, v);
            }
        }
        r.activate();
        let mut b = Buffers::new::<P>(&input);
        for start in (0..input.len()).step_by(block) {
            b.process(&mut r, start, block.min(input.len() - start), &mut orx_plugin::process::NoStreams);
        }
        r.set_param(enabled, 0.0);
        let mut b = Buffers::new::<P>(&silence);
        let most = ((TRAIL_LIMIT_S + 1.0) * rate) as usize / block;
        let rung = (0..most).position(|_| {
            b.process(&mut r, 0, block, &mut orx_plugin::process::NoStreams);
            r.is_idle()
        });
        assert!(rung.is_some(), "{}, {name}: still ringing {} s after a bypass", info.uri, TRAIL_LIMIT_S + 1.0);
        b.process(&mut r, 0, block, &mut orx_plugin::process::NoStreams);
        assert!(b.outputs.iter().all(|o| o.iter().all(|y| *y == 0.0)), "{}, {name}: not silent once stopped", info.uri);
    }
}
