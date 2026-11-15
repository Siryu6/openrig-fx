//! CPU benchmark harness.
//!
//! Measures a plugin through its [`Runner`] on a guitar-like signal with
//! default (or given) parameters, in host-sized blocks, and reports the
//! cost per sample and the share of a real-time period it takes. Uses the
//! median of several runs to be robust to noise.

use orx_plugin::process::NoStreams;
use orx_plugin::runner::Runner;
use orx_plugin::{Config, Plugin};
use std::hint::black_box;
use std::time::Instant;

use crate::signal;

/// Result of a benchmark
#[derive(Debug, Clone, Copy)]
pub struct Report {
    /// Median nanoseconds per frame (all channels)
    pub ns_per_frame: f64,
    /// Share of real time used at the benchmark's sample rate, percent
    pub realtime_percent: f64,
    pub block: usize,
    pub sample_rate: f64,
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:7.2} ns/frame  {:6.3} % of real time  (block {}, {} Hz)",
            self.ns_per_frame, self.realtime_percent, self.block, self.sample_rate
        )
    }
}

/// Benchmark `P` with parameters set by `setup` (port index, value)
pub fn run<P: Plugin>(sample_rate: f64, block: usize, setup: &[(usize, f32)]) -> Report {
    let seconds = 2.0;
    let len = (seconds * sample_rate) as usize / block * block;
    let input = signal::pluck(110.0, 0.8, 2.0, sample_rate, len);
    let ins = vec![input; P::INFO.audio_inputs()];
    let mut outs = vec![vec![0.0f32; block]; P::INFO.audio_outputs()];
    let mut r = Runner::<P>::new(Config { sample_rate, max_block: block });
    for (i, v) in setup {
        r.set_param(*i, *v);
    }
    r.activate();
    let mut times = Vec::new();
    for round in 0..7 {
        let t = Instant::now();
        for start in (0..len).step_by(block) {
            let mut i: [&[f32]; 8] = [&[]; 8];
            for (s, b) in i.iter_mut().zip(&ins) {
                *s = &b[start..start + block];
            }
            let mut o: [&mut [f32]; 8] = Default::default();
            for (s, b) in o.iter_mut().zip(outs.iter_mut()) {
                *s = &mut b[..];
            }
            r.process(block, black_box(&i[..ins.len()]), &mut o[..P::INFO.audio_outputs()], &mut NoStreams);
            black_box(&mut outs);
        }
        // First round warms caches and branch predictors
        if round > 0 {
            times.push(t.elapsed().as_secs_f64());
        }
    }
    times.sort_by(f64::total_cmp);
    let median = times[times.len() / 2];
    Report { ns_per_frame: median * 1e9 / len as f64, realtime_percent: median / seconds * 100.0, block, sample_rate }
}
