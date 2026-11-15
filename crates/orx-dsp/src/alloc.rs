//! The only place where DSP code allocates. Constructors call these; the
//! audio path never does (checked by the RT tests of `orx-test`).

/// A zeroed buffer of `len` samples
#[allow(clippy::disallowed_macros)] // construction time only
pub fn zeroed(len: usize) -> Box<[f32]> {
    vec![0.0; len].into_boxed_slice()
}

/// A buffer of `len` copies of `T::default()`
#[allow(clippy::disallowed_macros)] // construction time only
pub fn filled<T: Clone + Default>(len: usize) -> Box<[T]> {
    vec![T::default(); len].into_boxed_slice()
}

/// A buffer built from a function of the index
#[allow(clippy::disallowed_methods)] // construction time only
pub fn from_fn<T>(len: usize, f: impl FnMut(usize) -> T) -> Box<[T]> {
    (0..len).map(f).collect()
}
