//! The OpenRig FX plugins inside OpenRig's own LV2 host.
//!
//! Built by `cargo xtask openrig-host` against an OpenRig checkout (the
//! `pedalboard` crate), never as part of this workspace. For each plugin
//! of the suite it checks what OpenRig reads from the bundle (ports,
//! ranges, properties, units, presets, transport input) against the
//! plugin's description, then runs it through `EffectChain`, the chain
//! of the pedalboard's audio thread, and compares the output with the
//! plugin's own runner fed the same signal, parameters and transport:
//! the two must be bit-exact.
//!
//! Usage: `openrig-host-check LV2_PATH`. Prints one line per check,
//! `ok|FAIL|note <TAB> check <TAB> detail`, for xtask to report.

use orx_plugin::dynamic::{DynRunner, Entry};
use orx_plugin::process::NoStreams;
use orx_plugin::runner::TRAIL_LIMIT_S;
use orx_plugin::{Config, Event, EventKind, ParamKind, PluginInfo, PortKind, Position};
use pedalboard::audio::ChainNode;
use pedalboard::plugins::lv2_host::{Lv2Host, host_from_path};
use pedalboard::plugins::scanner::{self, PortType};
use pedalboard::plugins::{ChainCommand, ChainSpec, EffectChain};
use std::collections::HashMap;
use std::process::ExitCode;

const RATE: f32 = 48_000.0;
/// OpenRig's period on the Pi
const BLOCK: usize = 128;
/// Tempo OpenRig sends as the transport
const BPM: f32 = 120.0;

/// One line of results
struct Out {
    failures: usize,
}

impl Out {
    fn line(&mut self, status: &str, check: &str, detail: &str) {
        if status == "FAIL" {
            self.failures += 1;
        }
        println!("{status}\t{check}\t{detail}");
    }

    fn ok(&mut self, check: &str, detail: &str) {
        self.line("ok", check, detail);
    }

    fn fail(&mut self, check: &str, detail: &str) {
        self.line("FAIL", check, detail);
    }

    fn note(&mut self, check: &str, detail: &str) {
        self.line("note", check, detail);
    }

    /// `ok` when `problems` is empty, else one `FAIL` listing them
    fn verdict(&mut self, check: &str, ok: &str, problems: &[String]) {
        if problems.is_empty() { self.ok(check, ok) } else { self.fail(check, &problems.join("; ")) }
    }
}

fn main() -> ExitCode {
    let Some(lv2_path) = std::env::args().nth(1) else {
        eprintln!("usage: openrig-host-check LV2_PATH");
        return ExitCode::from(2);
    };
    let host = host_from_path(&lv2_path);
    let mut out = Out { failures: 0 };
    for entry in orx_suite::plugins() {
        check_plugin(&host, entry, &mut out);
    }
    dual_mono(&host, &mut out);
    if out.failures == 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn check_plugin(host: &Lv2Host, entry: &Entry, out: &mut Out) {
    let info = entry.info;
    let catalogue = host.scanner();
    let Some(found) = catalogue.find_by_uri(info.uri) else {
        return out.fail("discover", &format!("{}: not in OpenRig's catalogue", info.uri));
    };
    let check = entry.slug();
    out.verdict(
        &format!("{check}:ports"),
        &format!("{} ports read as described", found.ports.len()),
        &ports(info, found),
    );
    out.verdict(
        &format!("{check}:presets"),
        &format!("{} presets read as described", found.presets.len()),
        &presets(info, found),
    );
    if info.ports.iter().any(|p| p.kind == PortKind::Enabled) {
        out.note(
            &format!("{check}:enabled"),
            "OpenRig lists the lv2:enabled port as a control; it should hide it and bypass through it",
        );
    }

    // Defaults: transport, then a change of every continuous parameter,
    // then the plugin's own bypass (lv2:enabled)
    let stereo = info.audio_outputs() >= 2;
    let defaults = controls(info, &[]);
    let changes = staged_changes(info);
    out.verdict(
        &format!("{check}:chain"),
        "bit-exact through EffectChain (tempo, parameter changes, bypass until idle)",
        &compare(host, entry, &defaults, &changes, stereo),
    );
    // Each factory preset, as OpenRig applies it (its values by symbol)
    let mut problems = Vec::new();
    for preset in &found.presets {
        let values: Vec<(&str, f32)> = preset.values.iter().map(|(s, v)| (s.as_str(), *v)).collect();
        problems.extend(
            compare(host, entry, &controls(info, &values), &[], stereo)
                .into_iter()
                .map(|p| format!("{}: {p}", preset.label)),
        );
    }
    out.verdict(&format!("{check}:presets-audio"), "every preset bit-exact through EffectChain", &problems);
}

/// What OpenRig read from the bundle against the description
fn ports(info: &PluginInfo, found: &scanner::PluginInfo) -> Vec<String> {
    let mut problems = Vec::new();
    if found.ports.len() != info.ports.len() {
        problems.push(format!("{} ports, {} described", found.ports.len(), info.ports.len()));
    }
    if found.name != info.name {
        problems.push(format!("name {:?}", found.name));
    }
    if found.class != info.class.lv2_name() {
        problems.push(format!("class {:?}", found.class));
    }
    if found.audio_ports() != (info.audio_inputs(), info.audio_outputs()) {
        problems.push(format!("audio ports {:?}", found.audio_ports()));
    }
    for (i, (p, f)) in info.ports.iter().zip(&found.ports).enumerate() {
        let mut wrong = |what: &str| problems.push(format!("port {i} ({}): {what}", p.symbol));
        if f.index != i || f.symbol != p.symbol || f.name != p.name {
            wrong(&format!("read as #{} {:?} {:?}", f.index, f.symbol, f.name));
        }
        let expected = match p.kind {
            PortKind::AudioIn => PortType::AudioInput,
            PortKind::AudioOut => PortType::AudioOutput,
            PortKind::Param(_) | PortKind::Enabled => PortType::ControlInput,
            PortKind::Meter(_) | PortKind::Latency => PortType::ControlOutput,
            PortKind::Events { .. } => PortType::AtomInput,
            PortKind::Stream(_) => PortType::AtomOutput,
        };
        if f.port_type != expected {
            wrong(&format!("type {:?}", f.port_type));
        }
        match p.kind {
            PortKind::Param(param) => {
                if (f.min, f.max, f.default) != (param.min, param.max, param.default) {
                    wrong(&format!("range {} {} {}", f.min, f.max, f.default));
                }
                let (toggled, integer, enumeration) = match param.kind {
                    ParamKind::Float => (false, false, false),
                    ParamKind::Int => (false, true, false),
                    ParamKind::Toggle => (true, false, false),
                    ParamKind::Choice(_) => (false, true, true),
                };
                if (f.toggled, f.integer, f.enumeration, f.logarithmic) != (toggled, integer, enumeration, param.log) {
                    wrong(&format!(
                        "properties toggled={} integer={} enumeration={} logarithmic={}",
                        f.toggled, f.integer, f.enumeration, f.logarithmic
                    ));
                }
                let unit = Some(param.unit.symbol()).filter(|s| !s.is_empty());
                if f.unit.as_deref() != unit {
                    wrong(&format!("unit {:?}", f.unit));
                }
                if f.tempo != (param.unit == orx_plugin::Unit::Bpm) {
                    wrong(&format!("tempo {}", f.tempo));
                }
                if let ParamKind::Choice(choices) = param.kind {
                    let mut read = f.scale_points.clone();
                    read.sort_by(|a, b| a.1.total_cmp(&b.1));
                    let described: Vec<(String, f32)> =
                        choices.iter().map(|c| (c.label.to_string(), c.value)).collect();
                    if read != described {
                        wrong(&format!("choices {read:?}"));
                    }
                }
            }
            PortKind::Enabled if !f.toggled => wrong("not toggled"),
            PortKind::Events { midi, time } if (f.midi, f.time) != (midi, time) => {
                wrong(&format!("events midi={} time={}", f.midi, f.time));
            }
            _ => {}
        }
    }
    problems
}

fn presets(info: &PluginInfo, found: &scanner::PluginInfo) -> Vec<String> {
    let mut problems = Vec::new();
    if found.presets.len() != info.presets.len() {
        problems.push(format!("{} presets, {} described", found.presets.len(), info.presets.len()));
    }
    for preset in info.presets {
        let Some(read) = found.presets.iter().find(|p| p.label == preset.name) else {
            problems.push(format!("{:?} missing", preset.name));
            continue;
        };
        for (i, value) in info.preset_values(preset) {
            let symbol = info.ports[i].symbol;
            let got = read.values.iter().find(|(s, _)| s == symbol).map(|(_, v)| *v);
            if got != Some(value) {
                problems.push(format!("{:?}: {symbol} read as {got:?}, described {value}", preset.name));
            }
        }
    }
    problems
}

/// Blocks of each run: 2 s at 48 kHz
const BLOCKS: usize = 750;

/// A change applied before a block: (block, port, value)
type Change = (usize, usize, f32);

/// From a third of the run every continuous parameter moves to 30 % of
/// its range; from two thirds the plugin is bypassed (lv2:enabled = 0)
fn staged_changes(info: &PluginInfo) -> Vec<Change> {
    let mut changes = Vec::new();
    for (i, p) in info.ports.iter().enumerate() {
        match p.kind {
            PortKind::Param(param) if param.kind == ParamKind::Float => {
                changes.push((BLOCKS / 3, i, param.from_normalized(0.3)));
            }
            PortKind::Enabled => changes.push((2 * BLOCKS / 3, i, 0.0)),
            _ => {}
        }
    }
    changes
}

/// Guitar-like test signal: two partials and a slow swell
fn signal(frames: usize, seed: f32) -> Vec<f32> {
    (0..frames)
        .map(|n| {
            let t = n as f32 / RATE;
            let swell = 0.6 + 0.4 * (std::f32::consts::TAU * 0.7 * t).sin();
            swell
                * (0.4 * (std::f32::consts::TAU * (220.0 + seed) * t).sin()
                    + 0.1 * (std::f32::consts::TAU * 1375.0 * t).sin())
        })
        .collect()
}

/// The transport as OpenRig writes it (`atom::write_input`)
fn position() -> Event {
    let p = Position {
        bpm: Some(BPM),
        speed: Some(1.0),
        bar_beat: Some(0.0),
        beats_per_bar: Some(4.0),
        beat_unit: Some(4),
    };
    Event { frame: 0, kind: EventKind::Position(p) }
}

/// Value of every control input, by port: `values` by symbol, defaults
/// for the others
fn controls(info: &PluginInfo, values: &[(&str, f32)]) -> Vec<(usize, f32)> {
    info.defaults()
        .enumerate()
        .filter(|(i, _)| is_input_control(info, *i))
        .map(|(i, default)| {
            let symbol = info.ports[i].symbol;
            (i, values.iter().find(|(s, _)| *s == symbol).map_or(default, |(_, v)| *v))
        })
        .collect()
}

/// A control input: what OpenRig calls a parameter
fn is_input_control(info: &PluginInfo, port: usize) -> bool {
    matches!(info.ports[port].kind, PortKind::Param(_) | PortKind::Enabled)
}

/// Index of a port among the control inputs: OpenRig's parameter index
fn param_index(info: &PluginInfo, port: usize) -> usize {
    (0..port).filter(|i| is_input_control(info, *i)).count()
}

/// Run `entry` alone in an EffectChain and in its runner, from `values`
/// (port, value) with `changes`; the differences found.
///
/// The run lasts `BLOCKS` blocks. With a bypass among the changes it
/// goes on until the runner is idle (for a plugin with trails, once its
/// tail has rung out, at most `TRAIL_LIMIT_S`), then a sixth of `BLOCKS`
/// more, where both must play the input delayed by the latency.
fn compare(host: &Lv2Host, entry: &Entry, values: &[(usize, f32)], changes: &[Change], stereo: bool) -> Vec<String> {
    let info = entry.info;
    let bypass = changes.iter().find(|c| info.ports[c.1].kind == PortKind::Enabled).map(|c| c.0);
    // Without a bypass, `BLOCKS`; with one, up to the trail limit and the
    // settled stretch, cut short once the runner is idle
    let settled = BLOCKS / 6;
    let mut end = bypass.map_or(BLOCKS, |b| {
        let trail = ((TRAIL_LIMIT_S + 1.0) * f64::from(RATE) / BLOCK as f64).ceil() as usize;
        BLOCKS.max(b + trail + settled)
    });
    let input = signal(end * BLOCK, 0.0);

    // The plugin's runner, fed as OpenRig feeds it: a mono signal into
    // every input
    let mut runner = runner(entry, values);
    let outputs = info.audio_outputs();
    let mut expected = vec![vec![0.0; end * BLOCK]; outputs];
    let mut idle_from = None;
    let mut blocks = 0;
    while blocks < end {
        for &(_, port, value) in changes.iter().filter(|c| c.0 == blocks) {
            runner.set_param(port, value);
        }
        let span = blocks * BLOCK..(blocks + 1) * BLOCK;
        run_block(runner.as_mut(), &input[span.clone()], &mut expected, span);
        blocks += 1;
        if idle_from.is_none() && bypass.is_some_and(|from| blocks > from) && runner.is_idle() {
            idle_from = Some(blocks);
            end = BLOCKS.max(blocks + settled);
        }
    }
    let frames = blocks * BLOCK;
    let input = &input[..frames];
    for o in expected.iter_mut() {
        o.truncate(frames);
    }

    // OpenRig
    let id = 1;
    // `values` covers every control input, in port order
    let params: Vec<f32> = values.iter().map(|(_, v)| *v).collect();
    let spec = ChainSpec {
        generation: 1,
        nodes: vec![ChainNode::Plugin {
            instance_id: id,
            uri: info.uri.to_string(),
            enabled: true,
            num_inputs: info.audio_inputs(),
            num_outputs: info.audio_outputs(),
        }],
        params: HashMap::from([(id, params)]),
        stereo,
        bpm: BPM,
        ..Default::default()
    };
    let (mut chain, errors) = EffectChain::build(&spec, host, RATE, BLOCK);
    if !errors.is_empty() {
        return vec![format!("EffectChain::build: {errors:?}")];
    }
    chain.apply(&ChainCommand::SetTempo { bpm: BPM });
    let (mut left, mut right) = (input.to_vec(), input.to_vec());
    for (b, (l, r)) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)).enumerate() {
        for &(_, port, value) in changes.iter().filter(|c| c.0 == b) {
            let index = param_index(info, port);
            chain.apply(&ChainCommand::SetParameter { instance_id: id, index, value });
        }
        chain.process(l, r);
    }

    let mut problems = Vec::new();
    let (want_l, want_r) = match (stereo, outputs) {
        (true, 2..) => (expected[0].clone(), expected[1].clone()),
        (false, 2..) => {
            // A stereo plugin in a mono chain: OpenRig keeps the mix
            let mix: Vec<f32> = expected[0].iter().zip(&expected[1]).map(|(a, b)| (a + b) * 0.5).collect();
            (mix.clone(), mix)
        }
        _ => (expected[0].clone(), expected[0].clone()),
    };
    differences("left", &left, &want_l, &mut problems);
    if stereo {
        differences("right", &right, &want_r, &mut problems);
    }
    if left.iter().all(|x| *x == 0.0) {
        problems.push("silent".into());
    }

    // Bypassed by its lv2:enabled port, the plugin plays its input
    // (delayed by its latency) once idle
    if let Some(from) = bypass {
        match idle_from {
            None => problems.push(format!(
                "still running {:.1} s after the bypass",
                ((blocks - from) * BLOCK) as f32 / RATE
            )),
            Some(idle) => {
                let latency = runner.latency() as usize;
                if (idle * BLOCK..frames).any(|n| left[n] != input[n - latency]) {
                    problems.push(format!("bypassed output is not the input delayed by {latency} frames"));
                }
            }
        }
    }
    problems
}

/// One block of the runner: `input` into every input, outputs written at
/// `span` of `outputs`
fn run_block(runner: &mut dyn DynRunner, input: &[f32], outputs: &mut [Vec<f32>], span: std::ops::Range<usize>) {
    let inputs: Vec<&[f32]> = (0..runner.info().audio_inputs()).map(|_| input).collect();
    let mut outs: Vec<&mut [f32]> = outputs.iter_mut().map(|o| &mut o[span.clone()]).collect();
    runner.process(input.len(), &inputs, &mut outs, &mut NoStreams);
}

/// Note where `got` and `want` differ
fn differences(side: &str, got: &[f32], want: &[f32], problems: &mut Vec<String>) {
    if let Some(n) = got.iter().zip(want).position(|(a, b)| a.to_bits() != b.to_bits()) {
        let worst = got.iter().zip(want).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        problems.push(format!("{side}: differs from frame {n} ({} vs {}), at most by {worst:e}", got[n], want[n]));
    }
}

/// The runner of `entry`, active, with `values` and the transport
fn runner(entry: &Entry, values: &[(usize, f32)]) -> Box<dyn DynRunner> {
    let mut r = (entry.create)(Config { sample_rate: f64::from(RATE), max_block: BLOCK });
    r.activate();
    for &(port, value) in values {
        r.set_param(port, value);
    }
    r.push_event(position());
    r
}

/// Outputs of `entry`'s runner on `input` (into every input), by block
fn render(entry: &Entry, values: &[(usize, f32)], input: &[f32]) -> Vec<Vec<f32>> {
    let mut r = runner(entry, values);
    let mut outputs = vec![vec![0.0; input.len()]; entry.info.audio_outputs()];
    for (b, block) in input.chunks(BLOCK).enumerate() {
        let span = b * BLOCK..b * BLOCK + block.len();
        run_block(r.as_mut(), block, &mut outputs, span);
    }
    outputs
}

/// A mono plugin after a stereo one: OpenRig runs it once per side (two
/// instances). Each side must be the mono runner on that side. The stereo
/// plugin plays its first preset whose sides differ.
fn dual_mono(host: &Lv2Host, out: &mut Out) {
    let plugins: Vec<&Entry> = orx_suite::plugins().collect();
    let source = plugins.iter().find(|e| e.info.audio_outputs() == 2);
    let mono = plugins.iter().find(|e| e.info.audio_inputs() == 1 && e.info.audio_outputs() == 1);
    let (Some(source), Some(mono)) = (source, mono) else {
        return out.note("dual-mono", "needs a stereo and a mono plugin in the suite");
    };
    let input = signal(BLOCKS * BLOCK, 0.0);
    let stereo = source.info.presets.iter().find_map(|preset| {
        let values: Vec<(usize, f32)> = controls(source.info, preset.values);
        let sides = render(source, &values, &input);
        (sides[0] != sides[1]).then_some((preset, values, sides))
    });
    let Some((preset, values, sides)) = stereo else {
        return out.note("dual-mono", &format!("no preset of {} plays a stereo signal", source.slug()));
    };

    let node = |id: usize, info: &PluginInfo| ChainNode::Plugin {
        instance_id: id,
        uri: info.uri.to_string(),
        enabled: true,
        num_inputs: info.audio_inputs(),
        num_outputs: info.audio_outputs(),
    };
    let spec = ChainSpec {
        generation: 1,
        nodes: vec![node(1, source.info), node(2, mono.info)],
        params: HashMap::from([(1, values.iter().map(|(_, v)| *v).collect())]),
        stereo: true,
        bpm: BPM,
        ..Default::default()
    };
    let (mut chain, errors) = EffectChain::build(&spec, host, RATE, BLOCK);
    if !errors.is_empty() {
        return out.fail("dual-mono", &format!("EffectChain::build: {errors:?}"));
    }
    chain.apply(&ChainCommand::SetTempo { bpm: BPM });
    let (mut left, mut right) = (input.clone(), input);
    for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
        chain.process(l, r);
    }

    let defaults = controls(mono.info, &[]);
    let mut problems = Vec::new();
    differences("left", &left, &render(mono, &defaults, &sides[0])[0], &mut problems);
    differences("right", &right, &render(mono, &defaults, &sides[1])[0], &mut problems);
    out.verdict(
        "dual-mono",
        &format!("{} ({}) then {} on each side, bit-exact", source.slug(), preset.name, mono.slug()),
        &problems,
    );
}
