//! JACK side of OpenRig FX.
//!
//! - [`Standalone`] runs one plugin of the suite as a JACK client, without
//!   any LV2 host: audio ports named after the plugin's, a MIDI input
//!   (CC to parameters, events to plugins that take MIDI), the JACK
//!   transport tempo (or a fixed one) as `time:Position`. `orx-run` is
//!   its command line.
//! - [`probe`] plays a signal into any JACK client and records its
//!   output, and [`dummy`] starts a private server with the dummy
//!   backend: together, end-to-end checks of `orx-run` and of the LV2
//!   bundles under jalv, without a sound card.
//!
//! libjack is loaded at run time, so building needs no JACK development
//! files.

pub mod dummy;
mod host;
pub mod probe;
pub mod queue;

pub use host::{Options, Standalone};
