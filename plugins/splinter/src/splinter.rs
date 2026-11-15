//! The plugin: HOLD and its quantised start, the engines, the chop, the
//! crush, the mix and the grid stream.

use crate::clock::Clock;
use crate::crush::Crush;
use crate::info::{Action, BPM_MIN, GRID_CELLS, REPEATS_MAX, SLICE_MAX_QUARTERS, SPLINTER, STOP_MAX_BEATS, Scope};
use crate::pattern::{Chop, PATTERNS};
use crate::ramp::Ramp;
use crate::slicer::{Plan, Slicer};
use crate::tape::Tape;
use orx_dsp::Smoother;
use orx_dsp::delay::DelayLine;
use orx_plugin::view::StreamContent;
use orx_plugin::{Config, Context, EventKind, Param, Plugin, PluginInfo, PortKind};

/// A press this long after a grid line still starts on that line,
/// seconds (at most a quarter of the slice)
const LATE_S: f64 = 0.040;
/// Fade of the chop and crush when HOLD engages them, seconds
const SCOPE_FADE_S: f64 = 0.003;
const ROWS: usize = StreamContent::STEP_GRID_ROWS as usize;
/// A bar not played yet: silent, live, at normal speed
const EMPTY_GRID: [[f32; GRID_CELLS]; ROWS] = [[0.0; GRID_CELLS], [0.0; GRID_CELLS], [1.0; GRID_CELLS]];

#[derive(Debug, Clone, Copy)]
struct Ix {
    hold: usize,
    action: usize,
    slice: usize,
    repeats: usize,
    decay: usize,
    step: usize,
    stop_time: usize,
    tape_start: usize,
    pattern: usize,
    duty: usize,
    scope: usize,
    bits: usize,
    rate: usize,
    chaos: usize,
    mix: usize,
    quantize: usize,
    tempo: usize,
    grid: usize,
}

fn param(index: usize) -> Param {
    match SPLINTER.ports.get(index).map(|p| p.kind) {
        Some(PortKind::Param(p)) => p,
        _ => Param::float(0.0, 0.0, 1.0),
    }
}

/// A press waiting for its grid line
#[derive(Debug, Clone, Copy)]
struct Pending {
    /// Samples to wait
    wait: u64,
    action: Action,
    quarters: f64,
    stop_beats: f64,
}

/// OR Splinter
pub struct Splinter {
    ix: Ix,
    rate: f64,
    clock: Clock,
    /// The input, for captures that start in the past and the tape
    ring: DelayLine,
    slicer: Slicer,
    tape: Tape,
    chop: Chop,
    crush: Crush,
    mix: Smoother,
    duty: Smoother,
    bits: Smoother,
    /// How much the chop and the crush apply, 0 to 1
    scope: Ramp,
    scope_fade: usize,
    held: bool,
    /// HOLD is down and its action started
    engaged: bool,
    pending: Option<Pending>,
    reverse: bool,
    /// Level, source and speed per cell of the bar
    grid: [[f32; GRID_CELLS]; ROWS],
    cell: usize,
    fresh: bool,
}

impl Splinter {
    fn begin(&mut self, p: Pending, since: usize) {
        self.engaged = true;
        match p.action {
            Action::Repeat | Action::Reverse => {
                self.reverse = p.action == Action::Reverse;
                self.slicer.start(&self.ring, since, self.clock.samples(p.quarters), self.reverse);
            }
            Action::TapeStop => self.tape.stop(self.clock.samples(p.stop_beats)),
            Action::None => {}
        }
    }

    /// HOLD went down: start now, on the line just passed, or wait for
    /// the next one
    fn press(&mut self, p: Pending, quantize: bool, grid: bool) {
        if quantize && grid {
            let len = self.clock.samples(p.quarters);
            let since = self.clock.since_line(p.quarters);
            if since <= (0.25 * len).min(LATE_S * self.rate) {
                self.begin(p, since.round() as usize);
            } else {
                self.pending = Some(Pending { wait: self.clock.to_line(p.quarters).round() as u64, ..p });
            }
        } else {
            if !grid {
                // No grid to follow: the press is beat one
                self.clock.anchor();
            }
            self.begin(p, 0);
        }
    }

    fn release(&mut self, tape_start: bool, stop_beats: f64) {
        self.pending = None;
        self.engaged = false;
        self.slicer.release();
        self.tape.release(tape_start, 0.5 * self.clock.samples(stop_beats));
    }
}

impl Plugin for Splinter {
    const INFO: &'static PluginInfo = &SPLINTER;

    fn new(config: &Config) -> Self {
        let i = |s: &str| SPLINTER.port_index(s).unwrap_or(usize::MAX);
        let ix = Ix {
            hold: i("hold"),
            action: i("action"),
            slice: i("slice"),
            repeats: i("repeats"),
            decay: i("decay"),
            step: i("step"),
            stop_time: i("stop_time"),
            tape_start: i("tape_start"),
            pattern: i("pattern"),
            duty: i("duty"),
            scope: i("scope"),
            bits: i("bits"),
            rate: i("rate"),
            chaos: i("chaos"),
            mix: i("mix"),
            quantize: i("quantize"),
            tempo: i("tempo"),
            grid: i("grid"),
        };
        let rate = config.sample_rate;
        let beat_max = 60.0 / BPM_MIN * rate;
        let slicer = Slicer::new(rate, SLICE_MAX_QUARTERS * beat_max);
        // The tape's longest delay, and a capture starting in the past
        let ring_len = Tape::max_delay(STOP_MAX_BEATS * beat_max).max((LATE_S * rate) + slicer.edge() as f64);
        let smoother = |i: usize| {
            let p = param(i);
            Smoother::new(p.smoothing, rate as f32, p.default)
        };
        Self {
            ix,
            rate,
            clock: Clock::new(rate),
            ring: DelayLine::new(ring_len.ceil() as usize + 16),
            slicer,
            tape: Tape::new(rate),
            chop: Chop::new(rate),
            crush: Crush::new(rate),
            mix: smoother(ix.mix),
            duty: smoother(ix.duty),
            bits: smoother(ix.bits),
            scope: Ramp::at(0.0),
            scope_fade: (SCOPE_FADE_S * rate) as usize,
            held: false,
            engaged: false,
            pending: None,
            reverse: false,
            grid: EMPTY_GRID,
            cell: usize::MAX,
            fresh: true,
        }
    }

    fn reset(&mut self) {
        self.clock.reset();
        self.ring.reset();
        self.slicer.reset();
        self.tape.reset();
        self.chop.reset();
        self.crush.reset();
        self.held = false;
        self.engaged = false;
        self.pending = None;
        self.grid = EMPTY_GRID;
        self.cell = usize::MAX;
        self.fresh = true;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let ix = self.ix;
        let p = ctx.params;
        let hold = p.on(ix.hold);
        let press = Pending {
            wait: 0,
            action: Action::from_value(p.int(ix.action)),
            quarters: orx_plugin::tempo::DIVISIONS
                .get(usize::try_from(p.int(ix.slice)).unwrap_or(0))
                .map_or(0.5, |d| d.quarters)
                .clamp(1.0 / 12.0, SLICE_MAX_QUARTERS),
            stop_beats: f64::from(p.get(ix.stop_time)),
        };
        let plan = Plan {
            repeats: p.int(ix.repeats).clamp(0, REPEATS_MAX) as u32,
            decay_db: p.get(ix.decay),
            step: p.get(ix.step),
            chaos: p.get(ix.chaos) * 0.01,
        };
        let pattern = usize::try_from(p.int(ix.pattern) - 1).ok().and_then(|i| PATTERNS.get(i)).copied();
        let scope = Scope::from_value(p.int(ix.scope));
        let (tape_start, quantize) = (p.on(ix.tape_start), p.on(ix.quantize));
        let rate = p.get(ix.rate);
        let tempo = p.get(ix.tempo);
        let targets = [p.get(ix.mix), p.get(ix.duty), p.get(ix.bits)];
        if self.fresh {
            self.fresh = false;
            for (s, t) in [&mut self.mix, &mut self.duty, &mut self.bits].into_iter().zip(targets) {
                s.reset(t);
            }
            self.scope = Ramp::at(if scope == Scope::Always { 1.0 } else { 0.0 });
        } else {
            for (s, t) in [&mut self.mix, &mut self.duty, &mut self.bits].into_iter().zip(targets) {
                s.set_target(t);
            }
        }
        self.clock.set_tempo(tempo);

        let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) else {
            return;
        };
        let mut events = ctx.events.iter().peekable();
        for i in 0..ctx.frames {
            while let Some(e) = events.next_if(|e| e.frame as usize <= i) {
                if let EventKind::Position(pos) = e.kind {
                    self.clock.position(&pos);
                    self.clock.set_tempo(tempo);
                }
            }
            let x = input.get(i).copied().filter(|x| x.is_finite()).unwrap_or(0.0);
            self.ring.write(x);
            if i == 0 && hold != self.held {
                self.held = hold;
                if hold {
                    let grid = self.clock.is_tracking() || (scope == Scope::Always && pattern.is_some());
                    self.press(press, quantize, grid);
                } else {
                    self.release(tape_start, press.stop_beats);
                }
            }
            if let Some(w) = self.pending {
                if w.wait == 0 {
                    self.pending = None;
                    self.begin(w, 0);
                } else {
                    self.pending = Some(Pending { wait: w.wait - 1, ..w });
                }
            }
            let fx_on = scope == Scope::Always || self.engaged;
            let fx_to = if fx_on { 1.0 } else { 0.0 };
            if self.scope.target() != fx_to {
                self.scope.go(fx_to, self.scope_fade);
            }

            // The engines
            let (live_s, repeats) = self.slicer.process(x, &plan, self.reverse);
            let live_t = self.tape.live();
            let tape = self.tape.process(&self.ring);
            let mut wet = x * (live_s * live_t) + repeats + tape;

            // Chop and crush
            let fx = self.scope.next();
            let duty = 0.01 * self.duty.next();
            let gate = self.chop.next(pattern, self.clock.pos(), duty, plan.chaos);
            // Blends written to land exactly on either end
            let gain = fx * gate + (1.0 - fx);
            wet *= gain;
            let bits = self.bits.next();
            if !self.crush.is_off(bits, rate) {
                let c = self.crush.next(wet, bits, rate);
                wet = fx * c + (1.0 - fx) * wet;
            }
            let m = 0.01 * self.mix.next();
            if let Some(y) = output.get_mut(i) {
                *y = wet * m + x * (1.0 - m);
            }

            // The grid
            let (source, (speed, level)) = if self.tape.is_idle() {
                (self.slicer.source(), self.slicer.speed_level())
            } else {
                let v = self.tape.speed();
                (0.0, (v, v.sqrt()))
            };
            let cell = ((self.clock.pos() / self.clock.bar() * GRID_CELLS as f64) as usize).min(GRID_CELLS - 1);
            let level = level * gain;
            let [lv, src, spd] = &mut self.grid;
            if cell != self.cell {
                self.cell = cell;
                lv[cell] = level;
                src[cell] = source;
                spd[cell] = speed;
            } else {
                lv[cell] = lv[cell].min(level);
                src[cell] = src[cell].max(source);
            }
            self.clock.advance();
        }

        if ctx.streams.due(ix.grid) {
            let mut values = [0.0f32; ROWS * GRID_CELLS];
            for (dst, row) in values.as_chunks_mut::<GRID_CELLS>().0.iter_mut().zip(&self.grid) {
                dst.copy_from_slice(row);
            }
            let at = (self.clock.pos() / self.clock.bar()) as f32;
            ctx.streams.send(ix.grid, (ctx.frames.max(1) - 1) as u32, at, &values);
        }
    }
}
