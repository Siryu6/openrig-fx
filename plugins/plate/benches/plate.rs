//! CPU cost of OR Plate: `cargo bench -p orx-plate`
//!
//! At 48 kHz in 32- and 128-frame blocks, with the filters on and the
//! modulation active, so every path is measured.

use orx_plate::Plate;
use orx_plugin::Plugin;

fn main() {
    let info = Plate::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    let setup = [(at("decay"), 4.0), (at("low_cut"), 120.0), (at("high_cut"), 8000.0), (at("modulation"), 50.0)];
    for block in [32, 128] {
        let r = orx_test::bench::run::<Plate>(48_000.0, block, &setup);
        println!("{:<16} {r}", "OR Plate");
    }
}
