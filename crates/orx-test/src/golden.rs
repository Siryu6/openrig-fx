//! Golden files: processed audio compared to a reviewed reference.
//!
//! References live next to the plugin's tests (`tests/golden/*.wav`).
//! With `ORX_BLESS=1` the reference is (re)written; otherwise a mismatch
//! fails and writes the result as `<name>.wav.actual` to listen to.

use crate::{bless, metrics, wav};
use std::path::Path;

/// Compare `channels` to the reference at `path`: largest sample
/// difference at most `tolerance`
pub fn check(path: impl AsRef<Path>, channels: &[&[f32]], sample_rate: u32, tolerance: f32) {
    let path = path.as_ref();
    if bless() {
        wav::write(path, channels, sample_rate).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
        return;
    }
    let reference = wav::read(path)
        .unwrap_or_else(|e| panic!("{}: {e} (run with ORX_BLESS=1 to create it, then listen to it)", path.display()));
    let actual = path.with_extension("wav.actual");
    let fail = |msg: String| -> ! {
        let _ = wav::write(&actual, channels, sample_rate);
        panic!("{}: {msg}; result written to {}", path.display(), actual.display());
    };
    if reference.sample_rate != sample_rate {
        fail(format!("sample rate {} instead of {}", sample_rate, reference.sample_rate));
    }
    if reference.channels.len() != channels.len() || reference.frames() != channels.first().map_or(0, |c| c.len()) {
        fail(format!(
            "{} channels x {} frames instead of {} x {}",
            channels.len(),
            channels.first().map_or(0, |c| c.len()),
            reference.channels.len(),
            reference.frames()
        ));
    }
    for (k, (r, a)) in reference.channels.iter().zip(channels).enumerate() {
        let d = metrics::max_diff(r, a);
        if d > tolerance {
            fail(format!("channel {k} differs by {d:e} (tolerance {tolerance:e})"));
        }
    }
    let _ = std::fs::remove_file(actual);
}
