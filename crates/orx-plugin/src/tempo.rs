//! Note values for parameters that follow the host tempo.
//!
//! One table for the whole suite, so every tempo-synced choice port has
//! the same values and labels (a rig or a host's tap tempo can rely on
//! value 4 meaning a quarter note everywhere). Value 0 is "Off": the
//! plugin uses its free-running control instead.
//!
//! [`CHOICES`] holds the values from Off to 1/16T, the choices of the
//! suite's sync ports. The shorter 1/32 and 1/32T come after them, for
//! plugins that cut slices rather than set a rate; they build their own
//! choice lists with [`choice`].

use crate::desc::Choice;

/// A note value: port value, label, length in quarter notes (0 for Off)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Division {
    pub value: f32,
    pub label: &'static str,
    pub quarters: f64,
}

const fn d(value: f32, label: &'static str, quarters: f64) -> Division {
    Division { value, label, quarters }
}

/// Every note value, in port value order
pub const DIVISIONS: [Division; 14] = [
    d(0.0, "Off", 0.0),
    d(1.0, "1/1", 4.0),
    d(2.0, "1/2", 2.0),
    d(3.0, "1/2.", 3.0),
    d(4.0, "1/4", 1.0),
    d(5.0, "1/4.", 1.5),
    d(6.0, "1/4T", 2.0 / 3.0),
    d(7.0, "1/8", 0.5),
    d(8.0, "1/8.", 0.75),
    d(9.0, "1/8T", 1.0 / 3.0),
    d(10.0, "1/16", 0.25),
    d(11.0, "1/16T", 1.0 / 6.0),
    d(12.0, "1/32", 0.125),
    d(13.0, "1/32T", 1.0 / 12.0),
];

/// The choice of the note value of port value `value` (an index of
/// [`DIVISIONS`])
pub const fn choice(value: usize) -> Choice {
    Choice::new(DIVISIONS[value].value, DIVISIONS[value].label)
}

/// The choices of a tempo division port: Off to 1/16T
pub const CHOICES: [Choice; 12] = {
    let mut out = [Choice::new(0.0, ""); 12];
    let mut i = 0;
    while i < out.len() {
        out[i] = choice(i);
        i += 1;
    }
    out
};

/// Length in seconds of the note value `value` at `bpm`; None when the
/// value is Off or unknown, or the tempo is not a positive number
pub fn seconds(value: i32, bpm: Option<f32>) -> Option<f64> {
    let quarters = usize::try_from(value).ok().and_then(|i| DIVISIONS.get(i)).map(|d| d.quarters)?;
    let bpm = f64::from(bpm?);
    (quarters > 0.0 && bpm.is_finite() && bpm > 0.0).then(|| quarters * 60.0 / bpm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_their_index() {
        for (i, d) in DIVISIONS.iter().enumerate() {
            assert_eq!(d.value, i as f32);
            assert_eq!(choice(i).label, d.label);
            assert_eq!(choice(i).value, d.value);
        }
        for (c, d) in CHOICES.iter().zip(&DIVISIONS) {
            assert_eq!(c.label, d.label);
        }
    }

    #[test]
    fn seconds_follow_the_tempo() {
        assert_eq!(seconds(4, Some(120.0)), Some(0.5));
        assert_eq!(seconds(8, Some(120.0)), Some(0.375));
        assert!((seconds(9, Some(90.0)).unwrap_or(0.0) - 2.0 / 9.0).abs() < 1e-12);
        assert_eq!(seconds(0, Some(120.0)), None);
        assert_eq!(seconds(4, None), None);
        assert_eq!(seconds(4, Some(0.0)), None);
        assert_eq!(seconds(4, Some(f32::NAN)), None);
        assert_eq!(seconds(12, Some(120.0)), Some(0.0625));
        assert!((seconds(13, Some(120.0)).unwrap_or(0.0) - 0.5 / 12.0).abs() < 1e-12);
        assert_eq!(seconds(14, Some(120.0)), None);
        assert_eq!(seconds(-1, Some(120.0)), None);
    }
}
