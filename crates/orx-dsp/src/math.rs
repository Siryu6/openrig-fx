//! Small numeric helpers used in the audio path.

/// 2π
pub const TAU: f32 = core::f32::consts::TAU;

/// Linear gain of a level in dB
#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    // 10^(db/20) = e^(db · ln10/20)
    (db * (core::f32::consts::LN_10 / 20.0)).exp()
}

/// Level in dB of a linear gain (floored at -240 dB)
///
/// Written with the natural logarithm: glibc 2.43 gave `log10f` a new
/// symbol version, so a build on a recent system would not load on the
/// OpenRig image, while `logf` is versioned 2.27.
#[inline]
pub fn gain_to_db(gain: f32) -> f32 {
    // 20·log10(g) = ln(g) · 20/ln10
    gain.abs().max(1e-12).ln() * (20.0 / core::f32::consts::LN_10)
}

/// `sin(2π·x)`, `x` in turns, any finite value. Max error about 1e-7:
/// range reduction to a quarter turn, then an odd polynomial (Taylor to the
/// 13th power, which is below f32 resolution on ±π/2).
#[inline]
pub fn sin_turns(x: f32) -> f32 {
    let mut r = x - x.round(); // [-0.5, 0.5]
    if r > 0.25 {
        r = 0.5 - r;
    } else if r < -0.25 {
        r = -0.5 - r;
    }
    let t = r * TAU; // [-π/2, π/2]
    let t2 = t * t;
    let p = 1.0 / 6227020800.0;
    let p = -1.0 / 39916800.0 + t2 * p;
    let p = 1.0 / 362880.0 + t2 * p;
    let p = -1.0 / 5040.0 + t2 * p;
    let p = 1.0 / 120.0 + t2 * p;
    let p = -1.0 / 6.0 + t2 * p;
    t + t * t2 * p
}

/// `cos(2π·x)`, `x` in turns
#[inline]
pub fn cos_turns(x: f32) -> f32 {
    sin_turns(x + 0.25)
}

/// Audio-taper (logarithmic) pot: the share of its resistance at
/// rotation `t` (0 to 1), an exponential through 0 and 1 with about 18 %
/// at the middle
#[inline]
pub fn audio_taper(t: f32) -> f32 {
    const B: f32 = 20.0;
    (B.powf(t) - 1.0) / (B - 1.0)
}

/// Zero for NaN and infinities, the value otherwise
#[inline]
pub fn finite_or_zero(x: f32) -> f32 {
    if x.is_finite() { x } else { 0.0 }
}

/// Zero for values too small to matter (below about -600 dB), so that
/// recursive state decays to an exact zero instead of a subnormal
#[inline]
pub fn flush_tiny(x: f32) -> f32 {
    if x.abs() < 1e-30 { 0.0 } else { x }
}

/// Clamp that also maps NaN to `lo`
#[inline]
pub fn clamp_finite(x: f32, lo: f32, hi: f32) -> f32 {
    if x >= lo { if x <= hi { x } else { hi } } else { lo }
}

/// Linear interpolation between `a` and `b`
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_taper_spans_the_pot() {
        assert_eq!(audio_taper(0.0), 0.0);
        assert!((audio_taper(1.0) - 1.0).abs() < 1e-6);
        assert!((audio_taper(0.5) - 0.18).abs() < 0.01);
    }

    #[test]
    fn sine_is_accurate() {
        let mut worst = 0.0f64;
        for i in -40_000..=40_000 {
            let x = i as f32 * 0.000_137;
            let exact = (x as f64 * core::f64::consts::TAU).sin();
            worst = worst.max((sin_turns(x) as f64 - exact).abs());
        }
        assert!(worst < 4e-7, "max error {worst}");
        assert!((cos_turns(0.0) - 1.0).abs() < 1e-7);
    }

    #[test]
    fn decibels_round_trip() {
        for db in [-60.0f32, -6.0, 0.0, 3.0, 12.0] {
            assert!((gain_to_db(db_to_gain(db)) - db).abs() < 1e-4);
        }
        assert!((db_to_gain(-6.0206) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn clamp_maps_nan_low() {
        assert_eq!(clamp_finite(f32::NAN, -1.0, 1.0), -1.0);
        assert_eq!(clamp_finite(5.0, -1.0, 1.0), 1.0);
        assert_eq!(finite_or_zero(f32::INFINITY), 0.0);
    }
}
