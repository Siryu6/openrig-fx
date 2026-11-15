//! Load a bundle's binary in this process and drive every plugin through
//! its C entry points with the in-process test host of `orx-lv2`

use crate::report::Report;
use orx_lv2::host::Host;
use orx_lv2::sys::LV2_Descriptor;
use orx_suite::BundleDef;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::Path;

const RTLD_NOW: c_int = 2;

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn dlclose(handle: *mut c_void) -> c_int;
}

/// A loaded shared library, closed on drop
struct Library(*mut c_void);

impl Library {
    fn open(path: &Path) -> Result<Self, String> {
        let c = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        // SAFETY: dlopen with a NUL-terminated path; the library's
        // initialisers are those of a Rust cdylib (none besides std's)
        let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
        if h.is_null() { Err(last_error()) } else { Ok(Self(h)) }
    }

    fn entry(&self) -> Result<unsafe extern "C" fn(u32) -> *const LV2_Descriptor, String> {
        // SAFETY: dlsym on a live handle with a NUL-terminated name
        let f = unsafe { dlsym(self.0, c"lv2_descriptor".as_ptr()) };
        if f.is_null() {
            return Err(format!("no lv2_descriptor symbol: {}", last_error()));
        }
        // SAFETY: the LV2 entry point has this signature (lv2core)
        Ok(unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn(u32) -> *const LV2_Descriptor>(f) })
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: the handle came from dlopen and no instance outlives it
        unsafe { dlclose(self.0) };
    }
}

fn last_error() -> String {
    // SAFETY: dlerror returns null or a NUL-terminated message
    let e = unsafe { dlerror() };
    if e.is_null() {
        "unknown dl error".into()
    } else {
        // SAFETY: non-null, NUL-terminated (above)
        unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
    }
}

pub fn check(def: &BundleDef, lib: &Path, report: &mut Report) {
    match run(def, lib) {
        Ok(what) => report.pass("load", &what),
        Err(e) => report.fail("load", &e),
    }
}

fn run(def: &BundleDef, lib: &Path) -> Result<String, String> {
    let library = Library::open(lib)?;
    let entry = library.entry()?;
    let mut uris = Vec::new();
    loop {
        // SAFETY: the LV2 entry point, any index (null past the end)
        let d = unsafe { entry(uris.len() as u32) };
        if d.is_null() {
            break;
        }
        // SAFETY: a descriptor of the library, its URI NUL-terminated
        uris.push((d, unsafe { CStr::from_ptr((*d).uri) }.to_string_lossy().into_owned()));
    }
    let expected: Vec<&str> = def.plugins.iter().map(|e| e.info.uri).collect();
    let found: Vec<&str> = uris.iter().map(|(_, u)| u.as_str()).collect();
    if found != expected {
        return Err(format!("descriptors {found:?}, the registry says {expected:?}"));
    }
    let mut blocks = 0;
    for ((d, uri), e) in uris.iter().zip(def.plugins) {
        for in_place in [false, true] {
            for sample_rate in [44_100.0, 48_000.0, 96_000.0] {
                blocks += drive(*d, e.info, sample_rate, in_place).map_err(|why| format!("{uri}: {why}"))?;
            }
        }
    }
    drop(library);
    Ok(format!("{} plugins, {blocks} blocks through the C ABI", uris.len()))
}

/// Run noise through every preset; the output stays finite and bounded
fn drive(
    d: *const LV2_Descriptor,
    info: &'static orx_plugin::PluginInfo,
    sample_rate: f64,
    in_place: bool,
) -> Result<usize, String> {
    const BLOCK: usize = 256;
    let host = Host::new(sample_rate, BLOCK);
    // SAFETY: `d` is the library's descriptor of `info` (URIs compared)
    let mut p = unsafe { host.instantiate(d, info, in_place) }.ok_or("instantiate returned null")?;
    p.activate();
    let mut seed = 0x9e37_79b9_u32;
    let mut blocks = 0;
    let presets = info.presets.iter().map(Some).chain([None]);
    for preset in presets {
        if let Some(preset) = preset {
            for (i, v) in info.preset_values(preset) {
                p.set(i, v);
            }
        }
        for _ in 0..40 {
            for k in 0..info.audio_inputs() {
                for x in p.input(k) {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    *x = (seed as f32 / u32::MAX as f32 - 0.5) * 0.5;
                }
            }
            p.push_position(0, 120.0, 1.0);
            p.run(BLOCK);
            blocks += 1;
            for k in 0..info.audio_outputs() {
                if let Some(x) = p.output(k).iter().find(|x| !x.is_finite() || x.abs() > 16.0) {
                    return Err(format!("output {k} reached {x}"));
                }
            }
        }
    }
    Ok(blocks)
}
