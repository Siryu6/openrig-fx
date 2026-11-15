//! Where things are

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Cargo's target directory (honours CARGO_TARGET_DIR)
pub fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| workspace().join("target"), PathBuf::from)
}

/// The triple of this machine (what `rustc -vV` reports as host)
pub fn host_triple() -> Result<String, String> {
    let out = Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .arg("-vV")
        .output()
        .map_err(|e| format!("rustc -vV: {e}"))?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: ").map(str::to_string))
        .ok_or_else(|| "rustc -vV: no host line".into())
}

/// Shared library file name of a cdylib `lib` on `triple`, as Cargo
/// builds it and as it is named in the bundle
pub fn library_names(lib: &str, triple: &str) -> (String, String) {
    if triple.contains("windows") {
        (format!("{lib}.dll"), format!("{lib}.dll"))
    } else if triple.contains("apple") {
        (format!("lib{lib}.dylib"), format!("{lib}.dylib"))
    } else {
        (format!("lib{lib}.so"), format!("{lib}.so"))
    }
}

/// Is `tool` on PATH?
pub fn has_tool(tool: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()))
}

/// LV2_PATH for checking the bundles in `bundles`: that directory first,
/// then the user's path (or lilv's default on Linux) so the LV2
/// specifications are found
pub fn lv2_path(bundles: &Path) -> std::ffi::OsString {
    let mut parts = vec![bundles.to_path_buf()];
    match std::env::var_os("LV2_PATH") {
        Some(p) => parts.extend(std::env::split_paths(&p)),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                parts.push(PathBuf::from(home).join(".lv2"));
            }
            parts.extend(["/usr/local/lib/lv2", "/usr/lib/lv2"].map(PathBuf::from));
        }
    }
    std::env::join_paths(parts).unwrap_or_default()
}
