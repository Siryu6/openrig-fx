//! The shared library against the OpenRig image's ABI: its architecture,
//! the libraries it needs, the newest glibc symbol version it uses

use crate::paths;
use crate::report::Report;
use std::path::Path;
use std::process::Command;

/// glibc of the OpenRig image (Bootlin stable toolchain of Buildroot
/// 2026.02); a bundle must not need anything newer
pub const GLIBC_MAX: (u32, u32) = (2, 41);

/// The libraries a plugin may link: the C runtime only (no libstdc++, no
/// JACK, no GUI)
const ALLOWED: &[&str] = &["libc.so.6", "libm.so.6", "libgcc_s.so.1", "libdl.so.2", "libpthread.so.0", "ld-linux"];

/// Architecture of an ELF file as `std::env::consts::ARCH` names it
pub fn arch(lib: &Path) -> Result<&'static str, String> {
    let bytes = std::fs::read(lib).map_err(|e| format!("{}: {e}", lib.display()))?;
    if bytes.len() < 20 || &bytes[..4] != b"\x7fELF" {
        return Err(format!("{}: not an ELF file", lib.display()));
    }
    let machine = if bytes[5] == 1 {
        u16::from_le_bytes([bytes[18], bytes[19]])
    } else {
        u16::from_be_bytes([bytes[18], bytes[19]])
    };
    Ok(match machine {
        0x3e => "x86_64",
        0xb7 => "aarch64",
        0x28 => "arm",
        0xf3 => "riscv64",
        _ => "unknown",
    })
}

pub fn check(lib: &Path, report: &mut Report) {
    if !paths::has_tool("readelf") {
        report.skip("abi", "readelf not installed");
        return;
    }
    let out = match Command::new("readelf").args(["--dynamic", "--version-info", "--wide"]).arg(lib).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => {
            report.fail("abi", &format!("readelf failed on {}", lib.display()));
            return;
        }
    };
    let needed: Vec<&str> =
        out.lines().filter(|l| l.contains("(NEEDED)")).filter_map(|l| l.split('[').nth(1)?.split(']').next()).collect();
    let foreign: Vec<&&str> = needed.iter().filter(|n| !ALLOWED.iter().any(|a| n.starts_with(a))).collect();
    if foreign.is_empty() {
        report.pass("abi", &format!("needs {}", needed.join(", ")));
    } else {
        report.fail("abi", &format!("links libraries the image may lack: {foreign:?}"));
    }
    let newest = out
        .split(|c: char| c.is_whitespace() || c == ':')
        .filter_map(|w| w.strip_prefix("GLIBC_"))
        .filter_map(|v| {
            let mut it = v.split('.').map(|n| n.parse::<u32>().ok());
            Some((it.next()??, it.next().flatten().unwrap_or(0)))
        })
        .max();
    match newest {
        Some(v) if v <= GLIBC_MAX => {
            report.pass(
                "glibc",
                &format!("newest symbol GLIBC_{}.{} (image: {}.{})", v.0, v.1, GLIBC_MAX.0, GLIBC_MAX.1),
            );
        }
        Some(v) => {
            report.fail("glibc", &format!("needs GLIBC_{}.{}, the image has {}.{}", v.0, v.1, GLIBC_MAX.0, GLIBC_MAX.1))
        }
        None => report.pass("glibc", "no versioned glibc symbol"),
    }
}
