//! CPU cost of OR Gate: `cargo bench -p orx-gate`
//!
//! At 48 kHz in 32- and 128-frame blocks, with a threshold the bench
//! signal crosses, so the gate opens and closes.

use orx_gate::Gate;
use orx_plugin::Plugin;

fn main() {
    let info = Gate::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    let setup = [(at("threshold"), -30.0), (at("decay"), 50.0), (at("hold"), 5.0)];
    for block in [32, 128] {
        let r = orx_test::bench::run::<Gate>(48_000.0, block, &setup);
        println!("{:<16} {r}", "OR Gate");
    }
}
