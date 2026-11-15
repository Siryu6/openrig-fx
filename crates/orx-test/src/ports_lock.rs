//! Port lock: symbols and indices are frozen once released.
//!
//! `ports.lock` lists `index symbol kind` for every port. The check fails
//! when a locked port moved, was renamed, changed kind or disappeared;
//! new ports may only be appended. `ORX_BLESS=1` rewrites the file (after
//! the check passed, so only appends are ever blessed).

use orx_plugin::{PluginInfo, PortKind};
use std::path::Path;

fn kind(k: &PortKind) -> &'static str {
    match k {
        PortKind::AudioIn => "audio-in",
        PortKind::AudioOut => "audio-out",
        PortKind::Param(_) => "control-in",
        PortKind::Enabled => "enabled",
        PortKind::Meter(_) => "control-out",
        PortKind::Latency => "latency",
        PortKind::Events { .. } => "atom-in",
        PortKind::Stream(_) => "atom-out",
    }
}

/// The lock file content for `info`
pub fn render(info: &PluginInfo) -> String {
    let mut s = format!("# Port lock of {}: append only, never edit a line\n", info.uri);
    for (i, p) in info.ports.iter().enumerate() {
        s += &format!("{i} {} {}\n", p.symbol, kind(&p.kind));
    }
    s
}

/// Check `info` against the lock at `path` (see the module documentation)
pub fn check(info: &PluginInfo, path: impl AsRef<Path>) {
    let path = path.as_ref();
    let current = render(info);
    match std::fs::read_to_string(path) {
        Ok(locked) => {
            let now: Vec<&str> = current.lines().skip(1).collect();
            for (n, line) in locked.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).enumerate() {
                match now.get(n) {
                    Some(l) if *l == line.trim() => {}
                    Some(l) => panic!("{}: locked port `{line}` is now `{l}`", path.display()),
                    None => panic!("{}: locked port `{line}` was removed", path.display()),
                }
            }
            if locked != current {
                if crate::bless() {
                    std::fs::write(path, current).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                } else {
                    panic!("{}: new ports to lock: run the tests with ORX_BLESS=1", path.display());
                }
            }
        }
        Err(_) if crate::bless() => {
            std::fs::write(path, current).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
        Err(e) => panic!("{}: {e} (run the tests with ORX_BLESS=1 to create it)", path.display()),
    }
}
