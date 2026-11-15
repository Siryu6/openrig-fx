//! `cargo xtask openrig-host OPENRIG_DIR`: the bundles of this machine
//! inside OpenRig's own LV2 host.
//!
//! The check (`xtask/openrig-host/main.rs`) links the `pedalboard` crate
//! of an OpenRig checkout, so it cannot be a member of this workspace: it
//! gets a manifest of its own, written under the target directory, which
//! points at the checkout (read only) and at the suite's crates. It starts
//! from the checkout's Cargo.lock, so OpenRig's dependencies are the
//! versions it is tested with, and builds into its own target directory.

use crate::paths;
use crate::report::Report;
use std::path::Path;
use std::process::Command;

pub fn run(openrig: &Path, report: &mut Report) -> Result<(), String> {
    let openrig = openrig.canonicalize().map_err(|e| format!("{}: {e}", openrig.display()))?;
    if !openrig.join("src/plugins/lv2_host.rs").is_file() {
        return Err(format!("{}: not an OpenRig checkout", openrig.display()));
    }
    let bundles = paths::target_dir().join("bundles").join(paths::host_triple()?);
    if !bundles.is_dir() {
        return Err(format!("{}: no bundles, run `cargo xtask bundle` first", bundles.display()));
    }

    let dir = paths::target_dir().join("openrig-host");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let workspace = paths::workspace();
    let manifest = format!(
        r#"# Written by `cargo xtask openrig-host`
[package]
name = "orx-openrig-host-check"
version = "0.0.0"
edition = "2024"
publish = false

[[bin]]
name = "openrig-host-check"
path = "{check}"

[dependencies]
pedalboard = {{ path = "{openrig}", default-features = false, features = ["lv2"] }}
orx-plugin = {{ path = "{workspace}/crates/orx-plugin" }}
orx-suite = {{ path = "{workspace}/crates/orx-suite" }}

[workspace]
"#,
        check = workspace.join("xtask/openrig-host/main.rs").display(),
        openrig = openrig.display(),
        workspace = workspace.display(),
    );
    write(&dir.join("Cargo.toml"), &manifest)?;
    let lock = dir.join("Cargo.lock");
    if !lock.exists() {
        std::fs::copy(openrig.join("Cargo.lock"), &lock).map_err(|e| format!("Cargo.lock: {e}"))?;
    }

    println!("building the check against {}", openrig.display());
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--quiet", "--manifest-path"])
        .arg(dir.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if !status.success() {
        return Err("the OpenRig host check does not build".into());
    }

    let output = Command::new(dir.join("target/debug/openrig-host-check"))
        .arg(paths::lv2_path(&bundles))
        .output()
        .map_err(|e| format!("openrig-host-check: {e}"))?;
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        match line.splitn(3, '\t').collect::<Vec<_>>()[..] {
            ["ok", check, detail] => report.pass(check, detail),
            ["FAIL", check, detail] => report.fail(check, detail),
            ["note", check, detail] => report.note(check, detail),
            _ => println!("{line}"),
        }
    }
    if !output.status.success() && !report.failed() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        report.fail("openrig", &format!("the check exited with {}: {}", output.status, stderr.trim()));
    }
    Ok(())
}

/// Write `text` unless the file already holds it (keeps Cargo's
/// fingerprint of the manifest)
fn write(path: &Path, text: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).is_ok_and(|t| t == text) {
        return Ok(());
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
