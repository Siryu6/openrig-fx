//! CPU cost of OR EQ and OR Graphic EQ: `cargo bench -p orx-eq`
//!
//! At 48 kHz in 32- and 128-frame blocks, with every band active.

use orx_eq::{Graphic, Parametric};
use orx_plugin::Plugin;

fn main() {
    let p = Parametric::INFO;
    let at = |s: &str| p.port_index(s).unwrap_or(usize::MAX);
    let parametric = [
        (at("low_cut_on"), 1.0),
        (at("high_cut_on"), 1.0),
        (at("low_gain"), 3.0),
        (at("low_mid_gain"), -2.0),
        (at("mid_gain"), 4.0),
        (at("high_mid_gain"), -3.0),
        (at("high_gain"), 2.0),
    ];
    let g = Graphic::INFO;
    let graphic: Vec<(usize, f32)> = (0..7).map(|k| (g.port_index(g.ports[2 + k].symbol).unwrap_or(0), 3.0)).collect();
    for block in [32, 128] {
        let r = orx_test::bench::run::<Parametric>(48_000.0, block, &parametric);
        println!("{:<16} {r}", "OR EQ");
        let r = orx_test::bench::run::<Graphic>(48_000.0, block, &graphic);
        println!("{:<16} {r}", "OR Graphic EQ");
    }
}
