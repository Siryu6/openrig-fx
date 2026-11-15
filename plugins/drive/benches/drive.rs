//! CPU cost of OR Drive: `cargo bench -p orx-drive`
//!
//! At 48 kHz in 32- and 128-frame blocks, each mode at a high drive.

use orx_drive::{Drive, Mode};
use orx_plugin::Plugin;

fn main() {
    let info = Drive::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    for block in [32, 128] {
        for mode in Mode::ALL {
            let setup = [(at("mode"), mode as i32 as f32), (at("drive"), 80.0), (at("bass"), 3.0)];
            let r = orx_test::bench::run::<Drive>(48_000.0, block, &setup);
            println!("{:<22} {r}", format!("OR Drive {mode:?}"));
        }
    }
}
