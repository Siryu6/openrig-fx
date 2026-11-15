//! The equaliser: a chain of state variable filters built from the band
//! descriptions.

use crate::design::BandDef;
use crate::info::{GRAPHIC, GRAPHIC_BANDS, PARAMETRIC, PARAMETRIC_BANDS};
use orx_dsp::Smoother;
use orx_dsp::filter::svf::{Svf, SvfCoeffs};
use orx_dsp::math::db_to_gain;
use orx_dsp::xfade::Fade;
use orx_plugin::view::Value;
use orx_plugin::{Config, Context, Param, Plugin, PluginInfo, PortKind};
use std::marker::PhantomData;

/// Cut filters fade in and out over this time
const SWITCH_MS: f64 = 20.0;
/// Most bands of an equaliser
const MAX_BANDS: usize = 8;

/// The description of one equaliser
pub trait Design: Send + 'static {
    const INFO: &'static PluginInfo;
    const BANDS: &'static [BandDef];
}

/// OR EQ's bands
pub struct ParametricDesign;
impl Design for ParametricDesign {
    const INFO: &'static PluginInfo = &PARAMETRIC;
    const BANDS: &'static [BandDef] = &PARAMETRIC_BANDS;
}

/// OR Graphic EQ's bands
pub struct GraphicDesign;
impl Design for GraphicDesign {
    const INFO: &'static PluginInfo = &GRAPHIC;
    const BANDS: &'static [BandDef] = &GRAPHIC_BANDS;
}

/// A value of a band: a smoothed port, or fixed
#[derive(Clone)]
enum Control {
    Port(usize, Smoother),
    Fixed(f32),
}

impl Control {
    fn new(info: &PluginInfo, value: Value, sample_rate: f64) -> Self {
        match value {
            Value::Fixed(v) => Control::Fixed(v),
            Value::Port(symbol) => {
                let index = info.port_index(symbol).unwrap_or(usize::MAX);
                let p = match info.ports.get(index).map(|p| p.kind) {
                    Some(PortKind::Param(p)) => p,
                    _ => Param::float(0.0, 0.0, 1.0),
                };
                Control::Port(index, Smoother::new(p.smoothing, sample_rate as f32, p.default))
            }
        }
    }

    /// Follow the port; true when its value changed
    fn set(&mut self, ctx: &Context<'_>, snap: bool) -> bool {
        let Control::Port(index, s) = self else { return false };
        let v = ctx.params.get(*index);
        if snap {
            s.reset(v);
        } else if v != s.target() {
            s.set_target(v);
        } else {
            return false;
        }
        true
    }

    fn is_settled(&self) -> bool {
        match self {
            Control::Port(_, s) => s.is_settled(),
            Control::Fixed(_) => true,
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        match self {
            Control::Port(_, s) => s.next(),
            Control::Fixed(v) => *v,
        }
    }
}

struct Band {
    def: BandDef,
    frequency: Control,
    gain: Control,
    q: Control,
    /// The enable toggle's port and its fade
    enable: Option<(usize, Fade)>,
    svf: Svf,
    /// A control changed since the coefficients were set
    stale: bool,
}

impl Band {
    /// The coefficients have to follow the controls this sample
    fn moving(&self) -> bool {
        self.stale || !(self.frequency.is_settled() && self.gain.is_settled() && self.q.is_settled())
    }

    /// Advance the controls one sample and set the coefficients
    #[inline]
    fn update(&mut self, sample_rate: f64) {
        let f = f64::from(self.frequency.next());
        let g = f64::from(self.gain.next());
        let q = f64::from(self.q.next());
        self.svf.set(SvfCoeffs::new(self.def.svf_shape(), sample_rate, f, q, g));
        self.stale = false;
    }
}

/// An equaliser of design `D`
pub struct Equalizer<D: Design> {
    sample_rate: f64,
    bands: [Option<Band>; MAX_BANDS],
    level: Smoother,
    level_port: usize,
    fresh: bool,
    design: PhantomData<D>,
}

impl<D: Design> Plugin for Equalizer<D> {
    const INFO: &'static PluginInfo = D::INFO;

    fn new(config: &Config) -> Self {
        let info = D::INFO;
        let sr = config.sample_rate;
        let mut bands: [Option<Band>; MAX_BANDS] = Default::default();
        for (slot, def) in bands.iter_mut().zip(D::BANDS) {
            let enable = def.enable.and_then(|s| info.port_index(s)).map(|i| {
                let fade = Fade::new((SWITCH_MS * 0.001 * sr) as usize, false);
                (i, fade)
            });
            let mut band = Band {
                def: *def,
                frequency: Control::new(info, def.frequency, sr),
                gain: Control::new(info, def.gain.unwrap_or(Value::Fixed(0.0)), sr),
                q: Control::new(info, def.q, sr),
                enable,
                svf: Svf::new(SvfCoeffs::lowpass(sr, 1000.0)),
                stale: true,
            };
            band.update(sr);
            *slot = Some(band);
        }
        let level_port = info.port_index("level").unwrap_or(usize::MAX);
        let level = match info.ports.get(level_port).map(|p| p.kind) {
            Some(PortKind::Param(p)) => Smoother::new(p.smoothing, sr as f32, p.default),
            _ => Smoother::new(orx_dsp::Smoothing::None, sr as f32, 0.0),
        };
        Self { sample_rate: sr, bands, level, level_port, fresh: true, design: PhantomData }
    }

    fn reset(&mut self) {
        for b in self.bands.iter_mut().flatten() {
            b.svf.reset();
        }
        self.fresh = true;
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        let fresh = self.fresh;
        self.fresh = false;
        let sr = self.sample_rate;
        for b in self.bands.iter_mut().flatten() {
            // Every control is set (no short circuit)
            let changed = [b.frequency.set(ctx, fresh), b.gain.set(ctx, fresh), b.q.set(ctx, fresh)];
            b.stale |= changed.contains(&true);
            if let Some((port, fade)) = &mut b.enable {
                fade.set(ctx.params.on(*port));
                if fresh {
                    fade.finish();
                }
            }
            if fresh {
                b.update(sr);
            }
        }
        let level = db_to_gain(ctx.params.get(self.level_port));
        if fresh {
            self.level.reset(level);
        } else {
            self.level.set_target(level);
        }

        let (Some(input), Some(output)) = (ctx.inputs.first(), ctx.outputs.first_mut()) else {
            return;
        };
        for (x, y) in input.iter().zip(output.iter_mut()).take(ctx.frames) {
            let mut v = *x;
            for b in self.bands.iter_mut().flatten() {
                // Coefficients follow the smoothed controls sample by
                // sample while they move: no zipper noise
                if b.moving() {
                    b.update(sr);
                }
                v = match &mut b.enable {
                    // A cut filter switched off costs nothing and starts
                    // from rest when it fades back in
                    Some((_, fade)) if fade.is_off() => {
                        b.svf.reset();
                        v
                    }
                    Some((_, fade)) => {
                        let mix = fade.next();
                        v + (b.svf.process(v) - v) * mix
                    }
                    None => b.svf.process(v),
                };
            }
            *y = v * self.level.next();
        }
    }
}
