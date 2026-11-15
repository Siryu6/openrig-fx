//! SIMD strategy of OpenRig FX.
//!
//! Most DSP code is written as plain loops over fixed-size chunks laid out
//! as structure-of-arrays (channels, bands or delay lines side by side), so
//! LLVM vectorises it. That is enough on aarch64, where NEON is part of the
//! baseline, and on the OpenRig OS image for x86_64, built for x86-64-v3.
//!
//! Generic x86_64 builds (desktop packages) only assume SSE2. For the few
//! kernels where it matters, [`kernel!`] compiles the same body twice, once
//! with AVX2 and FMA enabled, and picks one at run time. When the crate is
//! built with AVX2 already enabled the check folds away.
//!
//! The ISA is detected without allocating or locking, so a kernel may be
//! called on the audio thread. [`init`] reads the `ORX_SIMD` override
//! (`scalar` forces the portable path) and must be called off the audio
//! thread, for instance when a plugin is instantiated.
//!
//! Rules for kernel bodies:
//! - no `mul_add` unless the scalar path is meant to differ: without it the
//!   AVX2 build gives the same per-lane results as the portable one (Rust
//!   never contracts `a * b + c` on its own);
//! - reductions across lanes (sums of bands) may round differently between
//!   paths; tests compare them with a tolerance.

#![no_std]

use core::sync::atomic::{AtomicU8, Ordering};

/// Instruction set a kernel runs with
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Isa {
    /// Portable code, vectorised for the baseline of the target (SSE2 on
    /// x86_64, NEON on aarch64)
    Baseline,
    /// x86_64 with AVX2 and FMA (x86-64-v3)
    Avx2Fma,
}

impl Isa {
    /// Short name, for logs and benches
    pub const fn name(self) -> &'static str {
        match self {
            Isa::Baseline => {
                if cfg!(target_arch = "aarch64") {
                    "neon"
                } else {
                    "baseline"
                }
            }
            Isa::Avx2Fma => "avx2+fma",
        }
    }
}

const UNKNOWN: u8 = 0;
const BASELINE: u8 = 1;
const AVX2: u8 = 2;

static SELECTED: AtomicU8 = AtomicU8::new(UNKNOWN);

/// Best ISA of this CPU, ignoring the override
pub fn detected() -> Isa {
    if avx2_fma_available() { Isa::Avx2Fma } else { Isa::Baseline }
}

#[inline]
fn avx2_fma_available() -> bool {
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2", target_feature = "fma"))]
    {
        true
    }
    #[cfg(all(target_arch = "x86_64", not(all(target_feature = "avx2", target_feature = "fma"))))]
    {
        extern crate std;
        std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// Choose the ISA once: the best one of the CPU, or the portable path when
/// the `ORX_SIMD` environment variable is `scalar` or `baseline`. Not
/// real-time safe (reads the environment); call it at instantiation.
pub fn init() {
    extern crate std;
    let forced = std::env::var_os("ORX_SIMD")
        .is_some_and(|v| v.eq_ignore_ascii_case("scalar") || v.eq_ignore_ascii_case("baseline"));
    let isa = if forced { Isa::Baseline } else { detected() };
    SELECTED.store(encode(isa), Ordering::Relaxed);
}

const fn encode(isa: Isa) -> u8 {
    match isa {
        Isa::Baseline => BASELINE,
        Isa::Avx2Fma => AVX2,
    }
}

/// ISA the kernels use: the one chosen by [`init`], else the best one of
/// the CPU. Real-time safe.
#[inline]
pub fn isa() -> Isa {
    match SELECTED.load(Ordering::Relaxed) {
        BASELINE => Isa::Baseline,
        AVX2 => Isa::Avx2Fma,
        _ => {
            let isa = detected();
            SELECTED.store(encode(isa), Ordering::Relaxed);
            isa
        }
    }
}

/// True when `isa` can run on this CPU
pub fn supported(isa: Isa) -> bool {
    match isa {
        Isa::Baseline => true,
        Isa::Avx2Fma => avx2_fma_available(),
    }
}

/// Define a kernel compiled for every ISA, with a dispatcher.
///
/// ```
/// orx_simd::kernel! {
///     /// Add `src` times `gain` to `dst`
///     pub fn mix_into, mix_into_with(dst: &mut [f32], src: &[f32], gain: f32) {
///         for (d, s) in dst.iter_mut().zip(src) {
///             *d += *s * gain;
///         }
///     }
/// }
/// let mut dst = [1.0; 4];
/// mix_into(&mut dst, &[1.0; 4], 0.5);
/// assert_eq!(dst, [1.5; 4]);
/// mix_into_with(orx_simd::Isa::Baseline, &mut dst, &[1.0; 4], 0.5);
/// assert_eq!(dst, [2.0; 4]);
/// ```
///
/// `mix_into` runs with [`isa()`]; `mix_into_with` with a given ISA (for
/// tests comparing paths; an ISA the CPU lacks falls back to the baseline).
#[macro_export]
macro_rules! kernel {
    (
        $(#[$meta:meta])*
        $vis:vis fn $name:ident, $name_with:ident ( $($arg:ident : $ty:ty),* $(,)? ) $(-> $ret:ty)? $body:block
    ) => {
        $(#[$meta])*
        #[inline]
        $vis fn $name($($arg: $ty),*) $(-> $ret)? {
            $name_with($crate::isa(), $($arg),*)
        }

        $(#[$meta])*
        ///
        /// Runs with the given instruction set (the baseline when the CPU
        /// lacks it).
        $vis fn $name_with(isa: $crate::Isa, $($arg: $ty),*) $(-> $ret)? {
            #[inline(always)]
            fn body($($arg: $ty),*) $(-> $ret)? $body

            #[cfg(target_arch = "x86_64")]
            {
                #[target_feature(enable = "avx2,fma")]
                fn avx2($($arg: $ty),*) $(-> $ret)? {
                    body($($arg),*)
                }
                if isa == $crate::Isa::Avx2Fma && $crate::supported($crate::Isa::Avx2Fma) {
                    // SAFETY: the CPU has AVX2 and FMA (checked just above)
                    return unsafe { avx2($($arg),*) };
                }
            }
            let _ = isa;
            body($($arg),*)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    kernel! {
        /// Dot product of two slices
        fn dot, dot_with(a: &[f32], b: &[f32]) -> f32 {
            let mut acc = [0.0f32; 8];
            let ((ca, ra), (cb, rb)) = (a.as_chunks::<8>(), b.as_chunks::<8>());
            for (x, y) in ca.iter().zip(cb) {
                for i in 0..8 {
                    acc[i] += x[i] * y[i];
                }
            }
            let mut sum: f32 = acc.iter().sum();
            for (x, y) in ra.iter().zip(rb) {
                sum += x * y;
            }
            sum
        }
    }

    #[test]
    fn paths_agree() {
        let a: [f32; 37] = core::array::from_fn(|i| i as f32 * 0.25);
        let b: [f32; 37] = core::array::from_fn(|i| 1.0 - i as f32 * 0.01);
        let reference: f64 = a.iter().zip(&b).map(|(x, y)| (*x as f64) * (*y as f64)).sum();
        for isa in [Isa::Baseline, Isa::Avx2Fma] {
            let got = dot_with(isa, &a, &b);
            assert!((got as f64 - reference).abs() < 1e-3, "{isa:?}: {got}");
        }
        assert!((dot(&a, &b) as f64 - reference).abs() < 1e-3);
    }

    #[test]
    fn selection_is_stable() {
        let first = isa();
        assert_eq!(first, isa());
        assert!(supported(first));
    }
}
