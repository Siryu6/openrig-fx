//! CPU cost of OR Black Dist: `cargo bench -p orx-black-dist`
//!
//! At 48 kHz in 32- and 128-frame blocks, at the middle and at the top of
//! Distortion, for each diode choice.

use orx_black_dist::BlackDist;
use orx_plugin::Plugin;

fn main() {
    let info = BlackDist::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    for (diodes, name) in [(0.0, "silicon"), (1.0, "led"), (2.0, "none")] {
        for dist in [50.0, 100.0] {
            let setup = [(at("distortion"), dist), (at("voicing"), diodes)];
            for block in [32, 128] {
                let r = orx_test::bench::run::<BlackDist>(48_000.0, block, &setup);
                println!("{:<20} {r}", format!("{name} dist {dist}"));
            }
        }
    }
}
