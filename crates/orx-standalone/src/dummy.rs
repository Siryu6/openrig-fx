//! A private JACK server with the dummy backend: no sound card, no
//! realtime rights, for tests and bundle checks

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const SIGTERM: i32 = 15;

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// Ask a child to stop (SIGTERM), then kill it if it lingers
pub fn stop(child: &mut Child) {
    if let Ok(pid) = i32::try_from(child.id()) {
        // SAFETY: kill(2) on our own child, which has not been reaped yet
        // (we hold its Child), so the PID still names it
        unsafe { kill(pid, SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// A `jackd -d dummy` of this process, stopped on drop
pub struct DummyServer {
    name: String,
    child: Child,
}

impl DummyServer {
    /// Start a server named after `tag` and this process, make it the
    /// default server of this process (`JACK_DEFAULT_SERVER`, also
    /// inherited by child processes) and wait until it accepts clients
    ///
    /// # Safety
    ///
    /// Sets an environment variable: no other thread may read or write
    /// the environment meanwhile (call it first thing in `main` or in a
    /// test binary's only test).
    pub unsafe fn start(tag: &str, sample_rate: u32, period: u32) -> Result<Self, String> {
        let name = format!("orx-{tag}-{}", std::process::id());
        let child = Command::new("jackd")
            .args([
                "-n",
                &name,
                "--no-realtime",
                "-d",
                "dummy",
                "-r",
                &sample_rate.to_string(),
                "-p",
                &period.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("jackd: {e}"))?;
        // SAFETY: no concurrent environment access (the caller's contract)
        unsafe { std::env::set_var("JACK_DEFAULT_SERVER", &name) };
        let mut server = Self { name, child };
        server.wait_ready()?;
        Ok(server)
    }

    /// Server name, for `JACK_DEFAULT_SERVER`
    pub fn name(&self) -> &str {
        &self.name
    }

    fn wait_ready(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return Err(format!("jackd exited: {status}"));
            }
            if crate::probe::server_answers() {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err("jackd did not start within 10 s".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for DummyServer {
    fn drop(&mut self) {
        stop(&mut self.child);
    }
}
