//! The bar clock: where the beat is, from the host transport or free
//! running at the tempo.
//!
//! A host that rolls sends `time:Position` with the beat in the bar, a
//! different one in each update. Some hosts (OpenRig among them) send a
//! tempo with a bar position that never moves: only the tempo is real.
//! The clock therefore locks to the host bar only when the position
//! changes from one update to the next ([`Clock::is_tracking`]), and
//! runs on its own at the tempo otherwise.

use crate::info::{BPM_MAX, BPM_MIN};
use orx_plugin::Position;

/// Position in the bar, from the host or free running
#[derive(Debug, Clone)]
pub struct Clock {
    sample_rate: f64,
    /// Position in the bar, quarter notes, in `[0, bar)`
    pos: f64,
    /// Bar length, quarter notes
    bar: f64,
    /// Quarter notes per sample
    step: f64,
    /// Tempo the host sent
    host_bpm: Option<f64>,
    /// Bar position of the last rolling update
    last_bar_beat: Option<f32>,
    tracking: bool,
}

impl Clock {
    pub fn new(sample_rate: f64) -> Self {
        let mut c =
            Self { sample_rate, pos: 0.0, bar: 4.0, step: 0.0, host_bpm: None, last_bar_beat: None, tracking: false };
        c.set_tempo(120.0);
        c
    }

    pub fn reset(&mut self) {
        self.pos = 0.0;
        self.bar = 4.0;
        self.host_bpm = None;
        self.last_bar_beat = None;
        self.tracking = false;
    }

    /// A transport update
    pub fn position(&mut self, p: &Position) {
        if let Some(bpm) = p.bpm.filter(|b| b.is_finite() && *b > 0.0) {
            self.host_bpm = Some(f64::from(bpm));
        }
        let rolling = p.speed.is_none_or(|s| s > 0.0);
        match p.bar_beat.filter(|b| b.is_finite() && *b >= 0.0) {
            Some(beat) if rolling => {
                // The same position twice: a host without a real bar
                let moved = self.last_bar_beat != Some(beat);
                self.last_bar_beat = Some(beat);
                self.tracking = moved;
                if moved {
                    let unit = f64::from(p.beat_unit.filter(|u| (1..=64).contains(u)).unwrap_or(4));
                    let quarter = 4.0 / unit;
                    let beats = p.beats_per_bar.filter(|n| n.is_finite() && *n > 0.0).map(f64::from);
                    self.bar = beats.map_or(4.0, |n| (n * quarter).clamp(1.0, 16.0));
                    self.pos = (f64::from(beat) * quarter).rem_euclid(self.bar);
                }
            }
            _ => {
                self.tracking = false;
                self.last_bar_beat = None;
            }
        }
    }

    /// Follow the host tempo, or `free` (the Tempo control) without one
    pub fn set_tempo(&mut self, free: f32) {
        let bpm = self.host_bpm.unwrap_or(f64::from(free)).clamp(BPM_MIN, BPM_MAX);
        self.step = bpm / 60.0 / self.sample_rate;
    }

    /// The tempo in use, BPM
    pub fn bpm(&self) -> f64 {
        self.step * 60.0 * self.sample_rate
    }

    /// Samples in `quarters` quarter notes at the tempo
    pub fn samples(&self, quarters: f64) -> f64 {
        quarters / self.step
    }

    /// The bar comes from a rolling host
    pub fn is_tracking(&self) -> bool {
        self.tracking
    }

    /// Make now the downbeat (free running only)
    pub fn anchor(&mut self) {
        self.pos = 0.0;
    }

    /// Position in the bar, quarter notes
    pub fn pos(&self) -> f64 {
        self.pos
    }

    /// Bar length, quarter notes
    pub fn bar(&self) -> f64 {
        self.bar
    }

    /// Samples since the last line of a grid of `q` quarter notes from the
    /// downbeat
    pub fn since_line(&self, q: f64) -> f64 {
        (self.pos - (self.pos / q).floor() * q) / self.step
    }

    /// Samples to the next line of a grid of `q` quarter notes from the
    /// downbeat (the downbeat ends a bar the grid does not divide)
    pub fn to_line(&self, q: f64) -> f64 {
        let next = ((self.pos / q).floor() + 1.0) * q;
        (next.min(self.bar) - self.pos) / self.step
    }

    /// One sample on
    #[inline]
    pub fn advance(&mut self) {
        self.pos += self.step;
        if self.pos >= self.bar {
            self.pos -= self.bar;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(bar_beat: f32) -> Position {
        Position {
            bpm: Some(120.0),
            speed: Some(1.0),
            bar_beat: Some(bar_beat),
            beats_per_bar: Some(4.0),
            beat_unit: Some(4),
        }
    }

    #[test]
    fn locks_only_to_a_moving_bar() {
        let mut c = Clock::new(48_000.0);
        c.set_tempo(90.0);
        assert_eq!(c.bpm(), 90.0);
        c.position(&at(1.5));
        c.set_tempo(90.0);
        assert!(c.is_tracking());
        assert_eq!(c.pos(), 1.5);
        assert!((c.bpm() - 120.0).abs() < 1e-9);
        // A host repeating its position (no real bar): free running
        c.position(&at(1.5));
        assert!(!c.is_tracking());
        c.position(&at(2.0));
        assert!(c.is_tracking());
        // Stopped transport
        c.position(&Position { speed: Some(0.0), ..at(3.0) });
        assert!(!c.is_tracking());
        assert_eq!(c.pos(), 2.0);
        // 6/8: a bar of three quarters
        c.position(&Position { beats_per_bar: Some(6.0), beat_unit: Some(8), ..at(5.0) });
        assert_eq!(c.bar(), 3.0);
        assert_eq!(c.pos(), 2.5);
    }

    #[test]
    fn grid_lines() {
        let mut c = Clock::new(48_000.0);
        c.set_tempo(120.0);
        // 24000 samples per quarter
        assert_eq!(c.samples(1.0), 24_000.0);
        for _ in 0..30_000 {
            c.advance();
        }
        assert!((c.since_line(1.0) - 6_000.0).abs() < 1e-6);
        assert!((c.to_line(1.0) - 18_000.0).abs() < 1e-6);
        assert!((c.to_line(0.5) - 6_000.0).abs() < 1e-6);
        // Wraps at the bar
        for _ in 0..66_000 {
            c.advance();
        }
        assert!(c.pos() < 1e-6 || c.pos() > 3.999_999);
    }
}
