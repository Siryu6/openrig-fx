//! `cargo xtask bundle`: test, build, assemble, validate

use crate::paths;
use crate::report::Report;
use orx_suite::{BUNDLES, BundleDef};
use std::path::{Path, PathBuf};
use std::process::Command;

fn cargo(args: &[&str]) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(&cargo)
        .args(args)
        .current_dir(paths::workspace())
        .status()
        .map_err(|e| format!("{cargo}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("cargo {} failed", args.join(" "))) }
}

pub fn run(target: Option<&str>, report: &mut Report) -> Result<(), String> {
    let host = paths::host_triple()?;
    let triple = target.unwrap_or(&host).to_string();

    // The plugins' tests hold the conformance and allocation checks that
    // `lv2:hardRTCapable` stands for: no bundle without them. They run on
    // this machine; the code is the same for every target.
    let mut test_args = vec!["test", "--quiet"];
    for b in BUNDLES {
        for t in b.tests {
            test_args.extend(["-p", t]);
        }
    }
    cargo(&test_args).map_err(|e| format!("{e}: the bundles need their tests to pass"))?;
    report.pass("tests", &BUNDLES.iter().flat_map(|b| b.tests.iter().copied()).collect::<Vec<_>>().join(", "));

    let mut build_args = vec!["build", "--release"];
    for b in BUNDLES {
        build_args.extend(["-p", b.package]);
    }
    if let Some(t) = target {
        build_args.extend(["--target", t]);
    }
    cargo(&build_args)?;

    let release = match target {
        Some(t) => paths::target_dir().join(t).join("release"),
        None => paths::target_dir().join("release"),
    };
    let out = paths::target_dir().join("bundles").join(&triple);
    for b in BUNDLES {
        let dir = assemble(b, &release, &out, &triple)?;
        report.pass("bundle", &dir.display().to_string());
        crate::validate::bundle(&dir, report)?;
    }
    Ok(())
}

/// The Turtle files of a bundle: (file name, content)
pub fn turtle(b: &BundleDef, binary: &str) -> Vec<(String, String)> {
    let mut files =
        vec![("manifest.ttl".to_string(), orx_lv2::ttl::manifest(b.plugins, &orx_lv2::ttl::Bundle { binary }))];
    for e in b.plugins {
        files.push((orx_lv2::ttl::ttl_name(e.info), orx_lv2::ttl::plugin(e)));
        if !e.info.presets.is_empty() {
            files.push((orx_lv2::ttl::presets_name(e.info), orx_lv2::ttl::presets(e.info)));
        }
    }
    files
}

fn assemble(b: &BundleDef, release: &Path, out: &Path, triple: &str) -> Result<PathBuf, String> {
    let (built, shipped) = paths::library_names(b.lib, triple);
    let dir = out.join(format!("{}.lv2", b.name));
    // Start from an empty bundle so no file of an older layout lingers
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let from = release.join(&built);
    std::fs::copy(&from, dir.join(&shipped)).map_err(|e| format!("{}: {e}", from.display()))?;
    for (name, content) in turtle(b, &shipped) {
        std::fs::write(dir.join(&name), content).map_err(|e| format!("{name}: {e}"))?;
    }
    let license = paths::workspace().join("LICENSE");
    std::fs::copy(&license, dir.join("LICENSE")).map_err(|e| format!("{}: {e}", license.display()))?;
    Ok(dir)
}
