//! Denormal control: flush subnormal floats to zero for the duration of a
//! processing call.
//!
//! Recursive filters and reverbs decay into subnormal numbers, which are
//! up to 100 times slower on many CPUs. [`FtzGuard`] sets flush-to-zero
//! (and denormals-are-zero on x86) in the floating-point control register
//! of the current thread, and restores the previous mode when dropped, so
//! the host thread is left as it was found.
//!
//! - x86_64: MXCSR bits FTZ (15) and DAZ (6)
//! - aarch64: FPCR bit FZ (24)
//! - other targets: no effect

/// Flush-to-zero mode, restored on drop
#[must_use = "the mode is restored when the guard is dropped"]
pub struct FtzGuard {
    #[cfg_attr(not(any(target_arch = "x86_64", target_arch = "aarch64")), allow(dead_code))]
    saved: u64,
}

impl FtzGuard {
    /// Enable flush-to-zero on this thread until the guard is dropped
    #[inline]
    pub fn new() -> Self {
        let saved = read();
        write(saved | FLAGS);
        Self { saved }
    }
}

impl Default for FtzGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FtzGuard {
    #[inline]
    fn drop(&mut self) {
        write(self.saved);
    }
}

#[cfg(target_arch = "x86_64")]
const FLAGS: u64 = 0x8040;
#[cfg(target_arch = "aarch64")]
const FLAGS: u64 = 1 << 24;
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
const FLAGS: u64 = 0;

#[cfg(target_arch = "x86_64")]
#[inline]
fn read() -> u64 {
    let mut csr: u32 = 0;
    // SAFETY: stmxcsr stores the 32-bit MXCSR into the given location
    unsafe {
        core::arch::asm!("stmxcsr [{}]", in(reg) &mut csr, options(nostack, preserves_flags));
    }
    u64::from(csr)
}

#[cfg(target_arch = "x86_64")]
#[inline]
fn write(value: u64) {
    let csr = value as u32;
    // SAFETY: ldmxcsr loads a valid MXCSR value (one read earlier, with
    // only the FTZ/DAZ bits added, both supported on every x86_64 CPU)
    unsafe {
        core::arch::asm!("ldmxcsr [{}]", in(reg) &csr, options(nostack, preserves_flags, readonly));
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn read() -> u64 {
    let fpcr: u64;
    // SAFETY: reading FPCR has no side effect
    unsafe {
        core::arch::asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags));
    }
    fpcr
}

#[cfg(target_arch = "aarch64")]
#[inline]
fn write(value: u64) {
    // SAFETY: writes back FPCR as read, with only the FZ bit changed
    unsafe {
        core::arch::asm!("msr fpcr, {}", in(reg) value, options(nomem, nostack, preserves_flags));
    }
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[inline]
fn read() -> u64 {
    0
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[inline]
fn write(_value: u64) {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::hint::black_box;

    #[test]
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    fn flushes_and_restores() {
        let before = read();
        let tiny = || black_box(1e-38f32) * black_box(1e-3f32);
        assert!(tiny() != 0.0, "subnormal expected without the guard");
        {
            let _guard = FtzGuard::new();
            assert_eq!(tiny(), 0.0);
        }
        // Exception flags (bits 0-5 on x86) are sticky: compare the modes
        assert_eq!(read() & !0x3f, before & !0x3f);
        assert!(tiny() != 0.0);
    }

    #[test]
    fn nests() {
        let before = read();
        {
            let _a = FtzGuard::new();
            let _b = FtzGuard::new();
        }
        assert_eq!(read() & !0x3f, before & !0x3f);
    }
}
