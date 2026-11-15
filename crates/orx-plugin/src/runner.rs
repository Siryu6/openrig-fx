//! The part of a plugin host every format shares.
//!
//! [`Runner`] wraps a [`Plugin`] and gives wrappers a simple contract:
//! set control values, push events, call [`Runner::process`] with
//! non-aliasing input and output slices of any length, read the control
//! outputs back. It takes care of:
//!
//! - **block splitting**: blocks longer than `max_block` are processed in
//!   pieces, events moved to the right piece;
//! - **parameter sanitising**: NaN, out-of-range, non-integer values never
//!   reach the plugin;
//! - **denormals**: flush-to-zero is on during processing;
//! - **bypass** (`lv2:enabled` port): 10 ms linear crossfade with the dry
//!   signal delayed by the reported latency; once bypassed the plugin is
//!   not run, and it is reset before it fades back in. A plugin with
//!   [`Plugin::TRAILS`] has its input faded instead and rings out first
//!   (see there);
//! - **non-finite output**: a block with NaN or infinity is replaced by
//!   silence and the plugin is reset (counted in [`Runner::faults`]);
//! - **latency** output port and rate-limited **streams**.

use orx_dsp::delay::DelayLine;
use orx_dsp::xfade::Fade;
use orx_dsp::{FtzGuard, alloc};

use crate::desc::{PluginInfo, PortKind};
use crate::process::{Context, Event, EventKind, Meters, Params, Position, StreamSink, Streams};
use crate::{Config, Plugin};

/// Most audio channels on each side
pub const MAX_CHANNELS: usize = 8;
/// Most events per `process` call (more are dropped)
pub const MAX_EVENTS: usize = 256;
/// Bypass crossfade time
pub const BYPASS_MS: f64 = 10.0;
/// Longest a bypassed [`Plugin::TRAILS`] plugin rings (a delay at full
/// feedback would hold forever) before its tail fades out in
/// [`BYPASS_MS`], seconds
pub const TRAIL_LIMIT_S: f64 = 30.0;

/// A plugin and the state its host needs
pub struct Runner<P: Plugin> {
    plugin: P,
    config: Config,
    /// Control inputs by port index (sanitised)
    params: Box<[f32]>,
    /// Control outputs by port index
    meters: Box<[f32]>,
    events: Box<[Event]>,
    event_count: usize,
    /// Events of the piece being processed, frames relative to it
    piece_events: Box<[Event]>,
    position: Position,
    countdown: Box<[u32]>,
    interval: Box<[u32]>,
    enabled_port: Option<usize>,
    latency_port: Option<usize>,
    fade: Fade,
    /// Bypassed and faded out: the plugin is not running
    idle: bool,
    /// Dry signal per output, delayed by the latency (when the plugin may
    /// report any)
    dry: Box<[DelayLine]>,
    /// Fade gain of each frame of the piece while the bypass fade moves
    gains: Box<[f32]>,
    /// Trails: the faded input of the plugin, per input
    trail_input: Box<[Box<[f32]>]>,
    /// Trails: frames rung since the input faded out
    trail_frames: u64,
    /// Trails: fades the tail out when it rings past the limit
    tail: Fade,
    faults: u32,
}

impl<P: Plugin> Runner<P> {
    /// Create the plugin and every buffer. Not real-time safe.
    pub fn new(config: Config) -> Self {
        orx_simd::init();
        let info = P::INFO;
        let config = Config {
            sample_rate: if config.sample_rate.is_finite() && config.sample_rate > 0.0 {
                config.sample_rate
            } else {
                48_000.0
            },
            max_block: config.max_block.clamp(1, 1 << 16),
        };
        let ports = info.ports.len();
        let params: Box<[f32]> = alloc::from_fn(ports, |i| match info.ports[i].kind {
            PortKind::Param(p) => p.default,
            PortKind::Enabled => 1.0,
            _ => 0.0,
        });
        let interval = alloc::from_fn(ports, |i| match info.ports[i].kind {
            PortKind::Stream(s) => {
                let rate = if s.max_rate > 0.0 { f64::from(s.max_rate) } else { 30.0 };
                (config.sample_rate / rate).ceil() as u32
            }
            _ => 0,
        });
        let max_latency = P::max_latency(&config) as usize;
        let outputs = info.audio_outputs();
        let dry = if max_latency > 0 {
            alloc::from_fn(outputs, |_| DelayLine::new(max_latency + 1))
        } else {
            alloc::from_fn(0, |_| DelayLine::new(0))
        };
        let fade_len = (BYPASS_MS * 0.001 * config.sample_rate) as usize;
        debug_assert!(!P::TRAILS || max_latency == 0, "{}: trails need a plugin without latency", info.uri);
        let trail_inputs = if P::TRAILS { info.audio_inputs().min(MAX_CHANNELS) } else { 0 };
        Self {
            plugin: P::new(&config),
            config,
            params,
            meters: alloc::zeroed(ports),
            events: alloc::from_fn(MAX_EVENTS, |_| EMPTY_EVENT),
            event_count: 0,
            piece_events: alloc::from_fn(MAX_EVENTS, |_| EMPTY_EVENT),
            position: Position::default(),
            countdown: alloc::from_fn(ports, |_| 0),
            interval,
            enabled_port: info.ports.iter().position(|p| matches!(p.kind, PortKind::Enabled)),
            latency_port: info.ports.iter().position(|p| matches!(p.kind, PortKind::Latency)),
            fade: Fade::new(fade_len, true),
            idle: false,
            dry,
            gains: alloc::zeroed(config.max_block),
            trail_input: alloc::from_fn(trail_inputs, |_| alloc::zeroed(config.max_block)),
            trail_frames: 0,
            tail: Fade::new(fade_len, true),
            faults: 0,
        }
    }

    pub fn info(&self) -> &'static PluginInfo {
        P::INFO
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn plugin(&self) -> &P {
        &self.plugin
    }

    pub fn plugin_mut(&mut self) -> &mut P {
        &mut self.plugin
    }

    /// Start of a stream (LV2 `activate`): clear the audio state
    pub fn activate(&mut self) {
        self.plugin.reset();
        for d in self.dry.iter_mut() {
            d.reset();
        }
        let on = self.enabled_port.is_none_or(|i| self.params.get(i).is_some_and(|v| *v >= 0.5));
        self.fade.set(on);
        self.fade.finish();
        self.idle = !on;
        self.restart_trail();
        self.event_count = 0;
        self.countdown.fill(0);
    }

    /// Set the control input at `index` (sanitised; other ports ignored)
    #[inline]
    pub fn set_param(&mut self, index: usize, raw: f32) {
        let Some(port) = P::INFO.ports.get(index) else { return };
        let value = match port.kind {
            PortKind::Param(p) => p.sanitize(raw),
            PortKind::Enabled => {
                if raw.is_nan() || raw >= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            _ => return,
        };
        if let Some(v) = self.params.get_mut(index) {
            *v = value;
        }
    }

    /// Current value of the control input at `index`
    pub fn param(&self, index: usize) -> f32 {
        self.params.get(index).copied().unwrap_or(0.0)
    }

    /// Current value of the control output at `index`
    pub fn meter(&self, index: usize) -> f32 {
        self.meters.get(index).copied().unwrap_or(0.0)
    }

    /// Queue an event for the next `process` (frame relative to that
    /// call, events in frame order). False when the queue is full.
    #[inline]
    pub fn push_event(&mut self, event: Event) -> bool {
        match self.events.get_mut(self.event_count) {
            Some(slot) => {
                *slot = event;
                self.event_count += 1;
                true
            }
            None => false,
        }
    }

    /// Latency the plugin reports now, frames
    pub fn latency(&self) -> u32 {
        self.plugin.latency()
    }

    /// Bypassed and no longer running the plugin (for trails: rung out)
    pub fn is_idle(&self) -> bool {
        self.idle
    }

    fn restart_trail(&mut self) {
        self.trail_frames = 0;
        self.tail.set(true);
        self.tail.finish();
    }

    /// Blocks replaced by silence because the plugin output NaN or infinity
    pub fn faults(&self) -> u32 {
        self.faults
    }

    /// Process `frames` frames. `inputs` and `outputs` hold one slice per
    /// audio port (at least `frames` long) and must not alias. Real-time
    /// safe.
    pub fn process(&mut self, frames: usize, inputs: &[&[f32]], outputs: &mut [&mut [f32]], sink: &mut dyn StreamSink) {
        let _ftz = FtzGuard::new();
        let info = P::INFO;
        let n_in = inputs.len().min(MAX_CHANNELS);
        let n_out = outputs.len().min(MAX_CHANNELS);
        let frames = inputs
            .iter()
            .take(n_in)
            .map(|s| s.len())
            .chain(outputs.iter().take(n_out).map(|s| s.len()))
            .fold(frames, usize::min);
        let enabled = self.enabled_port.is_none_or(|i| self.params.get(i).is_some_and(|v| *v >= 0.5));
        let latency = self.plugin.latency() as usize;

        let mut start = 0;
        let mut next_event = 0;
        let events = self.event_count.min(MAX_EVENTS);
        while start < frames {
            let n = (frames - start).min(self.config.max_block);
            let end = start + n;

            // Events of this piece, relative to it
            let mut count = 0;
            while next_event < events {
                let Some(e) = self.events.get(next_event).copied() else { break };
                let frame = e.frame as usize;
                if frame >= end && end < frames {
                    break;
                }
                if let EventKind::Position(p) = e.kind {
                    merge_position(&mut self.position, &p);
                }
                if let Some(slot) = self.piece_events.get_mut(count) {
                    *slot = Event { frame: frame.saturating_sub(start).min(n - 1) as u32, ..e };
                    count += 1;
                }
                next_event += 1;
            }

            let mut ins: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
            for (slot, input) in ins.iter_mut().zip(inputs.iter()) {
                *slot = &input[start..end];
            }
            let mut outs: [&mut [f32]; MAX_CHANNELS] = Default::default();
            for (slot, output) in outs.iter_mut().zip(outputs.iter_mut()) {
                *slot = &mut output[start..end];
            }

            if enabled && self.idle {
                self.plugin.reset();
                self.idle = false;
            }
            if enabled {
                self.restart_trail();
            }
            self.fade.set(enabled);

            if self.idle {
                copy_dry(&ins[..n_in], &mut outs[..n_out], &mut self.dry, latency);
            } else {
                let fading = !(self.fade.is_settled() && self.fade.position() == 1.0);
                if fading {
                    for g in self.gains[..n].iter_mut() {
                        *g = self.fade.next();
                    }
                }
                let Self { plugin, params, meters, piece_events, countdown, interval, position, config, .. } = self;
                // Trails: the plugin hears its input faded by the bypass
                let mut faded: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
                let plugin_ins = if P::TRAILS && fading {
                    for ((buf, src), slot) in self.trail_input.iter_mut().zip(ins.iter()).zip(faded.iter_mut()) {
                        for ((b, x), g) in buf[..n].iter_mut().zip(src.iter()).zip(self.gains.iter()) {
                            *b = x * g;
                        }
                        *slot = &buf[..n];
                    }
                    &faded[..n_in]
                } else {
                    &ins[..n_in]
                };
                // Short-lived reborrows: `Context` ties the slice list and
                // the slices to one lifetime
                let mut wet: [&mut [f32]; MAX_CHANNELS] = Default::default();
                for (slot, o) in wet.iter_mut().zip(outs.iter_mut()) {
                    *slot = &mut **o;
                }
                let mut ctx = Context {
                    frames: n,
                    sample_rate: config.sample_rate,
                    inputs: plugin_ins,
                    outputs: &mut wet[..n_out],
                    params: Params { values: params },
                    meters: Meters { values: meters },
                    events: &piece_events[..count],
                    position: *position,
                    streams: Streams { sink, countdown, interval, info },
                };
                plugin.process(&mut ctx);

                // Non-finite output: silence and start over
                let bad = outs[..n_out].iter().any(|o| o.iter().any(|x| !x.is_finite()));
                if bad {
                    for o in outs[..n_out].iter_mut() {
                        o.fill(0.0);
                    }
                    self.plugin.reset();
                    self.faults = self.faults.saturating_add(1);
                }

                if !fading {
                    feed_dry(&ins[..n_in], &mut self.dry, n);
                } else if P::TRAILS {
                    add_dry(&ins[..n_in], &mut outs[..n_out], &mut self.dry, &self.gains[..n], &mut self.tail);
                    if self.fade.is_off() {
                        self.trail_frames += n as u64;
                        if self.trail_frames as f64 > TRAIL_LIMIT_S * self.config.sample_rate {
                            self.tail.set(false);
                        }
                        if !self.plugin.is_ringing() || self.tail.is_off() {
                            self.idle = true;
                        }
                    }
                } else {
                    crossfade(&ins[..n_in], &mut outs[..n_out], &mut self.dry, latency, &self.gains[..n]);
                    if self.fade.is_off() {
                        self.idle = true;
                    }
                }
            }

            for c in self.countdown.iter_mut() {
                *c = c.saturating_sub(n as u32);
            }
            start = end;
        }

        self.event_count = 0;
        if let Some(i) = self.latency_port
            && let Some(v) = self.meters.get_mut(i)
        {
            *v = latency as f32;
        }
    }
}

const EMPTY_EVENT: Event = Event { frame: 0, kind: EventKind::Midi { data: [0; 3], len: 0 } };

fn merge_position(into: &mut Position, p: &Position) {
    into.bpm = p.bpm.or(into.bpm);
    into.speed = p.speed.or(into.speed);
    into.bar_beat = p.bar_beat.or(into.bar_beat);
    into.beats_per_bar = p.beats_per_bar.or(into.beats_per_bar);
    into.beat_unit = p.beat_unit.or(into.beat_unit);
}

/// Input feeding output `k`: the input of the same rank, else the last
#[inline]
fn dry_source<'a>(ins: &[&'a [f32]], k: usize) -> Option<&'a [f32]> {
    ins.get(k.min(ins.len().saturating_sub(1))).copied().filter(|_| !ins.is_empty())
}

/// Bypassed: outputs carry the (latency-aligned) dry signal
fn copy_dry(ins: &[&[f32]], outs: &mut [&mut [f32]], dry: &mut [DelayLine], latency: usize) {
    for (k, out) in outs.iter_mut().enumerate() {
        match (dry_source(ins, k), dry.get_mut(k)) {
            (Some(src), Some(line)) => {
                for (o, x) in out.iter_mut().zip(src.iter()) {
                    line.write(*x);
                    *o = line.read(latency);
                }
            }
            (Some(src), None) => out.copy_from_slice(&src[..out.len()]),
            (None, _) => out.fill(0.0),
        }
    }
}

/// Fully on: keep the dry delay lines fed
fn feed_dry(ins: &[&[f32]], dry: &mut [DelayLine], n: usize) {
    for (k, line) in dry.iter_mut().enumerate() {
        if let Some(src) = dry_source(ins, k) {
            for x in &src[..n] {
                line.write(*x);
            }
        }
    }
}

/// Fading: out = dry·(1 − g) + wet·g, `g` the fade gain of each frame
fn crossfade(ins: &[&[f32]], outs: &mut [&mut [f32]], dry: &mut [DelayLine], latency: usize, gains: &[f32]) {
    for (k, out) in outs.iter_mut().enumerate() {
        let src = dry_source(ins, k);
        let mut line = dry.get_mut(k);
        for (i, (o, g)) in out.iter_mut().zip(gains).enumerate() {
            let x = src.and_then(|s| s.get(i)).copied().unwrap_or(0.0);
            let d = match line.as_deref_mut() {
                Some(l) => {
                    l.write(x);
                    l.read(latency)
                }
                None => x,
            };
            *o = d + (*o - d) * g;
        }
    }
}

/// Trails, the plugin having heard its input faded by `gains`:
/// out = tail·plugin + dry·(1 − g), `tail` fading the plugin out when it
/// rang too long
fn add_dry(ins: &[&[f32]], outs: &mut [&mut [f32]], dry: &mut [DelayLine], gains: &[f32], tail: &mut Fade) {
    let start = *tail;
    for (k, out) in outs.iter_mut().enumerate() {
        // Every channel follows the same fade
        let mut t = start;
        let src = dry_source(ins, k);
        let mut line = dry.get_mut(k);
        for (i, (o, g)) in out.iter_mut().zip(gains).enumerate() {
            let x = src.and_then(|s| s.get(i)).copied().unwrap_or(0.0);
            // No latency with trails: the line only stays fed
            if let Some(l) = line.as_deref_mut() {
                l.write(x);
            }
            *o = *o * t.next() + x * (1.0 - g);
        }
        *tail = t;
    }
    if outs.is_empty() {
        for _ in gains {
            tail.next();
        }
    }
}

#[cfg(test)]
mod tests;
