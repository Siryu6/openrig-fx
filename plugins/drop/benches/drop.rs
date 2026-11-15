//! CPU cost of OR Drop: `cargo bench -p orx-drop`
//!
//! At 48 kHz in 32- and 128-frame blocks, both modes, a drop and an
//! octave down (more splices).

use orx_drop::DropPedal;
use orx_plugin::Plugin;

fn main() {
    let info = DropPedal::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    for (name, mode) in [("tight", 0.0), ("quality", 1.0)] {
        for shift in [-2.0, -12.0] {
            let setup = [(at("mode"), mode), (at("shift"), shift)];
            for block in [32, 128] {
                let r = orx_test::bench::run::<DropPedal>(48_000.0, block, &setup);
                println!("{:<16} {r}", format!("{name} {shift}"));
            }
        }
    }
}
