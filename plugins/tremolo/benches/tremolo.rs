//! CPU cost of OR Tremolo: `cargo bench -p orx-tremolo`
//!
//! Each mode at 48 kHz in 128-frame blocks (OpenRig's usual period), the
//! LFO at 5 Hz with half shape, so the tanh path is measured.

use orx_plugin::Plugin;
use orx_tremolo::{Mono, Stereo};

fn run<P: Plugin>(label: &str) {
    let info = P::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    let modes: &[(&str, f32)] = if info.audio_inputs() == 2 {
        &[("classic", 0.0), ("opto", 1.0), ("harmonic", 2.0), ("pan", 3.0)]
    } else {
        &[("classic", 0.0), ("opto", 1.0), ("harmonic", 2.0)]
    };
    for (name, mode) in modes {
        let setup = [(at("mode"), *mode), (at("depth"), 80.0), (at("shape"), 50.0)];
        for block in [32, 128] {
            let r = orx_test::bench::run::<P>(48_000.0, block, &setup);
            println!("{label:<16} {name:<9} {r}");
        }
    }
}

fn main() {
    run::<Mono>("OR Tremolo");
    run::<Stereo>("OR Tremolo Stereo");
}
