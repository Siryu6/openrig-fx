//! CPU cost of OR Green Drive: `cargo bench -p orx-green-drive`
//!
//! At 48 kHz in 32- and 128-frame blocks, at the middle and at the top of
//! Drive (more Newton iterations as the diodes conduct harder).

use orx_green_drive::GreenDrive;
use orx_plugin::Plugin;

fn main() {
    let info = GreenDrive::INFO;
    let at = |s: &str| info.port_index(s).unwrap_or(usize::MAX);
    for drive in [50.0, 100.0] {
        let setup = [(at("drive"), drive)];
        for block in [32, 128] {
            let r = orx_test::bench::run::<GreenDrive>(48_000.0, block, &setup);
            println!("{:<16} {r}", format!("drive {drive}"));
        }
    }
}
