//! The time-domain pitch shifter: a delay line read at the pitch ratio,
//! kept in range by pitch-synchronous splices.
//!
//! The read head moves `r` input samples per output sample, so its delay
//! `d` drifts by `1 − r` per sample. The delay swings around a fixed
//! centre `D` (the reported latency): when it has drifted half a period
//! past `D`, the head jumps by one period of the signal towards the other
//! side, and the jump is crossfaded. Jumping by a whole period lands on
//! the same point of the waveform, so a periodic signal (a note, or a
//! chord, whose common period is longer) continues without a seam.
//!
//! - The period comes from the newest input: a normalised cross
//!   correlation on a copy decimated to about 12 kHz, whose shortest lag
//!   within 90 % of the best peak wins (the best may be a multiple of
//!   the period). With no clear pitch (a chord whose common period is
//!   out of reach), the most similar lag; with silence or noise, a
//!   period that keeps the head around the centre, so that the next
//!   note starts at the latency.
//! - At the splice, the lag is refined at the full rate where the head
//!   reads: the windows just behind the old and the new position are
//!   compared (behind, so the search needs no audio that has not come
//!   in yet), the best of a few lags around the period wins, and a
//!   parabola through its neighbours makes it fractional (a whole-sample
//!   jump on a period of 145.6 samples would raise the pitch by cents).
//! - A downward splice needs one period of delay to jump back by: on a
//!   period longer than twice the centre, the head swings between the
//!   newest input and one period behind it, so the delay is half a
//!   period on average instead of the centre. In Quality (14 ms) that
//!   only happens below 36 Hz; in Tight (6 ms) below about 85 Hz, where
//!   the latency follows the note (12 ms on E1).
//!
//! The read head interpolates with a four-point cubic (exact at whole
//! samples). No filterbank and no resynthesis: the output is always
//! input audio, resampled and spliced.

/// Centre delay and longest crossfade of a mode, ms
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mode {
    pub delay_ms: f64,
    pub crossfade_ms: f64,
}

/// Low latency: 6 ms down to about 85 Hz, half a period below
pub const TIGHT: Mode = Mode { delay_ms: 6.0, crossfade_ms: 6.0 };
/// A constant 14 ms down to 36 Hz, longer crossfades
pub const QUALITY: Mode = Mode { delay_ms: 14.0, crossfade_ms: 12.0 };

/// Shortest and longest period the detector looks for, ms
pub const MIN_PERIOD_MS: f64 = 2.0;
pub const MAX_PERIOD_MS: f64 = 33.0;
/// Most and least pitch ratio (+12 and −24 semitones)
pub const MAX_RATIO: f64 = 2.0;
pub const MIN_RATIO: f64 = 0.25;

/// Samples the interpolation needs after the read position
const MARGIN: f64 = 3.0;
/// Lags tried on each side of the period at the splice
const REFINE: usize = 6;
/// Longest refinement window, samples
const REFINE_WINDOW: usize = 256;
/// Rate of the detector's decimated copy, Hz (about)
const DETECT_RATE: f64 = 12_000.0;
/// Longest lag the detector scores, decimated samples
const MAX_DEC_LAG: usize = 400;
/// Detector runs every this many input samples
const DETECT_EVERY: usize = 256;
/// A correlation below this is not a pitch: the last period holds
const VOICED: f32 = 0.5;
/// The shortest lag within this share of the best correlation wins
const NEAR_BEST: f32 = 0.9;
/// Below a pitch, a lag still worth splicing on rather than the last one
const RESEMBLES: f32 = 0.1;

/// Power-of-two ring buffer
struct Ring {
    buf: Vec<f32>,
    mask: usize,
}

impl Ring {
    fn new(len: usize) -> Self {
        let n = len.next_power_of_two();
        Self { buf: vec![0.0; n], mask: n - 1 }
    }

    #[inline]
    fn at(&self, i: usize) -> f32 {
        self.buf[i & self.mask]
    }

    #[inline]
    fn set(&mut self, i: usize, v: f32) {
        self.buf[i & self.mask] = v;
    }

    fn clear(&mut self) {
        self.buf.fill(0.0);
    }
}

/// Normalised correlation of the `w` samples ending at `a` with those
/// ending at `b`
#[inline]
fn ncc(ring: &Ring, a: usize, b: usize, w: usize) -> f32 {
    let (mut ab, mut aa, mut bb) = (0.0f32, 0.0f32, 0.0f32);
    for k in 0..w {
        let (x, y) = (ring.at(a - k), ring.at(b - k));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    let norm = (aa * bb).sqrt();
    if norm > 1e-12 { ab / norm } else { 0.0 }
}

/// Dot product in eight lanes (vectorised; the order of the additions
/// is fixed, so the result does not depend on the target)
#[inline]
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut lanes = [0.0f32; 8];
    let ((ca, ra), (cb, rb)) = (a.as_chunks::<8>(), b.as_chunks::<8>());
    for (x, y) in ca.iter().zip(cb) {
        for k in 0..8 {
            lanes[k] += x[k] * y[k];
        }
    }
    let mut tail = 0.0f32;
    for (x, y) in ra.iter().zip(rb) {
        tail += x * y;
    }
    ((lanes[0] + lanes[4]) + (lanes[1] + lanes[5])) + ((lanes[2] + lanes[6]) + (lanes[3] + lanes[7])) + tail
}

/// A crossfade from an old read head to the current one
#[derive(Debug, Clone, Copy)]
struct Fade {
    /// Delay of the old head
    d: f64,
    /// Old delay of the dry signal, on a mode change
    dry: Option<usize>,
    done: usize,
    len: usize,
}

/// The pitch shifter, mono
pub struct Shifter {
    rate: f64,
    input: Ring,
    dec: Ring,
    /// The detector's straight copy of the decimated input and its
    /// running energy
    lin: Vec<f32>,
    energy: Vec<f64>,
    /// Decimation factor of the detector's copy
    dec_factor: usize,
    /// Samples written; the newest is `written − 1`
    written: usize,
    dec_written: usize,
    dec_acc: f32,
    /// Delay of the read head, samples
    d: f64,
    fade: Option<Fade>,
    /// Mode waiting for the current crossfade to end
    pending: Option<Mode>,
    /// Centre delay (the latency), samples
    centre: usize,
    /// Longest crossfade, samples
    crossfade: usize,
    /// Longest period the detector finds, samples
    cap: usize,
    min_period: usize,
    /// Period of the newest input, samples
    period: usize,
    since_detect: usize,
}

fn samples(ms: f64, rate: f64) -> usize {
    (ms * rate / 1000.0).round() as usize
}

impl Shifter {
    /// Allocates the buffers for any mode at `rate`
    pub fn new(rate: f64, mode: Mode) -> Self {
        let dec_factor = ((rate / DETECT_RATE).round() as usize).max(1);
        let longest = samples(QUALITY.delay_ms + MAX_PERIOD_MS + QUALITY.crossfade_ms, rate) + REFINE_WINDOW + 64;
        let mut s = Self {
            rate,
            input: Ring::new(2 * longest),
            dec: Ring::new(2 * MAX_DEC_LAG + 16),
            lin: vec![0.0; 2 * MAX_DEC_LAG + 2],
            energy: vec![0.0; 2 * MAX_DEC_LAG + 3],
            dec_factor,
            written: 0,
            dec_written: 0,
            dec_acc: 0.0,
            d: 0.0,
            fade: None,
            pending: None,
            centre: 0,
            crossfade: 0,
            min_period: samples(MIN_PERIOD_MS, rate).max(2 * dec_factor),
            cap: samples(MAX_PERIOD_MS, rate).min(MAX_DEC_LAG * dec_factor),
            period: 0,
            since_detect: 0,
        };
        s.apply_mode(mode);
        s.reset();
        s
    }

    /// Most latency of any mode at `rate`
    pub fn max_latency(rate: f64) -> usize {
        samples(QUALITY.delay_ms.max(TIGHT.delay_ms), rate)
    }

    fn apply_mode(&mut self, mode: Mode) {
        self.centre = samples(mode.delay_ms, self.rate);
        self.crossfade = samples(mode.crossfade_ms, self.rate).max(1);
    }

    /// Latency of the shifted signal: its delay swings around this
    pub fn latency(&self) -> usize {
        self.centre
    }

    /// Longest period the detector finds, samples
    pub fn longest_period(&self) -> usize {
        self.cap
    }

    /// Period found in the newest input, samples
    pub fn period(&self) -> usize {
        self.period
    }

    /// Period used while there is no pitch: the longest whose splices
    /// keep the delay around the centre
    fn resting_period(&self) -> usize {
        let room = 2.0 * (self.centre as f64 - MARGIN) - (REFINE + 4) as f64;
        (room.max(0.0) as usize).clamp(self.min_period, self.cap)
    }

    /// Change the mode at once, clearing the audio (fresh start)
    pub fn set_mode_now(&mut self, mode: Mode) {
        self.apply_mode(mode);
        self.reset();
    }

    /// Change the mode while playing: the read head and the dry signal
    /// crossfade to the new centre
    pub fn set_mode(&mut self, mode: Mode) {
        self.pending = Some(mode);
    }

    pub fn reset(&mut self) {
        self.input.clear();
        self.dec.clear();
        self.written = 0;
        self.dec_written = 0;
        self.dec_acc = 0.0;
        self.d = self.centre as f64;
        self.fade = None;
        if let Some(mode) = self.pending.take() {
            self.apply_mode(mode);
            self.d = self.centre as f64;
        }
        self.period = self.resting_period();
        self.since_detect = 0;
    }

    /// Index of the newest input, offset by a whole ring so that reads
    /// behind it never wrap below zero
    #[inline]
    fn newest(&self) -> usize {
        self.written + self.input.buf.len() - 1
    }

    /// The input delayed by exactly `delay` samples
    #[inline]
    fn delayed(&self, delay: usize) -> f32 {
        self.input.at(self.newest() - delay)
    }

    /// Four-point cubic (Catmull-Rom) read `d` samples behind the newest
    /// input; exact at whole samples
    #[inline]
    fn read(&self, d: f64) -> f32 {
        let back = d.floor();
        let i = self.newest() - back as usize;
        if d == back {
            return self.input.at(i);
        }
        // Between i − 1 (older) and i, `t` from the older one
        let t = (back + 1.0 - d) as f32;
        let (x0, x1, x2, x3) = (self.input.at(i - 2), self.input.at(i - 1), self.input.at(i), self.input.at(i + 1));
        let c1 = 0.5 * (x2 - x0);
        let c2 = x0 - 2.5 * x1 + 2.0 * x2 - 0.5 * x3;
        let c3 = 0.5 * (x3 - x0) + 1.5 * (x1 - x2);
        ((c3 * t + c2) * t + c1) * t + x1
    }

    fn detect(&mut self) {
        let newest = self.dec_written + self.dec.buf.len() - 1;
        let lo = self.min_period / self.dec_factor;
        let hi = (self.cap / self.dec_factor).min(MAX_DEC_LAG);
        // One longest period of signal against each lag, from a straight
        // copy (oldest first) and its running energy
        let w = hi;
        let n = w + hi + 2;
        let (lin, energy) = (&mut self.lin[..n], &mut self.energy[..=n]);
        let mut sum = 0.0f64;
        energy[0] = 0.0;
        for (j, (v, e)) in lin.iter_mut().zip(energy[1..].iter_mut()).enumerate() {
            *v = self.dec.at(newest + 1 + j - n);
            sum += f64::from(*v) * f64::from(*v);
            *e = sum;
        }
        let newest_window = &lin[n - w..];
        let aa = energy[n] - energy[n - w];
        let mut scores = [0.0f32; MAX_DEC_LAG + 2];
        for (lag, s) in scores.iter_mut().enumerate().take(hi + 2).skip(lo - 1) {
            let bb = energy[n - lag] - energy[n - lag - w];
            let norm = (aa * bb).sqrt();
            *s = if norm > 1e-12 {
                (f64::from(dot(newest_window, &lin[n - w - lag..n - lag])) / norm) as f32
            } else {
                0.0
            };
        }
        let (mut argmax, mut best) = (hi, f32::MIN);
        for (l, s) in scores.iter().enumerate().take(hi + 1).skip(lo) {
            if *s > best {
                (argmax, best) = (l, *s);
            }
        }
        let lag = if best >= VOICED {
            // A pitch: its period is the shortest lag nearly as good as
            // the best (the best may be a multiple of it)
            (lo..=hi)
                .find(|&l| {
                    let s = scores[l];
                    s >= NEAR_BEST * best && s >= scores[l - 1] && s >= scores[l + 1]
                })
                .unwrap_or(argmax)
        } else if best >= RESEMBLES {
            // No period within reach (a low chord in Tight): the lag that
            // resembles it most makes the least audible seam
            argmax
        } else {
            // Silence or noise: the longest period that swings around the
            // centre, so that the next note starts near the latency
            self.period = self.resting_period();
            return;
        };
        self.period = (lag * self.dec_factor).clamp(self.min_period, self.cap);
    }

    /// The lag near the period that best continues the waveform where the
    /// head reads, towards newer input (`newer`) or older, at most `most`;
    /// fractional, from a parabola through the best correlation and its
    /// neighbours (a period is rarely a whole number of samples, and a
    /// whole-sample jump would shift the pitch)
    fn splice_lag(&self, newer: bool, most: f64) -> f64 {
        let at = self.newest() - self.d.floor() as usize;
        let top = (most.floor().max(1.0) as usize).saturating_sub(1).max(1);
        let hi = (self.period + REFINE).min(top);
        let lo = self.period.saturating_sub(REFINE).max(self.min_period).min(hi);
        let w = self.period.min(REFINE_WINDOW);
        let score = |lag: usize| ncc(&self.input, at, if newer { at + lag } else { at - lag }, w);
        let mut best = (hi, f32::MIN);
        for lag in lo..=hi {
            let s = score(lag);
            if s > best.1 {
                best = (lag, s);
            }
        }
        let (lag, s0) = best;
        let (sm, sp) = (score(lag - 1), score(lag + 1));
        let curve = sm - 2.0 * s0 + sp;
        let offset = if curve < 0.0 { (0.5 * (sm - sp) / curve).clamp(-0.5, 0.5) } else { 0.0 };
        (lag as f64 + f64::from(offset)).min(most)
    }

    fn start_fade(&mut self, new_d: f64, len: usize, dry: Option<usize>) {
        self.fade = Some(Fade { d: self.d, dry, done: 0, len: len.max(1) });
        self.d = new_d;
    }

    /// One sample in at pitch ratio `ratio`; the shifted sample and the
    /// dry one, both delayed by the latency
    #[inline]
    pub fn process(&mut self, x: f32, ratio: f64) -> (f32, f32) {
        let r = ratio.clamp(MIN_RATIO, MAX_RATIO);
        self.input.set(self.written, x);
        self.written += 1;
        self.dec_acc += x;
        if self.written.is_multiple_of(self.dec_factor) {
            self.dec.set(self.dec_written, self.dec_acc / self.dec_factor as f32);
            self.dec_written += 1;
            self.dec_acc = 0.0;
        }
        self.since_detect += 1;
        if self.since_detect >= DETECT_EVERY {
            self.since_detect = 0;
            self.detect();
        }

        let drift = 1.0 - r;
        self.d += drift;
        if let Some(f) = &mut self.fade {
            f.d += drift;
        }
        if self.fade.is_none() {
            self.splice(r);
        }
        // Never read past the newest input or beyond the buffer
        let limit = (self.input.buf.len() - REFINE_WINDOW - samples(MAX_PERIOD_MS, self.rate) - 8) as f64;
        self.d = self.d.clamp(MARGIN, limit);
        let mut y = self.read(self.d);
        let mut dry = self.delayed(self.centre);
        if let Some(f) = &mut self.fade {
            f.done += 1;
            let w = f.done as f32 / f.len as f32;
            let g = 0.5 - 0.5 * (core::f32::consts::PI * w).cos();
            f.d = f.d.clamp(MARGIN, limit);
            let (old_d, old_dry, done) = (f.d, f.dry, f.done >= f.len);
            y = g * y + (1.0 - g) * self.read(old_d);
            if let Some(o) = old_dry {
                dry = g * dry + (1.0 - g) * self.delayed(o);
            }
            if done {
                self.fade = None;
            }
        }
        (y, dry)
    }

    /// Start a splice, a mode change or a return to the centre when due
    fn splice(&mut self, r: f64) {
        let c = self.centre as f64;
        if let Some(mode) = self.pending.take() {
            let old = self.centre;
            self.apply_mode(mode);
            let to = self.d - old as f64 + self.centre as f64;
            self.start_fade(to.max(MARGIN), self.crossfade, Some(old));
            return;
        }
        let half = 0.5 * self.period as f64;
        if r < 1.0 && self.d >= (c + half).max(MARGIN + 2.0 + (self.period + REFINE) as f64) {
            // Fell behind: jump one period towards the newest input (on a
            // period longer than twice the centre, from one period and
            // the refinement's reach behind it: the delay grows to about
            // half a period on average)
            let lag = self.splice_lag(true, self.d - MARGIN);
            self.start_fade(self.d - lag, (lag as usize).min(self.crossfade), None);
        } else if r > 1.0 {
            // Running ahead: jump one period back, early enough that the
            // old head stays behind the newest input for the whole fade
            let len = self.period.min(self.crossfade) as f64;
            if self.d <= (c - half).max(MARGIN + (r - 1.0) * len) {
                let lag = self.splice_lag(false, (self.cap + REFINE) as f64);
                let room = ((self.d - MARGIN) / (r - 1.0)).max(1.0) as usize;
                self.start_fade(self.d + lag, (lag as usize).min(self.crossfade).min(room), None);
            }
        } else if r == 1.0 && self.d != c {
            // No shift: back to the plain delay
            self.start_fade(c, self.crossfade, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_shift_is_a_pure_delay() {
        let mut s = Shifter::new(48_000.0, TIGHT);
        let input: Vec<f32> = (0..4000).map(|i| ((i * 7919) % 1000) as f32 / 1000.0 - 0.5).collect();
        let out: Vec<(f32, f32)> = input.iter().map(|x| s.process(*x, 1.0)).collect();
        let lat = s.latency();
        for (o, x) in out[lat..].iter().zip(&input) {
            assert_eq!(*o, (*x, *x));
        }
    }

    #[test]
    fn the_cubic_read_is_exact_on_a_line() {
        let mut s = Shifter::new(48_000.0, TIGHT);
        for i in 0..100 {
            s.process(i as f32 * 0.01, 1.0);
        }
        // The newest sample is 0.99; 10.25 samples back reads 0.8875
        assert!((s.read(10.25) - 0.8875).abs() < 1e-5, "{}", s.read(10.25));
        assert_eq!(s.read(10.0), s.delayed(10));
    }

    #[test]
    fn the_detector_finds_a_low_e_in_both_modes() {
        let rate = 48_000.0;
        let f = 41.2;
        let tone = |i: usize| {
            let t = i as f64 / rate;
            let w = core::f64::consts::TAU * f * t;
            (w.sin() + 0.5 * (2.0 * w).sin() + 0.3 * (3.0 * w).sin()) as f32 * 0.3
        };
        let mut q = Shifter::new(rate, QUALITY);
        let mut t = Shifter::new(rate, TIGHT);
        for i in 0..9600 {
            q.process(tone(i), 1.0);
            t.process(tone(i), 1.0);
        }
        let expected = rate / f;
        assert!((q.period() as f64 - expected).abs() <= 4.0, "{} vs {expected}", q.period());
        assert!((t.period() as f64 - expected).abs() <= 4.0, "{} vs {expected}", t.period());
    }
}
