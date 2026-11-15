//! One plugin as a JACK client

use crate::queue::{Consumer, Producer, queue};
use jack::{AudioIn, AudioOut, Client, ClientOptions, Control, MidiIn, Port, ProcessScope};
use orx_plugin::dynamic::{DynRunner, Entry};
use orx_plugin::process::NoStreams;
use orx_plugin::runner::MAX_CHANNELS;
use orx_plugin::{Config, Event, EventKind, PluginInfo, PortKind, Position};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// How to run the plugin
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// JACK client name (default: the plugin's slug)
    pub name: Option<String>,
    /// Tempo when the JACK transport has none (no timebase master)
    pub bpm: Option<f32>,
    /// MIDI CC number driving a parameter (by port index), over its range
    pub cc: Vec<(u8, usize)>,
    /// Connect to the first physical capture and playback ports
    pub connect: bool,
}

/// A control change for the audio thread
#[derive(Debug, Clone, Copy)]
enum Command {
    Set(usize, f32),
}

/// What the audio thread publishes
struct Shared {
    /// Last value of each control port (parameters and meters), f32 bits
    values: Box<[AtomicU32]>,
    faults: AtomicU32,
    shut_down: AtomicBool,
}

/// A running plugin; stopped on drop
pub struct Standalone {
    client: Option<jack::AsyncClient<Notifications, Process>>,
    commands: Producer<Command>,
    shared: Arc<Shared>,
    info: &'static PluginInfo,
    name: String,
}

impl Standalone {
    pub fn start(entry: &Entry, options: &Options) -> Result<Self, String> {
        let info = entry.info;
        let name = options.name.clone().unwrap_or_else(|| format!("or-{}", entry.slug()));
        let (client, _) = Client::new(&name, ClientOptions::NO_START_SERVER).map_err(|e| format!("JACK: {e}"))?;
        let name = client.name().to_string();
        let config = Config { sample_rate: f64::from(client.sample_rate()), max_block: client.buffer_size() as usize };

        let mut ins = Vec::new();
        let mut outs = Vec::new();
        let mut has_events = false;
        for p in info.ports {
            match p.kind {
                PortKind::AudioIn => {
                    ins.push(client.register_port(p.symbol, AudioIn::default()).map_err(|e| e.to_string())?)
                }
                PortKind::AudioOut => {
                    outs.push(client.register_port(p.symbol, AudioOut::default()).map_err(|e| e.to_string())?);
                }
                PortKind::Events { .. } => has_events = true,
                _ => {}
            }
        }
        let wants_midi = has_events || !options.cc.is_empty();
        let midi = if wants_midi {
            Some(client.register_port("midi_in", MidiIn::default()).map_err(|e| e.to_string())?)
        } else {
            None
        };

        let mut cc = [None; 128];
        for (number, port) in &options.cc {
            let p = info
                .ports
                .get(*port)
                .and_then(|p| p.as_param())
                .ok_or(format!("CC {number}: port {port} is not a parameter"))?;
            cc[usize::from(*number & 0x7f)] = Some((*port, *p));
        }

        let mut runner = (entry.create)(config);
        runner.activate();
        let shared = Arc::new(Shared {
            values: info.ports.iter().map(|_| AtomicU32::new(0)).collect(),
            faults: AtomicU32::new(0),
            shut_down: AtomicBool::new(false),
        });
        let (producer, consumer) = queue(1024);
        let transport = client.transport();
        let process = Process {
            runner,
            info,
            ins,
            outs,
            midi,
            forward_midi: info.ports.iter().any(|p| matches!(p.kind, PortKind::Events { midi: true, .. })),
            cc,
            commands: consumer,
            shared: shared.clone(),
            transport,
            fixed_bpm: options.bpm,
            last_position: None,
        };
        process.publish();
        let active = client
            .activate_async(Notifications { shared: shared.clone() }, process)
            .map_err(|e| format!("JACK: {e}"))?;
        let me = Self { client: Some(active), commands: producer, shared, info, name };
        if options.connect {
            me.connect_physical()?;
        }
        Ok(me)
    }

    /// JACK client name
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn info(&self) -> &'static PluginInfo {
        self.info
    }

    /// Set a parameter by port index; false when the queue is full or the
    /// port is not a parameter
    pub fn set(&mut self, port: usize, value: f32) -> bool {
        self.info.ports.get(port).is_some_and(|p| p.as_param().is_some() || p.kind == PortKind::Enabled)
            && self.commands.push(Command::Set(port, value))
    }

    /// Current value of a control port (parameter or meter)
    pub fn value(&self, port: usize) -> Option<f32> {
        self.shared.values.get(port).map(|v| f32::from_bits(v.load(Ordering::Relaxed)))
    }

    /// Times the runner replaced a non-finite output with silence
    pub fn faults(&self) -> u32 {
        self.shared.faults.load(Ordering::Relaxed)
    }

    /// The JACK server went away
    pub fn shut_down(&self) -> bool {
        self.shared.shut_down.load(Ordering::Relaxed)
    }

    fn connect_physical(&self) -> Result<(), String> {
        let client = self.client.as_ref().ok_or("not running")?.as_client();
        let physical =
            |flags| client.ports(None, Some("32 bit float mono audio"), jack::PortFlags::IS_PHYSICAL | flags);
        let captures = physical(jack::PortFlags::IS_OUTPUT);
        let playbacks = physical(jack::PortFlags::IS_INPUT);
        let mine = |kind: PortKind| -> Vec<String> {
            self.info.ports.iter().filter(|p| p.kind == kind).map(|p| format!("{}:{}", self.name, p.symbol)).collect()
        };
        let (ins, outs) = (mine(PortKind::AudioIn), mine(PortKind::AudioOut));
        // Every input from the first capture (a guitar is mono); each output
        // to its playback channel, a mono output to the first two
        if let Some(capture) = captures.first() {
            for i in &ins {
                client.connect_ports_by_name(capture, i).map_err(|e| e.to_string())?;
            }
        }
        for (k, playback) in playbacks.iter().take(2).enumerate() {
            if let Some(o) = outs.get(k).or(outs.first()) {
                client.connect_ports_by_name(o, playback).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

impl Drop for Standalone {
    fn drop(&mut self) {
        if let Some(c) = self.client.take() {
            let _ = c.deactivate();
        }
    }
}

struct Notifications {
    shared: Arc<Shared>,
}

impl jack::NotificationHandler for Notifications {
    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.shared.shut_down.store(true, Ordering::Relaxed);
    }
}

struct Process {
    runner: Box<dyn DynRunner>,
    info: &'static PluginInfo,
    ins: Vec<Port<AudioIn>>,
    outs: Vec<Port<AudioOut>>,
    midi: Option<Port<MidiIn>>,
    /// The plugin takes MIDI events
    forward_midi: bool,
    /// CC number to (port, parameter)
    cc: [Option<(usize, orx_plugin::Param)>; 128],
    commands: Consumer<Command>,
    shared: Arc<Shared>,
    transport: jack::Transport,
    fixed_bpm: Option<f32>,
    last_position: Option<Position>,
}

impl Process {
    fn publish(&self) {
        for (i, (p, v)) in self.info.ports.iter().zip(self.shared.values.iter()).enumerate() {
            let value = match p.kind {
                PortKind::Param(_) | PortKind::Enabled => self.runner.param(i),
                PortKind::Meter(_) | PortKind::Latency => self.runner.meter(i),
                _ => continue,
            };
            v.store(value.to_bits(), Ordering::Relaxed);
        }
        self.shared.faults.store(self.runner.faults(), Ordering::Relaxed);
    }

    /// The transport's tempo and position, or the fixed tempo
    fn position(&self) -> Option<Position> {
        let t = self.transport.query().ok();
        let rolling = t.as_ref().map(|t| t.state == jack::TransportState::Rolling);
        match t.as_ref().and_then(|t| t.pos.bbt()) {
            Some(bbt) => Some(Position {
                bpm: Some(bbt.bpm as f32),
                speed: rolling.map(|r| if r { 1.0 } else { 0.0 }),
                bar_beat: Some((bbt.beat - 1) as f32 + (bbt.tick as f64 / bbt.ticks_per_beat) as f32),
                beats_per_bar: Some(bbt.sig_num),
                beat_unit: Some(bbt.sig_denom as i32),
            }),
            None => self.fixed_bpm.map(|bpm| Position { bpm: Some(bpm), speed: Some(1.0), ..Position::default() }),
        }
    }
}

impl jack::ProcessHandler for Process {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> Control {
        while let Some(Command::Set(port, value)) = self.commands.pop() {
            self.runner.set_param(port, value);
        }
        if let Some(midi) = &self.midi {
            for m in midi.iter(ps) {
                let &[status, data1, data2] = m.bytes else {
                    if self.forward_midi && !m.bytes.is_empty() && m.bytes.len() < 3 {
                        let mut data = [0; 3];
                        data[..m.bytes.len()].copy_from_slice(m.bytes);
                        let kind = EventKind::Midi { data, len: m.bytes.len() as u8 };
                        self.runner.push_event(Event { frame: m.time, kind });
                    }
                    continue;
                };
                if status & 0xf0 == 0xb0
                    && let Some((port, param)) = self.cc[usize::from(data1 & 0x7f)]
                {
                    self.runner.set_param(port, param.from_normalized(f32::from(data2) / 127.0));
                }
                if self.forward_midi {
                    let kind = EventKind::Midi { data: [status, data1, data2], len: 3 };
                    self.runner.push_event(Event { frame: m.time, kind });
                }
            }
        }
        let position = self.position();
        if position.is_some() && position != self.last_position {
            if let Some(p) = position {
                self.runner.push_event(Event { frame: 0, kind: EventKind::Position(p) });
            }
            self.last_position = position;
        }

        let n = ps.n_frames() as usize;
        let (n_in, n_out) = (self.ins.len(), self.outs.len());
        let mut inputs: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
        for (slot, port) in inputs.iter_mut().zip(&self.ins) {
            *slot = port.as_slice(ps);
        }
        let mut outputs: [&mut [f32]; MAX_CHANNELS] = Default::default();
        for (slot, port) in outputs.iter_mut().zip(self.outs.iter_mut()) {
            *slot = port.as_mut_slice(ps);
        }
        self.runner.process(n, &inputs[..n_in], &mut outputs[..n_out], &mut NoStreams);
        self.publish();
        Control::Continue
    }
}
