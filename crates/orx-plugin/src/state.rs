//! Plugin state beyond parameters (rarely needed: parameters are saved by
//! hosts already). Keys are short names unique in the plugin; wrappers
//! turn them into URIs (`<plugin URI>#<key>`).

/// A value in a saved state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StateValue<'a> {
    Bytes(&'a [u8]),
    Str(&'a str),
    F32(f32),
    I64(i64),
}

/// Where `save_state` writes
pub trait StateSink {
    fn put(&mut self, key: &str, value: StateValue<'_>);
}

/// Where `load_state` reads
pub trait StateSource {
    fn get(&self, key: &str) -> Option<StateValue<'_>>;
}

/// Why a state was refused
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateError {
    Missing(&'static str),
    Invalid(&'static str),
}

impl core::fmt::Display for StateError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StateError::Missing(k) => write!(f, "state key {k} missing"),
            StateError::Invalid(k) => write!(f, "state key {k} invalid"),
        }
    }
}

impl std::error::Error for StateError {}
