//! Allocation trap for the audio path.
//!
//! Install [`RtCheck`] as the global allocator of a test binary with
//! [`install_rt_check!`], then run audio code inside [`no_alloc`]: any
//! allocation, reallocation or free on that thread aborts the test with a
//! message (a panic would itself allocate). Other threads are not
//! affected.
//!
//! ```ignore
//! orx_test::install_rt_check!();
//!
//! #[test]
//! fn process_is_rt_safe() {
//!     orx_test::rt::no_alloc(|| runner.process(...));
//! }
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    static FORBIDDEN: Cell<bool> = const { Cell::new(false) };
}

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// The checking allocator (forwards to the system allocator)
pub struct RtCheck;

impl RtCheck {
    #[inline]
    fn check(&self, what: &str) {
        // `try_with`: the thread-local may be gone during thread teardown
        // A panic on the audio path is reported as such (its message
        // allocates); the test fails anyway
        if FORBIDDEN.try_with(Cell::get).unwrap_or(false) && !std::thread::panicking() {
            let _ = FORBIDDEN.try_with(|f| f.set(false));
            let msg = b"orx-test: memory ";
            write_stderr(msg);
            write_stderr(what.as_bytes());
            write_stderr(b" on the audio path\n");
            // The trap is off now: printing the backtrace may allocate
            if std::env::var_os("ORX_RT_BACKTRACE").is_some() {
                write_stderr(std::backtrace::Backtrace::force_capture().to_string().as_bytes());
            } else {
                write_stderr(b"(set ORX_RT_BACKTRACE=1 for a backtrace)\n");
            }
            std::process::abort();
        }
    }
}

fn write_stderr(bytes: &[u8]) {
    use std::io::Write;
    let _ = std::io::stderr().lock().write_all(bytes);
}

// SAFETY: forwards every call to `System` unchanged; the check only reads
// a const-initialised thread-local, which does not allocate.
unsafe impl GlobalAlloc for RtCheck {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        INSTALLED.store(true, Ordering::Relaxed);
        self.check("allocation");
        // SAFETY: same contract as the caller's
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        self.check("allocation");
        // SAFETY: same contract as the caller's
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.check("free");
        // SAFETY: same contract as the caller's
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        self.check("reallocation");
        // SAFETY: same contract as the caller's
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Make [`RtCheck`] the global allocator of this binary
#[macro_export]
macro_rules! install_rt_check {
    () => {
        #[global_allocator]
        static ORX_RT_CHECK: $crate::rt::RtCheck = $crate::rt::RtCheck;
    };
}

/// Is [`RtCheck`] the global allocator? (It has seen an allocation.)
pub fn installed() -> bool {
    let probe = Box::new(0u8);
    drop(std::hint::black_box(probe));
    INSTALLED.load(Ordering::Relaxed)
}

/// Run `f` with allocation forbidden on this thread
pub fn no_alloc<R>(f: impl FnOnce() -> R) -> R {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            FORBIDDEN.with(|c| c.set(self.0));
        }
    }
    let _restore = Restore(FORBIDDEN.with(|c| c.replace(true)));
    f()
}
