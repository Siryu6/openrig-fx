//! CPU cost of OR Comp: `cargo bench -p orx-comp`
//!
//! At 48 kHz in 32- and 128-frame blocks, with a threshold the bench
//! signal crosses, compressing with a blend.

use orx_comp::Comp;
use orx_plugin::Plugin;

fn main() {
    let info = Comp::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    let setup = [(at("threshold"), -40.0), (at("ratio"), 8.0), (at("mix"), 60.0)];
    for block in [32, 128] {
        let r = orx_test::bench::run::<Comp>(48_000.0, block, &setup);
        println!("{:<16} {r}", "OR Comp");
    }
}
