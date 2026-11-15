//! LV2 wrapper of OpenRig FX.
//!
//! A thin, dependency-free binding written against the LV2 C headers: the
//! descriptor and its entry points ([`instance`]), URID mapping, event
//! input (MIDI, `time:Position`) and view frame output ([`atom`]), the
//! state interface ([`state`]), and the Turtle generator ([`ttl`]) that
//! writes each bundle's metadata from the plugin descriptions, so symbols
//! and indices never drift between code and TTL.
//!
//! A plugin's cdylib crate is one line:
//!
//! ```ignore
//! orx_lv2::export_lv2!(orx_tremolo::Mono, orx_tremolo::Stereo);
//! ```
//!
//! [`host`] is a minimal in-process LV2 host used by tests to drive the
//! real C entry points.

pub mod atom;
pub mod host;
pub mod instance;
mod state;
pub mod sys;
pub mod ttl;
pub mod uris;
pub mod vocab;

pub use instance::{Descriptor, descriptor, needs_map};

/// The descriptors of a bundle, built once (not real-time)
pub struct Table(Box<[Descriptor]>);

impl Table {
    pub fn new(descriptors: Vec<Descriptor>) -> Self {
        Self(descriptors.into_boxed_slice())
    }

    pub fn get(&self, index: u32) -> *const sys::LV2_Descriptor {
        self.0.get(index as usize).map_or(std::ptr::null(), |d| &d.raw)
    }
}

/// Export the `lv2_descriptor` symbol of a bundle holding the given
/// plugin types, in this order (the order is the descriptor index)
#[macro_export]
macro_rules! export_lv2 {
    ($($plugin:ty),+ $(,)?) => {
        /// LV2 entry point: the descriptor at `index`, null past the end
        ///
        /// # Safety
        ///
        /// Called by LV2 hosts per the LV2 core specification.
        #[unsafe(no_mangle)]
        pub extern "C" fn lv2_descriptor(index: u32) -> *const $crate::sys::LV2_Descriptor {
            static TABLE: ::std::sync::OnceLock<$crate::Table> = ::std::sync::OnceLock::new();
            ::std::panic::catch_unwind(|| {
                TABLE.get_or_init(|| $crate::Table::new(vec![$($crate::descriptor::<$plugin>()),+])).get(index)
            })
            .unwrap_or(::std::ptr::null())
        }
    };
}
