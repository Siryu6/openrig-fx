//! Play a signal into JACK ports and record others: an end-to-end check
//! of any JACK client (jalv, `orx-run`)

use jack::{AudioIn, AudioOut, Client, ClientOptions, Control, Port, ProcessScope};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Does the default JACK server accept a client?
pub fn server_answers() -> bool {
    Client::new("orx-ping", ClientOptions::NO_START_SERVER).is_ok()
}

struct Player {
    outs: Vec<Port<AudioOut>>,
    ins: Vec<Port<AudioIn>>,
    signal: Vec<Vec<f32>>,
    recorded: Vec<Vec<f32>>,
    position: usize,
    /// Set once every connection is made: silence until then
    start: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
}

impl jack::ProcessHandler for Player {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> Control {
        if !self.start.load(Ordering::Acquire) {
            for port in &mut self.outs {
                port.as_mut_slice(ps).fill(0.0);
            }
            return Control::Continue;
        }
        let n = ps.n_frames() as usize;
        let len = self.recorded.first().map_or(0, Vec::len);
        let start = self.position.min(len);
        let end = (self.position + n).min(len);
        for (port, signal) in self.outs.iter_mut().zip(&self.signal) {
            let out = port.as_mut_slice(ps);
            out.fill(0.0);
            let from = self.position.min(signal.len());
            let to = (self.position + n).min(signal.len());
            out[..to - from].copy_from_slice(&signal[from..to]);
        }
        for (port, rec) in self.ins.iter().zip(self.recorded.iter_mut()) {
            rec[start..end].copy_from_slice(&port.as_slice(ps)[..end - start]);
        }
        self.position += n;
        if self.position >= len {
            self.done.store(true, Ordering::Release);
        }
        Control::Continue
    }
}

/// Feed `signal[k]` to the port named `sources[k]` and record `frames`
/// frames of each port in `sinks`, from the cycle the signal starts.
/// The ports belong to clients already running on the default server.
pub fn run(sources: &[&str], sinks: &[&str], signal: Vec<Vec<f32>>, frames: usize) -> Result<Vec<Vec<f32>>, String> {
    let (client, _) = Client::new("orx-probe", ClientOptions::NO_START_SERVER).map_err(|e| format!("JACK: {e}"))?;
    let me = client.name().to_string();
    let reg = |e: jack::Error| e.to_string();
    let outs = (0..sources.len())
        .map(|k| client.register_port(&format!("out_{k}"), AudioOut::default()).map_err(reg))
        .collect::<Result<Vec<_>, _>>()?;
    let ins = (0..sinks.len())
        .map(|k| client.register_port(&format!("in_{k}"), AudioIn::default()).map_err(reg))
        .collect::<Result<Vec<_>, _>>()?;
    let done = Arc::new(AtomicBool::new(false));
    let start = Arc::new(AtomicBool::new(false));
    let player = Player {
        outs,
        ins,
        signal,
        recorded: vec![vec![0.0; frames]; sinks.len()],
        position: 0,
        start: start.clone(),
        done: done.clone(),
    };
    let active = client.activate_async((), player).map_err(reg)?;
    let c = active.as_client();
    for (k, s) in sources.iter().enumerate() {
        c.connect_ports_by_name(&format!("{me}:out_{k}"), s).map_err(|e| format!("{s}: {e}"))?;
    }
    for (k, s) in sinks.iter().enumerate() {
        c.connect_ports_by_name(s, &format!("{me}:in_{k}")).map_err(|e| format!("{s}: {e}"))?;
    }
    start.store(true, Ordering::Release);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done.load(Ordering::Acquire) {
        if Instant::now() > deadline {
            return Err("the JACK server stopped processing".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let (_, _, player) = active.deactivate().map_err(reg)?;
    Ok(player.recorded)
}
