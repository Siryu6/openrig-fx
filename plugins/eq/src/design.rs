//! The bands of an equaliser, described once: the processor builds its
//! filters from them, and the `openrig:curve` descriptions a host draws
//! are generated from them, so the drawn curve and the sound cannot drift
//! apart.

use orx_dsp::filter::Shape;
use orx_plugin::view::{Curve, CurveShape, FilterShape, QUnit, Value};

/// A band of an equaliser
#[derive(Debug, Clone, Copy)]
pub struct BandDef {
    pub shape: FilterShape,
    /// Frequency, Hz: a port or fixed
    pub frequency: Value,
    /// Gain, dB, for bells and shelves
    pub gain: Option<Value>,
    /// Cookbook Q
    pub q: Value,
    /// Toggle port turning the band on (cut filters)
    pub enable: Option<&'static str>,
}

impl BandDef {
    /// The filter shape of the processor
    pub const fn svf_shape(&self) -> Shape {
        match self.shape {
            FilterShape::Lowpass => Shape::Lowpass,
            FilterShape::Highpass => Shape::Highpass,
            FilterShape::Bandpass => Shape::Bandpass,
            FilterShape::Notch => Shape::Notch,
            FilterShape::Allpass => Shape::Allpass,
            FilterShape::Peak => Shape::Peak,
            FilterShape::LowShelf => Shape::LowShelf,
            FilterShape::HighShelf => Shape::HighShelf,
        }
    }

    /// The `openrig:curve` of the band
    pub const fn curve(&self) -> Curve {
        Curve {
            shape: CurveShape::Fixed(self.shape),
            frequency: self.frequency,
            gain: self.gain,
            q: Some(self.q),
            q_unit: QUnit::Q,
            enable: self.enable,
        }
    }
}

/// The curves of `N` bands
pub const fn curves<const N: usize>(bands: &[BandDef; N]) -> [Curve; N] {
    let mut out = [bands[0].curve(); N];
    let mut i = 1;
    while i < N {
        out[i] = bands[i].curve();
        i += 1;
    }
    out
}
