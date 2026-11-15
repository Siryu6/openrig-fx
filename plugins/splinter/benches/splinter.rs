//! CPU cost of OR Splinter: `cargo bench -p orx-splinter`
//!
//! At 48 kHz in 32- and 128-frame blocks: idle, repeating a slice with a
//! pitch step and chaos, the tape stop, and the chop and crush always on.

use orx_plugin::Plugin;
use orx_splinter::Splinter;

fn main() {
    let info = Splinter::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    let cases: [(&str, Vec<(usize, f32)>); 4] = [
        ("idle", vec![]),
        ("repeat", vec![(at("hold"), 1.0), (at("slice"), 10.0), (at("step"), -1.0), (at("chaos"), 50.0)]),
        ("tape stop", vec![(at("hold"), 1.0), (at("action"), 2.0), (at("stop_time"), 4.0)]),
        ("chop+crush", vec![(at("scope"), 1.0), (at("pattern"), 4.0), (at("bits"), 8.0), (at("rate"), 12_000.0)]),
    ];
    for (name, setup) in &cases {
        for block in [32, 128] {
            let r = orx_test::bench::run::<Splinter>(48_000.0, block, setup);
            println!("{name:<16} {r}");
        }
    }
}
