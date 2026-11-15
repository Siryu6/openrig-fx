//! Format-neutral plugin model of OpenRig FX.
//!
//! A plugin is a type implementing [`Plugin`]: a static description
//! ([`PluginInfo`]: ports, parameters, presets, view metadata) and a
//! real-time `process`. Wrappers turn it into an LV2 plugin (`orx-lv2`) or
//! a JACK client (`orx-standalone`); both drive it through the same
//! [`runner::Runner`], which owns everything a format should not have to
//! reimplement: block splitting, parameter sanitising, click-free bypass
//! with latency-aligned dry signal, non-finite output recovery, latency
//! reporting and rate-limited data streams.
//!
//! # Port indices are a public contract
//!
//! The position of a port in [`PluginInfo::ports`] is its LV2 index, and
//! its symbol is how presets and rigs refer to it. Both are frozen once a
//! version is released: new ports are only appended. Each plugin keeps a
//! `ports.lock` file checked by its tests (see `orx-test`).
//!
//! # Real-time contract of `process`
//!
//! No allocation, no lock, no system call, no unbounded loop, no panic.
//! `orx-test` runs every plugin under an allocation-trapping allocator and
//! random parameter automation; a plugin declares `hard_rt` only when it
//! passes.

pub mod desc;
pub mod dynamic;
pub mod process;
pub mod runner;
pub mod state;
pub mod tempo;
pub mod validate;
pub mod view;

pub use desc::{Choice, Class, Meter, Param, ParamKind, PluginInfo, Port, PortKind, Preset, Stream, Unit, Version};
pub use process::{Context, Event, EventKind, Position};
pub use state::{StateError, StateSink, StateSource, StateValue};
pub use view::{Face, Family, Finish, KnobStyle, PlateStyle, Role, View, ViewKind};

/// What a plugin instance is created for
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// Sample rate, Hz
    pub sample_rate: f64,
    /// Most frames a single `process` call will get
    pub max_block: usize,
}

/// An audio effect of the suite
pub trait Plugin: Sized + Send + 'static {
    /// The static description: ports, parameters, presets, views
    const INFO: &'static PluginInfo;

    /// Does the plugin save state beyond its parameters (`save_state`)?
    const HAS_STATE: bool = false;

    /// Build an instance: allocate every buffer here, for `config`.
    /// Not called on the audio thread.
    fn new(config: &Config) -> Self;

    /// Clear the audio state (delay lines, filter memories, envelopes) as
    /// at the start of a stream. Must be real-time safe: the runner calls
    /// it on the audio thread after a bypass and after non-finite output.
    fn reset(&mut self);

    /// Process one block. Real-time safe; see the crate documentation.
    fn process(&mut self, ctx: &mut Context<'_>);

    /// Let the sound ring out when the plugin is bypassed ("trails", or
    /// "spillover"), for echoes and reverbs. When `lv2:enabled` goes to 0
    /// the runner fades the plugin's *input* out instead of its output,
    /// adds the dry signal back, `out = plugin(g·in) + (1 − g)·in`, and
    /// keeps running the plugin with silent input while
    /// [`Plugin::is_ringing`] (at most [`runner::TRAIL_LIMIT_S`]). A
    /// plugin whose output is its dry signal at unity plus its effect is
    /// then exactly "dry plus the tail" during the whole bypass. Requires
    /// a plugin without latency.
    const TRAILS: bool = false;

    /// With [`Plugin::TRAILS`]: does the plugin still hold sound that
    /// silent input would let out (echoes in a delay line, a reverb tail)?
    /// Called once per processed piece; real-time safe and cheap.
    fn is_ringing(&self) -> bool {
        false
    }

    /// Latency the plugin adds now, in frames
    fn latency(&self) -> u32 {
        0
    }

    /// Most latency the plugin may ever report for `config` (sizes the
    /// dry delay of the bypass)
    fn max_latency(_config: &Config) -> u32 {
        0
    }

    /// Save the state that parameters do not carry. Not on the audio thread.
    fn save_state(&self, _sink: &mut dyn StateSink) {}

    /// Restore a state saved by `save_state`. Not on the audio thread, never
    /// concurrently with `process`.
    fn load_state(&mut self, _source: &dyn StateSource) -> Result<(), StateError> {
        Ok(())
    }
}
