//! `orx-run`: one OpenRig FX plugin as a JACK client.
//!
//! ```text
//! orx-run --list
//! orx-run PLUGIN [--name NAME] [--bpm BPM] [--set SYMBOL=VALUE]... [--preset ID]
//!                [--cc NUMBER=SYMBOL]... [--connect]
//! ```
//!
//! PLUGIN is a slug (`tremolo`, `tremolo-stereo`) or a URI. Commands on
//! standard input, one per line: `set SYMBOL VALUE` (or `SYMBOL=VALUE`),
//! `preset ID`, `bypass on|off`, `params`, `meters`, `presets`, `quit`.
//! It runs until `quit`, SIGINT or SIGTERM (not at the end of its input,
//! so it also runs in the background).

use orx_plugin::{PluginInfo, PortKind};
use orx_standalone::{Options, Standalone};
use std::io::BufRead;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: i32) {
    STOP.store(true, Ordering::Relaxed);
}

unsafe extern "C" {
    fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
}

const USAGE: &str = "usage: orx-run --list
       orx-run PLUGIN [--name NAME] [--bpm BPM] [--set SYMBOL=VALUE]... [--preset ID] [--cc NUMBER=SYMBOL]... [--connect]";

struct Args {
    plugin: &'static orx_plugin::dynamic::Entry,
    options: Options,
    sets: Vec<(usize, f32)>,
}

fn port(info: &PluginInfo, symbol: &str) -> Result<usize, String> {
    info.port_index(symbol).ok_or_else(|| format!("{}: no port `{symbol}`", info.name))
}

fn parse_value(s: &str) -> Result<f32, String> {
    s.parse().map_err(|_| format!("`{s}` is not a number"))
}

/// The parameter values of preset `id`
fn preset(info: &PluginInfo, id: &str) -> Result<Vec<(usize, f32)>, String> {
    let p = info.presets.iter().find(|p| p.id == id).ok_or_else(|| format!("{}: no preset `{id}`", info.name))?;
    Ok(info.preset_values(p).collect())
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut it = args.iter();
    let name = it.next().ok_or(USAGE)?;
    let plugin = orx_suite::find(name).ok_or_else(|| format!("no plugin `{name}` (see --list)"))?;
    let info = plugin.info;
    let mut options = Options::default();
    let mut sets = Vec::new();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--name" => options.name = Some(value()?.clone()),
            "--bpm" => options.bpm = Some(parse_value(value()?)?),
            "--connect" => options.connect = true,
            "--preset" => sets.extend(preset(info, value()?)?),
            "--set" => {
                let (s, v) = value()?.split_once('=').ok_or("--set takes SYMBOL=VALUE")?;
                sets.push((port(info, s)?, parse_value(v)?));
            }
            "--cc" => {
                let (n, s) = value()?.split_once('=').ok_or("--cc takes NUMBER=SYMBOL")?;
                let n: u8 = n.parse().ok().filter(|n| *n < 128).ok_or("CC numbers are 0 to 127")?;
                options.cc.push((n, port(info, s)?));
            }
            _ => return Err(format!("unknown option {a}\n{USAGE}")),
        }
    }
    Ok(Args { plugin, options, sets })
}

fn list() {
    for e in orx_suite::plugins() {
        println!("{:<16} {:<22} {}", e.slug(), e.info.name, e.info.uri);
    }
}

fn print_values(host: &Standalone, meters: bool) {
    let info = host.info();
    for (i, p) in info.ports.iter().enumerate() {
        let wanted = match p.kind {
            PortKind::Param(_) | PortKind::Enabled => !meters,
            PortKind::Meter(_) | PortKind::Latency => meters,
            _ => false,
        };
        if wanted && let Some(v) = host.value(i) {
            let label =
                p.as_param().and_then(|param| param.choice_label(v)).map(|l| format!(" ({l})")).unwrap_or_default();
            println!("{:<12} {v}{label}", p.symbol);
        }
    }
}

/// Run one command line; false to quit
fn command(host: &mut Standalone, line: &str) -> Result<bool, String> {
    let info = host.info();
    let words: Vec<&str> = line.split_whitespace().collect();
    let set = |host: &mut Standalone, i: usize, v: f32| {
        if host.set(i, v) { Ok(()) } else { Err(format!("cannot set {}", info.ports[i].symbol)) }
    };
    match words.as_slice() {
        [] => {}
        ["quit" | "exit"] => return Ok(false),
        ["params"] => print_values(host, false),
        ["meters"] => print_values(host, true),
        ["presets"] => info.presets.iter().for_each(|p| println!("{:<16} {}", p.id, p.name)),
        ["preset", id] => {
            for (i, v) in preset(info, id)? {
                set(host, i, v)?;
            }
        }
        ["bypass", on @ ("on" | "off")] => set(host, port(info, "enabled")?, if *on == "on" { 0.0 } else { 1.0 })?,
        ["set", symbol, value] => set(host, port(info, symbol)?, parse_value(value)?)?,
        [assignment] if assignment.contains('=') => {
            let (s, v) = assignment.split_once('=').unwrap_or_default();
            set(host, port(info, s)?, parse_value(v)?)?;
        }
        _ => return Err(format!("unknown command `{line}` (set, preset, bypass, params, meters, presets, quit)")),
    }
    Ok(true)
}

fn run(args: Args) -> Result<(), String> {
    let mut host = Standalone::start(args.plugin, &args.options)?;
    for (i, v) in args.sets {
        if !host.set(i, v) {
            return Err(format!("cannot set {}", host.info().ports[i].symbol));
        }
    }
    eprintln!("{} running as JACK client `{}`", host.info().name, host.name());
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    while !STOP.load(Ordering::Relaxed) {
        if host.shut_down() {
            return Err("the JACK server shut down".into());
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => match command(&mut host, &line) {
                Ok(true) => {}
                Ok(false) => break,
                Err(e) => eprintln!("{e}"),
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // Input ended: keep running until a signal
            Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    if host.faults() > 0 {
        eprintln!("{} non-finite blocks replaced with silence", host.faults());
    }
    Ok(())
}

fn main() -> ExitCode {
    // SAFETY: installing a handler that only stores to an atomic, which is
    // async-signal-safe; SIGINT is 2 and SIGTERM 15 on every Unix
    unsafe {
        signal(2, on_signal);
        signal(15, on_signal);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--list") {
        list();
        return ExitCode::SUCCESS;
    }
    if args.is_empty() || args.first().is_some_and(|a| a == "--help" || a == "-h") {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    match parse(&args).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("orx-run: {e}");
            ExitCode::FAILURE
        }
    }
}
