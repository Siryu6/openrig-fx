//! `orx-run` end to end, on a private dummy JACK server: audio through
//! the plugin, and commands from standard input. Needs `jackd`; skipped
//! (with a message) without it.
#![allow(clippy::expect_used)]

use orx_standalone::dummy::{DummyServer, stop};
use orx_standalone::probe;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SR: u32 = 48_000;

/// Smallest and largest level over 5 ms windows, skipping the first `skip` frames
fn envelope(x: &[f32], skip: usize) -> (f32, f32) {
    x[skip..]
        .chunks(240)
        .filter(|c| c.len() == 240)
        .map(|c| c.iter().map(|v| v.abs()).fold(0.0, f32::max))
        .fold((f32::MAX, 0.0f32), |(lo, hi), p| (lo.min(p), hi.max(p)))
}

fn start(name: &str, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_orx-run"))
        .args(["tremolo", "--name", name])
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("orx-run starts")
}

/// Probe the client until its ports exist
fn measure(name: &str, frames: usize) -> Vec<f32> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let input = format!("{name}:in");
        let output = format!("{name}:out");
        match probe::run(&[&input], &[&output], vec![vec![0.5; frames]], frames) {
            Ok(mut rec) => return rec.remove(0),
            // Not up yet
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => panic!("probe: {e}"),
        }
    }
}

#[test]
fn runs_under_jack() {
    if !std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("jackd").is_file())) {
        eprintln!("jackd not installed: skipped");
        return;
    }
    // SAFETY: the only test of this binary, before any other thread
    let _server = unsafe { DummyServer::start("orx-run-test", SR, 256) }.expect("dummy JACK server");
    let name = "orx-run-test";
    let mut child = start(name, &["--set", "depth=100", "--set", "rate=4", "--set", "shape=0"]);

    // Classic tremolo at full depth on a constant 0.5: the level swings
    // from silence to 0.5 four times a second
    let out = measure(name, SR as usize);
    let (lo, hi) = envelope(&out, 4_800);
    assert!(lo < 0.01 && (0.49..=0.5).contains(&hi), "envelope {lo}..{hi}");

    // Depth 0 from standard input: transparent
    let stdin = child.stdin.as_mut().expect("piped stdin");
    writeln!(stdin, "set depth 0").expect("command sent");
    stdin.flush().expect("flushed");
    std::thread::sleep(Duration::from_millis(200));
    let out = measure(name, SR as usize / 2);
    let (lo, hi) = envelope(&out, 4_800);
    assert!(lo > 0.4999 && hi < 0.5001, "envelope {lo}..{hi}");

    writeln!(child.stdin.as_mut().expect("piped stdin"), "quit").expect("quit sent");
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let status = child.try_wait().ok().flatten();
    if status.is_none() {
        stop(&mut child);
    }
    assert!(status.is_some_and(|s| s.success()), "orx-run exit: {status:?}");
}
