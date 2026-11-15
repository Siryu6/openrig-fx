//! Type-erased plugins, for hosts that pick one at run time (the
//! standalone, test suites, the bundle generator).

use crate::desc::PluginInfo;
use crate::process::{Event, StreamSink};
use crate::runner::Runner;
use crate::{Config, Plugin};

/// A [`Runner`] behind a trait object
pub trait DynRunner: Send {
    fn info(&self) -> &'static PluginInfo;
    fn activate(&mut self);
    fn set_param(&mut self, index: usize, value: f32);
    fn param(&self, index: usize) -> f32;
    fn meter(&self, index: usize) -> f32;
    fn push_event(&mut self, event: Event) -> bool;
    fn latency(&self) -> u32;
    fn faults(&self) -> u32;
    /// [`Runner::is_idle`]
    fn is_idle(&self) -> bool;
    fn process(&mut self, frames: usize, inputs: &[&[f32]], outputs: &mut [&mut [f32]], sink: &mut dyn StreamSink);
}

impl<P: Plugin> DynRunner for Runner<P> {
    fn info(&self) -> &'static PluginInfo {
        P::INFO
    }
    fn activate(&mut self) {
        Runner::activate(self);
    }
    fn set_param(&mut self, index: usize, value: f32) {
        Runner::set_param(self, index, value);
    }
    fn param(&self, index: usize) -> f32 {
        Runner::param(self, index)
    }
    fn meter(&self, index: usize) -> f32 {
        Runner::meter(self, index)
    }
    fn push_event(&mut self, event: Event) -> bool {
        Runner::push_event(self, event)
    }
    fn latency(&self) -> u32 {
        Runner::latency(self)
    }
    fn faults(&self) -> u32 {
        Runner::faults(self)
    }
    fn is_idle(&self) -> bool {
        Runner::is_idle(self)
    }
    fn process(&mut self, frames: usize, inputs: &[&[f32]], outputs: &mut [&mut [f32]], sink: &mut dyn StreamSink) {
        Runner::process(self, frames, inputs, outputs, sink);
    }
}

/// A plugin of a registry: its description and a constructor
#[derive(Clone, Copy)]
pub struct Entry {
    pub info: &'static PluginInfo,
    /// [`Plugin::HAS_STATE`]
    pub has_state: bool,
    pub create: fn(Config) -> Box<dyn DynRunner>,
}

impl Entry {
    /// The entry of plugin `P`
    pub const fn of<P: Plugin>() -> Self {
        Self { info: P::INFO, has_state: P::HAS_STATE, create: create::<P> }
    }

    /// The short name used on command lines: the last URI segment
    pub fn slug(&self) -> &'static str {
        self.info.uri.rsplit('/').next().unwrap_or(self.info.uri)
    }
}

impl core::fmt::Debug for Entry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Entry").field("uri", &self.info.uri).field("has_state", &self.has_state).finish_non_exhaustive()
    }
}

fn create<P: Plugin>(config: Config) -> Box<dyn DynRunner> {
    Box::new(Runner::<P>::new(config))
}
