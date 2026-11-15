//! What a plugin sees in `process`.

use crate::desc::{PluginInfo, PortKind};

/// Transport state from a `time:Position` (fields the host did not send are
/// `None`)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    pub bpm: Option<f32>,
    /// 0 stopped, 1 playing
    pub speed: Option<f32>,
    /// Beat within the bar
    pub bar_beat: Option<f32>,
    pub beats_per_bar: Option<f32>,
    pub beat_unit: Option<i32>,
}

/// An event of the block
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EventKind {
    /// Short MIDI message (1 to 3 bytes)
    Midi { data: [u8; 3], len: u8 },
    /// Transport update
    Position(Position),
}

/// An event at a frame of the block
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// Frame within the block, `< frames`
    pub frame: u32,
    pub kind: EventKind,
}

/// Values of the control inputs for this block, by port index (sanitised:
/// in range, never NaN)
#[derive(Clone, Copy)]
pub struct Params<'a> {
    pub(crate) values: &'a [f32],
}

impl Params<'_> {
    /// Value of the control input at `index` (0 for any other port)
    #[inline]
    pub fn get(&self, index: usize) -> f32 {
        self.values.get(index).copied().unwrap_or(0.0)
    }

    /// Toggle or choice port as a bool (≥ 0.5)
    #[inline]
    pub fn on(&self, index: usize) -> bool {
        self.get(index) >= 0.5
    }

    /// Integer or choice port as an integer
    #[inline]
    pub fn int(&self, index: usize) -> i32 {
        self.get(index).round() as i32
    }
}

/// Control outputs of the block, by port index
pub struct Meters<'a> {
    pub(crate) values: &'a mut [f32],
}

impl Meters<'_> {
    /// Report `value` on the control output at `index`
    #[inline]
    pub fn set(&mut self, index: usize, value: f32) {
        if let Some(v) = self.values.get_mut(index) {
            *v = if value.is_finite() { value } else { 0.0 };
        }
    }
}

/// Destination of stream frames (implemented by each wrapper)
pub trait StreamSink {
    /// The host listens to the stream output at port `index`
    fn connected(&self, index: usize) -> bool;
    /// Send a frame; false when it could not be delivered (no room)
    fn send(&mut self, index: usize, frame: u32, channels: u32, position: f32, values: &[f32]) -> bool;
}

/// A sink that drops everything (no host listening)
pub struct NoStreams;

impl StreamSink for NoStreams {
    fn connected(&self, _index: usize) -> bool {
        false
    }

    fn send(&mut self, _: usize, _: u32, _: u32, _: f32, _: &[f32]) -> bool {
        false
    }
}

/// Rate-limited access to the stream outputs
pub struct Streams<'a> {
    pub(crate) sink: &'a mut dyn StreamSink,
    /// Frames left before each port may send again, by port index
    pub(crate) countdown: &'a mut [u32],
    pub(crate) interval: &'a [u32],
    pub(crate) info: &'static PluginInfo,
}

impl Streams<'_> {
    /// A frame for the stream output at `index` is wanted now: the host
    /// listens and the rate limit allows it. Compute the frame only then.
    #[inline]
    pub fn due(&self, index: usize) -> bool {
        self.countdown.get(index).is_some_and(|c| *c == 0) && self.sink.connected(index)
    }

    /// Send a frame (if [`Streams::due`]); `values` holds the channels one
    /// after the other and is cut to the declared size. `position` is the
    /// "now" marker of the frame (see the stream content).
    pub fn send(&mut self, index: usize, frame: u32, position: f32, values: &[f32]) -> bool {
        if !self.due(index) {
            return false;
        }
        let Some(PortKind::Stream(desc)) = self.info.ports.get(index).map(|p| p.kind) else {
            return false;
        };
        let values = &values[..values.len().min(desc.values())];
        let sent = self.sink.send(index, frame, desc.channels, position, values);
        if sent && let (Some(c), Some(i)) = (self.countdown.get_mut(index), self.interval.get(index)) {
            *c = *i;
        }
        sent
    }
}

/// Everything `process` works with
pub struct Context<'a> {
    /// Frames in this block (every audio slice has this length)
    pub frames: usize,
    pub sample_rate: f64,
    /// Audio inputs, in port order
    pub inputs: &'a [&'a [f32]],
    /// Audio outputs, in port order; never aliasing the inputs
    pub outputs: &'a mut [&'a mut [f32]],
    pub params: Params<'a>,
    pub meters: Meters<'a>,
    /// Events of the block, in frame order
    pub events: &'a [Event],
    /// Last transport state received (persists across blocks)
    pub position: Position,
    pub streams: Streams<'a>,
}
