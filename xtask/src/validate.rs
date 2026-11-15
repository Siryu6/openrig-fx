//! `cargo xtask validate`: every check that applies to an assembled bundle

use crate::report::Report;
use crate::{bundle, elf, jack, load, paths};
use orx_suite::{BUNDLES, BundleDef};
use std::path::{Path, PathBuf};
use std::process::Command;

fn def_of(dir: &Path) -> Result<&'static BundleDef, String> {
    let stem = dir.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    BUNDLES.iter().find(|b| b.name == stem).ok_or_else(|| format!("{}: not a bundle of the suite", dir.display()))
}

pub fn bundle(dir: &Path, report: &mut Report) -> Result<(), String> {
    let def = def_of(dir)?;
    println!("{}", dir.display());
    let lib = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .find(|p| p.file_stem().and_then(|s| s.to_str()) == Some(def.lib) && p.extension().is_some_and(|x| x != "ttl"))
        .ok_or_else(|| format!("{}: no {} library", dir.display(), def.lib))?;
    let binary = lib.file_name().and_then(|n| n.to_str()).unwrap_or_default();

    // The Turtle files are those the registry generates today
    let files = bundle::turtle(def, binary);
    let mut paths_ttl: Vec<PathBuf> = Vec::new();
    for (name, expected) in &files {
        let path = dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(text) if &text == expected => {
                let undefined = orx_lv2::vocab::undefined_terms(&text);
                if !undefined.is_empty() {
                    report.fail("vocab", &format!("{name}: openrig terms not in the vocabulary: {undefined:?}"));
                }
            }
            Ok(_) => report.fail("turtle", &format!("{name} differs from the generator: rebuild the bundle")),
            Err(e) => report.fail("turtle", &format!("{name}: {e}")),
        }
        paths_ttl.push(path);
    }
    report.pass("turtle", &format!("{} files as generated", files.len()));
    syntax(&paths_ttl, report);
    schemas(&paths_ttl, report);

    elf::check(&lib, report);
    match elf::arch(&lib) {
        Ok(arch) if arch == std::env::consts::ARCH => {
            load::check(def, &lib, report);
            lint(def, dir, report);
            jack::check(def, dir, report);
        }
        Ok(arch) => {
            for check in ["load", "lv2lint", "jalv"] {
                report.skip(check, &format!("{arch} binary on a {} machine", std::env::consts::ARCH));
            }
        }
        Err(e) => report.fail("abi", &e),
    }
    if report.failed() { Err(format!("{} does not validate", dir.display())) } else { Ok(()) }
}

/// Turtle syntax, with serd
fn syntax(files: &[PathBuf], report: &mut Report) {
    if !paths::has_tool("serdi") {
        return report.skip("syntax", "serdi not installed");
    }
    for f in files {
        let ok = Command::new("serdi").args(["-q", "-i", "turtle", "-o", "ntriples"]).arg(f).output();
        match ok {
            Ok(o) if o.status.success() => {}
            Ok(o) => return report.fail("syntax", &format!("{}: {}", f.display(), String::from_utf8_lossy(&o.stderr))),
            Err(e) => return report.fail("syntax", &e.to_string()),
        }
    }
    report.pass("syntax", "serdi parses every file");
}

/// The LV2 schemas and the openrig vocabulary, with sord_validate
fn schemas(files: &[PathBuf], report: &mut Report) {
    if !paths::has_tool("lv2_validate") {
        return report.skip("schema", "lv2_validate not installed");
    }
    let vocab = paths::workspace().join("ns/openrig-view.ttl");
    match Command::new("lv2_validate").arg(&vocab).args(files).output() {
        Ok(o) => {
            let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
            let problems: Vec<&str> =
                text.lines().filter(|l| l.starts_with("error:") || l.starts_with("warning:")).collect();
            if o.status.success() && problems.is_empty() {
                report.pass("schema", "lv2_validate: no error, no warning");
            } else {
                report.fail("schema", &format!("lv2_validate:\n{}", problems.join("\n")));
            }
        }
        Err(e) => report.fail("schema", &e.to_string()),
    }
}

/// lv2lint, warnings counting as errors
fn lint(def: &BundleDef, dir: &Path, report: &mut Report) {
    if !paths::has_tool("lv2lint") {
        return report.skip("lv2lint", "not installed");
    }
    let lv2_path = paths::lv2_path(dir.parent().unwrap_or(dir));
    for e in def.plugins {
        let out = Command::new("lv2lint")
            .args(["-M", "pack", "-S", "warn", "-E", "warn", "-I"])
            .arg(dir)
            .arg(e.info.uri)
            .env("LV2_PATH", &lv2_path)
            .output();
        match out {
            Ok(o) if o.status.success() => report.pass("lv2lint", e.info.uri),
            Ok(o) => report.fail(
                "lv2lint",
                &format!(
                    "{}:\n{}{}",
                    e.info.uri,
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                ),
            ),
            Err(err) => report.fail("lv2lint", &err.to_string()),
        }
    }
}
