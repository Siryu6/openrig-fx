//! CPU cost of OR Echo: `cargo bench -p orx-echo`
//!
//! Each mode at 48 kHz in 32- and 128-frame blocks, with the loop
//! filters on, modulation and ducking active, so every path is measured.

use orx_echo::{Mono, Stereo};
use orx_plugin::Plugin;

fn run<P: Plugin>(label: &str) {
    let info = P::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    for (name, mode) in [("digital", 0.0), ("tape", 1.0), ("analog", 2.0)] {
        let setup = [
            (at("mode"), mode),
            (at("feedback"), 60.0),
            (at("low_cut"), 100.0),
            (at("high_cut"), 6000.0),
            (at("modulation"), 50.0),
            (at("duck"), 10.0),
            (at("ping_pong"), 1.0),
        ];
        for block in [32, 128] {
            let r = orx_test::bench::run::<P>(48_000.0, block, &setup);
            println!("{label:<16} {name:<8} {r}");
        }
    }
}

fn main() {
    run::<Mono>("OR Echo");
    run::<Stereo>("OR Echo Stereo");
}
