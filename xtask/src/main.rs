//! `cargo xtask`: build and check the LV2 bundles of OpenRig FX.
//!
//! ```text
//! cargo xtask bundle [--target TRIPLE] [--strict]   test, build, assemble and validate every bundle
//! cargo xtask validate [--strict] DIR.lv2...        validate assembled bundles
//! cargo xtask vocab [--check]                       write (or compare) ns/openrig-view.ttl
//! cargo xtask openrig-host OPENRIG_DIR               run the bundles in OpenRig's LV2 host
//! ```
//!
//! Validation runs every check that applies: Turtle syntax (serdi), the
//! LV2 schemas (lv2_validate), `openrig:` terms against the vocabulary,
//! the ELF dependencies and glibc symbol versions, then, for a binary of
//! this machine's architecture, loading it through the in-process test
//! host, lv2lint, and a run under jalv on a private dummy JACK server.
//! A check whose tool is missing is reported as skipped; `--strict`
//! turns skips into failures (CI).

mod bundle;
mod elf;
mod jack;
mod load;
mod openrig;
mod paths;
mod report;
mod validate;

use report::Report;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!(
        "usage:\n  cargo xtask bundle [--target TRIPLE] [--strict]\n  cargo xtask validate [--strict] DIR.lv2...\n  cargo xtask vocab [--check]\n  cargo xtask openrig-host OPENRIG_DIR"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let strict = args.iter().any(|a| a == "--strict");
    let mut report = Report::new(strict);
    let result = match args.first().map(String::as_str) {
        Some("bundle") => {
            let target = args.iter().position(|a| a == "--target").and_then(|i| args.get(i + 1)).cloned();
            bundle::run(target.as_deref(), &mut report)
        }
        Some("validate") => {
            let dirs: Vec<&String> = args[1..].iter().filter(|a| !a.starts_with("--")).collect();
            if dirs.is_empty() {
                return usage();
            }
            dirs.iter().try_for_each(|d| validate::bundle(std::path::Path::new(d), &mut report))
        }
        Some("openrig-host") => match args.get(1) {
            Some(dir) => openrig::run(std::path::Path::new(dir), &mut report),
            None => return usage(),
        },
        Some("vocab") => vocab(args.iter().any(|a| a == "--check"), &mut report),
        _ => return usage(),
    };
    if let Err(e) = result {
        report.fail("xtask", &e);
    }
    report.finish()
}

fn vocab(check: bool, report: &mut Report) -> Result<(), String> {
    let path = paths::workspace().join("ns/openrig-view.ttl");
    let generated = orx_lv2::vocab::turtle();
    if check {
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current == generated {
            report.pass("vocab", "ns/openrig-view.ttl matches orx_plugin::view");
        } else {
            report.fail("vocab", "ns/openrig-view.ttl is stale: run `cargo xtask vocab`");
        }
    } else {
        std::fs::write(&path, generated).map_err(|e| format!("{}: {e}", path.display()))?;
        report.pass("vocab", &format!("wrote {}", path.display()));
    }
    Ok(())
}
