//! The slice engine: capture a slice of the input and repeat it.
//!
//! Capture is forward, as in a sampler's beat repeat: from the start `c` (a
//! grid line), the input plays on for one slice length `L` while it is
//! recorded, then the recording repeats, period after period, each period
//! exactly `L` long so the repeats stay on the grid. Repeat `n` (from 1)
//! plays at speed `2^((n − 1)·step/12)` and `−(n − 1)·decay` dB; a slower
//! repeat is cut at the end of its period, a faster one starts over
//! inside it.
//!
//! Every start of a slice fades in over a few milliseconds while what was
//! playing fades out reading on past its end: the recording keeps a few
//! milliseconds before the slice and after it for that, so no edge is
//! ever cut.

use crate::ramp::Ramp;
use orx_dsp::alloc;
use orx_dsp::delay::DelayLine;
use orx_dsp::math::db_to_gain;
use orx_dsp::noise::Rng;

/// Longest fade at a slice edge, seconds
const FADE_S: f64 = 0.003;
/// Shortest fade, samples
const FADE_MIN: usize = 8;
/// Shortest roll segment, seconds
const ROLL_MIN_S: f64 = 0.010;
/// Speed limits of the pitch steps (two octaves either way)
const SPEED_MIN: f64 = 0.25;
const SPEED_MAX: f64 = 4.0;
/// Share of the repeats chaos changes at most
const CHAOS_EVENTS: f32 = 0.6;
const VOICES: usize = 3;

/// Settings read each block
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    /// Repeats after the slice (0: until released)
    pub repeats: u32,
    /// Level lost per repeat, dB
    pub decay_db: f32,
    /// Pitch change per repeat, semitones
    pub step: f32,
    /// 0 to 1
    pub chaos: f32,
}

/// One reading of the recording
#[derive(Debug, Clone, Copy)]
struct Voice {
    on: bool,
    /// Fading out for good
    dying: bool,
    buf: usize,
    /// Read position in the slice, samples (0 its start)
    pos: f64,
    /// Samples read per sample; negative backward
    speed: f64,
    gain: f32,
    env: Ramp,
    fade: usize,
}

const IDLE: Voice =
    Voice { on: false, dying: false, buf: 0, pos: 0.0, speed: 1.0, gain: 0.0, env: Ramp::at(0.0), fade: FADE_MIN };

/// What a period plays
#[derive(Debug, Clone, Copy)]
struct Period {
    speed: f64,
    gain: f32,
}

/// The slice engine
pub struct Slicer {
    bufs: [Box<[f32]>; 2],
    cur: usize,
    /// Samples kept before and after the slice
    edge: usize,
    max_fade: usize,
    roll_min: f64,
    /// Slice length, samples
    len: f64,
    recorded: usize,
    record_to: usize,
    /// Samples since the start of the slice
    t: u64,
    /// Period now (0: the slice itself, playing live)
    period: u32,
    /// Next segment boundary, in `t`; segments left in the period after
    /// this one (rolls), and their length
    seg_end: f64,
    segs_left: u32,
    seg_len: f64,
    now: Period,
    voices: [Voice; VOICES],
    /// Gain of the input
    live: Ramp,
    active: bool,
    captures: u32,
    rng: Rng,
}

/// 4-point cubic Hermite read of `buf` at `p` (0 outside)
#[inline]
fn read(buf: &[f32], p: f64) -> f32 {
    let i = p.floor();
    let t = (p - i) as f32;
    let at = |k: f64| if k >= 0.0 { buf.get(k as usize).copied().unwrap_or(0.0) } else { 0.0 };
    let (xm1, x0, x1, x2) = (at(i - 1.0), at(i), at(i + 1.0), at(i + 2.0));
    let c1 = 0.5 * (x1 - xm1);
    let c2 = xm1 - 2.5 * x0 + 2.0 * x1 - 0.5 * x2;
    let c3 = 0.5 * (x2 - xm1) + 1.5 * (x0 - x1);
    ((c3 * t + c2) * t + c1) * t + x0
}

impl Slicer {
    /// An engine for slices up to `max_len` samples
    pub fn new(sample_rate: f64, max_len: f64) -> Self {
        let max_fade = (FADE_S * sample_rate).ceil() as usize;
        // A voice fading out reads on for at most two fades at the
        // highest speed past either end
        let edge = 2 * max_fade * SPEED_MAX as usize + 8;
        let size = 2 * edge + max_len.ceil() as usize + 8;
        Self {
            bufs: [alloc::zeroed(size), alloc::zeroed(size)],
            cur: 0,
            edge,
            max_fade,
            roll_min: ROLL_MIN_S * sample_rate,
            len: 0.0,
            recorded: 0,
            record_to: 0,
            t: 0,
            period: 0,
            seg_end: 0.0,
            segs_left: 0,
            seg_len: 0.0,
            now: Period { speed: 1.0, gain: 1.0 },
            voices: [IDLE; VOICES],
            live: Ramp::at(1.0),
            active: false,
            captures: 0,
            rng: Rng::new(1),
        }
    }

    pub fn reset(&mut self) {
        self.voices = [IDLE; VOICES];
        self.live = Ramp::at(1.0);
        self.active = false;
        self.captures = 0;
        self.recorded = 0;
        self.record_to = 0;
    }

    /// Samples kept before a slice: the most a capture may start in the
    /// past is what the ring holds beyond this
    pub fn edge(&self) -> usize {
        self.edge
    }

    /// Capturing or repeating
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Nothing sounds but the input
    pub fn is_idle(&self) -> bool {
        !self.active && self.live.is_settled() && self.voices.iter().all(|v| !v.on)
    }

    /// Start a slice of `len` samples that began `since` samples ago (0:
    /// now). `ring` holds the input up to and including the current
    /// sample; [`Slicer::process`] records the current sample.
    pub fn start(&mut self, ring: &DelayLine, since: usize, len: f64, reverse: bool) {
        // What plays fades out; record into the buffer no voice reads
        // (two presses within a fade: cut the older voices)
        self.release();
        let used = |b: usize| self.voices.iter().any(|v| v.on && v.buf == b);
        let other = 1 - self.cur;
        self.cur = if used(other) && !used(self.cur) { self.cur } else { other };
        let cur = self.cur;
        for v in self.voices.iter_mut().filter(|v| v.buf == cur) {
            v.on = false;
        }
        let len = len.clamp(1.0, (self.bufs[0].len() - 2 * self.edge - 8) as f64);
        let since = since.min(ring.max_delay().saturating_sub(self.edge + 1));
        let pre = self.edge + since;
        if let Some(buf) = self.bufs.get_mut(self.cur) {
            for (j, s) in buf.iter_mut().take(pre).enumerate() {
                *s = ring.read(pre - j);
            }
        }
        self.len = len;
        self.recorded = pre;
        self.record_to = (2 * self.edge + len.ceil() as usize + 4).min(self.bufs[0].len());
        self.t = since as u64;
        self.period = 0;
        self.seg_len = len;
        self.seg_end = len;
        self.segs_left = 0;
        self.now = Period { speed: if reverse { -1.0 } else { 1.0 }, gain: 1.0 };
        self.active = true;
        self.captures = self.captures.wrapping_add(1);
        self.rng = Rng::new(self.captures.wrapping_mul(0x9E37_79B9) ^ 0x5EED);
    }

    /// Back to the input
    pub fn release(&mut self) {
        if self.active {
            self.active = false;
            self.live.go(1.0, self.max_fade);
            for v in self.voices.iter_mut().filter(|v| v.on) {
                v.dying = true;
                v.env.go(0.0, self.max_fade);
            }
        }
    }

    /// Fade length for a segment played at `speed`
    fn fade_for(&self, seg_len: f64, speed: f64) -> usize {
        let f = (seg_len / (4.0 * speed.abs().max(1.0))) as usize;
        f.clamp(FADE_MIN.min(self.max_fade), self.max_fade)
    }

    /// Start a voice at segment boundary `b`, now being `t`
    fn spawn(&mut self, b: f64, t: f64) {
        let fade = self.fade_for(self.seg_len, self.now.speed);
        for v in self.voices.iter_mut().filter(|v| v.on && !v.dying) {
            v.dying = true;
            v.env.go(0.0, v.fade);
        }
        if self.now.gain <= 0.0 {
            return;
        }
        let late = (t - b) * self.now.speed;
        let pos = if self.now.speed >= 0.0 { late } else { self.len + late };
        self.add(Voice {
            on: true,
            dying: false,
            buf: self.cur,
            pos,
            speed: self.now.speed,
            gain: self.now.gain,
            env: Ramp::at(0.0),
            fade,
        });
    }

    /// Put `v` in a free slot (or the quietest one), fading in
    fn add(&mut self, mut v: Voice) {
        v.env.go(1.0, v.fade);
        let slot = self.voices.iter().position(|v| !v.on).unwrap_or_else(|| {
            let mut best = 0;
            for (i, o) in self.voices.iter().enumerate() {
                if o.env.value() < self.voices[best].env.value() {
                    best = i;
                }
            }
            best
        });
        if let Some(s) = self.voices.get_mut(slot) {
            *s = v;
        }
    }

    /// The segment boundary at `seg_end` is reached at `t`
    fn boundary(&mut self, t: f64, plan: &Plan, reverse: bool) {
        let b = self.seg_end;
        if self.segs_left > 0 {
            self.segs_left -= 1;
            self.seg_end += self.seg_len;
            self.spawn(b, t);
            return;
        }
        self.period += 1;
        if plan.repeats > 0 && self.period > plan.repeats {
            self.release();
            return;
        }
        let k = f64::from(self.period - 1);
        let mut speed = (k * f64::from(plan.step) / 12.0).exp2().clamp(SPEED_MIN, SPEED_MAX);
        let mut gain = db_to_gain(-plan.decay_db * k as f32);
        let mut backward = reverse;
        let mut div = 1u32;
        if plan.chaos > 0.0 && self.rng.uniform() < plan.chaos * CHAOS_EVENTS {
            match self.rng.next_u32() % 4 {
                0 => backward = !backward,
                1 => {
                    let up = self.rng.uniform() < 0.5;
                    speed = (if up { speed * 2.0 } else { speed * 0.5 }).clamp(SPEED_MIN, SPEED_MAX);
                }
                2 => {
                    if self.len / 4.0 >= self.roll_min && self.rng.uniform() < 0.5 {
                        div = 4;
                    } else if self.len / 2.0 >= self.roll_min {
                        div = 2;
                    }
                }
                _ => gain = 0.0,
            }
        }
        self.now = Period { speed: if backward { -speed } else { speed }, gain };
        self.seg_len = self.len / f64::from(div);
        self.segs_left = div - 1;
        // Period n spans [n·L, (n + 1)·L): no drift from rounding
        self.seg_end = f64::from(self.period) * self.len + self.seg_len;
        if self.period == 1 {
            let fade = self.fade_for(self.seg_len, speed);
            self.live.go(0.0, fade);
        }
        self.spawn(b, t);
    }

    /// One sample: record `x`; returns the gain of the input and the
    /// repeats. `reverse` applies to the next repeats.
    #[inline]
    pub fn process(&mut self, x: f32, plan: &Plan, reverse: bool) -> (f32, f32) {
        if self.recorded < self.record_to {
            if let Some(s) = self.bufs.get_mut(self.cur).and_then(|b| b.get_mut(self.recorded)) {
                *s = x;
            }
            self.recorded += 1;
        }
        let t = self.t as f64;
        while self.active && t >= self.seg_end {
            self.boundary(t, plan, reverse);
        }
        let live = self.live.next();
        let mut y = 0.0;
        let edge = self.edge as f64;
        for v in self.voices.iter_mut().filter(|v| v.on) {
            let e = v.env.next();
            if let Some(buf) = self.bufs.get(v.buf) {
                y += v.gain * e * read(buf, edge + v.pos);
            }
            v.pos += v.speed;
            if v.dying && v.env.is_settled() {
                v.on = false;
            }
        }
        // A fast repeat starts over within its period
        for i in 0..VOICES {
            let v = self.voices[i];
            let over = if v.speed > 0.0 { v.pos >= self.len } else { v.pos <= 0.0 };
            if v.on && !v.dying && over {
                self.voices[i].dying = true;
                self.voices[i].env.go(0.0, v.fade);
                let pos = if v.speed > 0.0 { v.pos - self.len } else { v.pos + self.len };
                self.add(Voice { pos, dying: false, env: Ramp::at(0.0), ..v });
            }
        }
        self.t += 1;
        (live, y)
    }

    /// For the grid: 0 the input, 1 the slice being captured, 2 a repeat
    pub fn source(&self) -> f32 {
        match (self.active, self.period) {
            (false, _) => 0.0,
            (true, 0) => 1.0,
            _ => 2.0,
        }
    }

    /// Speed and level of what plays now
    pub fn speed_level(&self) -> (f32, f32) {
        if self.active && self.period > 0 { (self.now.speed as f32, self.now.gain) } else { (1.0, 1.0) }
    }
}
