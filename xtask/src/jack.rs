//! Each plugin under jalv on a private dummy JACK server, with audio
//! played through it and recorded by a probe client

use crate::paths;
use crate::report::Report;
use orx_plugin::PortKind;
use orx_plugin::dynamic::Entry;
use orx_standalone::dummy::{DummyServer, stop};
use orx_standalone::probe;
use orx_suite::BundleDef;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SR: u32 = 48_000;

pub fn check(def: &BundleDef, dir: &Path, report: &mut Report) {
    for tool in ["jackd", "jalv"] {
        if !paths::has_tool(tool) {
            return report.skip("jalv", &format!("{tool} not installed"));
        }
    }
    // SAFETY: xtask runs no thread besides main (child processes only),
    // so nothing reads the environment concurrently
    let server = match unsafe { DummyServer::start("xtask", SR, 256) } {
        Ok(s) => s,
        Err(e) => return report.fail("jalv", &e),
    };
    for e in def.plugins {
        // From the first preset, set as controls: the preset's values
        // without relying on how a jalv version loads presets
        let (what, controls) = match e.info.presets.first() {
            Some(p) => (
                format!("{} ({})", e.info.uri, p.name),
                e.info.preset_values(p).map(|(i, v)| format!("{}={v}", e.info.ports[i].symbol)).collect(),
            ),
            None => (e.info.uri.to_string(), Vec::new()),
        };
        let controls: Vec<&str> = controls.iter().map(String::as_str).collect();
        match run(e, dir, &controls).and_then(|r| sounds(&r)) {
            Ok(result) => report.pass("jalv", &format!("{what}: {result}")),
            Err(why) => report.fail("jalv", &format!("{what}: {why}")),
        }
        // Bypassed through jalv's control: the input, exactly, a few
        // periods late
        if e.info.ports.iter().any(|p| p.kind == PortKind::Enabled) {
            match run(e, dir, &["enabled=0"]).and_then(|r| transparent(&r)) {
                Ok(result) => report.pass("jalv", &format!("{} bypassed: {result}", e.info.uri)),
                Err(why) => report.fail("jalv", &format!("{} bypassed: {why}", e.info.uri)),
            }
        }
    }
    drop(server);
}

/// The jalv process, stopped on drop, and what it printed
struct Jalv(Child);

impl Jalv {
    fn output(mut self) -> String {
        stop(&mut self.0);
        let mut text = String::new();
        if let Some(mut s) = self.0.stdout.take() {
            let _ = s.read_to_string(&mut text);
        }
        if let Some(mut s) = self.0.stderr.take() {
            let _ = s.read_to_string(&mut text);
        }
        text
    }
}

impl Drop for Jalv {
    fn drop(&mut self) {
        stop(&mut self.0);
    }
}

fn sine(frames: usize) -> Vec<f32> {
    (0..frames).map(|i| 0.5 * (std::f32::consts::TAU * 220.0 * i as f32 / SR as f32).sin()).collect()
}

/// Run jalv on the plugin with `controls` (SYMBOL=VALUE), play one second
/// of a 220 Hz sine through it; what each output recorded
fn run(e: &Entry, dir: &Path, controls: &[&str]) -> Result<Vec<Vec<f32>>, String> {
    let client = "orx-jalv";
    let mut args = vec!["-i", "-x", "-n", client];
    for c in controls {
        args.extend(["-c", c]);
    }
    args.push(e.info.uri);
    let child = Command::new("jalv")
        .args(&args)
        .env("LV2_PATH", paths::lv2_path(dir.parent().unwrap_or(dir)))
        .env("JACK_NO_START_SERVER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jalv: {e}"))?;
    let mut jalv = Jalv(child);
    let ports = |kind: PortKind| -> Vec<String> {
        e.info.ports.iter().filter(|p| p.kind == kind).map(|p| format!("{client}:{}", p.symbol)).collect()
    };
    let (ins, outs) = (ports(PortKind::AudioIn), ports(PortKind::AudioOut));
    let ins: Vec<&str> = ins.iter().map(String::as_str).collect();
    let outs: Vec<&str> = outs.iter().map(String::as_str).collect();
    let frames = SR as usize;
    let sine = sine(frames);

    // Probe until jalv has registered its ports
    let deadline = Instant::now() + Duration::from_secs(10);
    let recorded = loop {
        if let Ok(Some(status)) = jalv.0.try_wait() {
            return Err(format!("jalv exited ({status})\n{}", jalv.output()));
        }
        match probe::run(&ins, &outs, vec![sine.clone(); ins.len()], frames) {
            Ok(r) => break r,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            Err(why) => return Err(format!("{why}\n{}", jalv.output())),
        }
    };
    let alive = matches!(jalv.0.try_wait(), Ok(None));
    drop(jalv);
    if alive { Ok(recorded) } else { Err("jalv stopped while processing".into()) }
}

/// Finite and not silent, from 0.1 s on (the pipeline is a period or two
/// late)
fn sounds(recorded: &[Vec<f32>]) -> Result<String, String> {
    let mut peaks = Vec::new();
    for (k, r) in recorded.iter().enumerate() {
        let tail = &r[r.len() / 10..];
        if let Some(x) = tail.iter().find(|x| !x.is_finite()) {
            return Err(format!("output {k} is {x}"));
        }
        let peak = tail.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        if peak < 1e-3 {
            return Err(format!("output {k} is silent"));
        }
        peaks.push(format!("{peak:.2}"));
    }
    Ok(format!("1 s of audio through jalv, output peaks {}", peaks.join(", ")))
}

/// Every output is the input sine, bit for bit, delayed by a whole
/// number of frames (the JACK graph's)
fn transparent(recorded: &[Vec<f32>]) -> Result<String, String> {
    let input = sine(recorded.first().map_or(0, Vec::len));
    let mut delays = Vec::new();
    for (k, r) in recorded.iter().enumerate() {
        let start = r.iter().position(|x| *x != 0.0).ok_or(format!("output {k} is silent"))?;
        // The sine starts at 0: the first non-zero output is input frame 1
        let delay = start.checked_sub(1).ok_or(format!("output {k} does not start with the input's 0"))?;
        if r[delay..] != input[..r.len() - delay] {
            return Err(format!("output {k} is not the input delayed by {delay} frames"));
        }
        delays.push(delay.to_string());
    }
    Ok(format!("outputs equal the input delayed by {} frames", delays.join(", ")))
}
