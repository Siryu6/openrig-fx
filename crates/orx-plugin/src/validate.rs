//! Static checks of a [`PluginInfo`]: everything the generated TTL and the
//! wrappers rely on. Run by every plugin's tests and by the bundle
//! generator before it writes anything.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::desc::{ParamKind, PluginInfo, PortKind};
use crate::view::{CurveShape, Face, Family, StreamContent, Value, ViewKind, is_color};

/// Problems found in `info`, one line each (empty when it is valid)
pub fn check(info: &PluginInfo) -> Vec<String> {
    let mut errors = Vec::new();
    let mut err = |msg: String| errors.push(msg);

    if !info.uri.starts_with("https://") || info.uri.contains(char::is_whitespace) || info.uri.ends_with('/') {
        err(format!("uri {:?}: expected an https URI without spaces or trailing slash", info.uri));
    }
    if info.name.trim().is_empty() {
        err("empty name".into());
    }
    if info.license != "GPL-3.0-or-later" {
        err(format!("license {:?}: the suite is GPL-3.0-or-later", info.license));
    }
    if info.version.minor % 2 == 1 {
        err(format!("odd lv2:minorVersion {} marks an unstable plugin", info.version.minor));
    }
    if info.ports.is_empty() {
        err("no ports".into());
    }

    let mut symbols = HashSet::new();
    let (mut enabled, mut latency) = (0, 0);
    for (i, port) in info.ports.iter().enumerate() {
        let at = format!("port {i} ({})", port.symbol);
        if !is_symbol(port.symbol) {
            err(format!("{at}: symbol must match [A-Za-z_][A-Za-z0-9_]*"));
        }
        if !symbols.insert(port.symbol) {
            err(format!("{at}: duplicate symbol"));
        }
        if port.name.trim().is_empty() {
            err(format!("{at}: empty name"));
        }
        match port.kind {
            PortKind::Param(p) => {
                if !(p.min.is_finite() && p.max.is_finite() && p.default.is_finite()) {
                    err(format!("{at}: non-finite range or default"));
                } else if p.min >= p.max {
                    err(format!("{at}: min {} >= max {}", p.min, p.max));
                } else if p.default < p.min || p.default > p.max {
                    err(format!("{at}: default {} outside [{}, {}]", p.default, p.min, p.max));
                } else if p.sanitize(p.default) != p.default {
                    err(format!("{at}: default {} is not a valid value", p.default));
                }
                if p.log && p.min <= 0.0 {
                    err(format!("{at}: logarithmic with min <= 0"));
                }
                if let ParamKind::Choice(choices) = p.kind {
                    if choices.len() < 2 {
                        err(format!("{at}: fewer than two choices"));
                    }
                    let mut values = HashSet::new();
                    for c in choices {
                        if c.value.fract() != 0.0 || !values.insert(c.value.to_bits()) {
                            err(format!("{at}: choice values must be distinct integers"));
                        }
                        if c.label.trim().is_empty() {
                            err(format!("{at}: empty choice label"));
                        }
                    }
                }
                if p.steps == Some(0) || p.steps == Some(1) {
                    err(format!("{at}: steps must be at least 2"));
                }
            }
            PortKind::Meter(m) => {
                if !(m.min.is_finite() && m.max.is_finite()) || m.min >= m.max {
                    err(format!("{at}: bad meter range"));
                }
            }
            PortKind::Enabled => {
                enabled += 1;
                if port.symbol != "enabled" {
                    err(format!("{at}: the enabled port has symbol \"enabled\""));
                }
            }
            PortKind::Latency => {
                latency += 1;
                if port.symbol != "latency" {
                    err(format!("{at}: the latency port has symbol \"latency\""));
                }
            }
            PortKind::Events { midi, time } => {
                if !midi && !time {
                    err(format!("{at}: event port supporting nothing"));
                }
            }
            PortKind::Stream(s) => {
                if s.content == StreamContent::StepGrid && s.channels != StreamContent::STEP_GRID_ROWS {
                    err(format!("{at}: a StepGrid stream has {} rows", StreamContent::STEP_GRID_ROWS));
                }
                if s.length == 0 || s.channels == 0 || s.values() > 1024 {
                    err(format!("{at}: stream size must be 1 to 1024 values"));
                }
                if !(s.max_rate > 0.0 && s.max_rate <= 30.0) {
                    err(format!("{at}: stream rate must be in (0, 30] Hz"));
                }
            }
            PortKind::AudioIn | PortKind::AudioOut => {}
        }
    }
    if enabled > 1 {
        err("more than one enabled port".into());
    }
    if latency > 1 {
        err("more than one latency port".into());
    }
    if info.audio_inputs() > crate::runner::MAX_CHANNELS || info.audio_outputs() > crate::runner::MAX_CHANNELS {
        err(format!("more than {} audio ports on a side", crate::runner::MAX_CHANNELS));
    }

    // Primary ranks: distinct
    let mut ranks = HashSet::new();
    for port in info.ports {
        if let Some(rank) = port.as_param().and_then(|p| p.primary)
            && (rank == 0 || !ranks.insert(rank))
        {
            err(format!("port {}: primary rank {rank} is 0 or taken", port.symbol));
        }
    }

    // Face
    let face = &info.face;
    let n = face.tagline.chars().count();
    if n == 0 || n > Face::TAGLINE_MAX || face.tagline.trim() != face.tagline {
        err(format!("face: tagline must be 1 to {} characters, trimmed", Face::TAGLINE_MAX));
    }
    match face.color {
        Some(c) if !is_color(c) => err(format!("face: colour {c:?} is not #RRGGBB")),
        None if face.family == Family::Signature => err("face: a signature plugin needs its colour".into()),
        _ => {}
    }
    if let Some(name) = face.plate_name {
        let n = name.chars().count();
        if n == 0 || n > Face::PLATE_NAME_MAX || name.trim() != name {
            err(format!("face: plate name must be 1 to {} characters, trimmed", Face::PLATE_NAME_MAX));
        }
    }
    if face.wear.is_some_and(|w| !(0.0..=1.0).contains(&w)) {
        err("face: wear must be within 0 to 1".into());
    }
    if face.family == Family::Signature && (face.plate_name.is_none() || face.finish.is_none() || face.wear.is_none()) {
        err("face: a signature plugin needs its plate name, finish and wear".into());
    }
    if face.plate_style.is_some() && face.family != Family::Signature {
        err("face: only a signature plugin has a plate style".into());
    }
    let mut placed = HashSet::new();
    for s in face.layout {
        let control = info.port_index(s).is_some_and(|i| matches!(info.ports[i].kind, PortKind::Param(_)));
        if !control || !placed.insert(*s) {
            err(format!("face: {s} is not a parameter, or placed twice"));
        }
    }

    // Presets
    let mut ids = HashSet::new();
    for preset in info.presets {
        let at = format!("preset {:?}", preset.id);
        if !is_symbol(&preset.id.replace('-', "_")) || !ids.insert(preset.id) {
            err(format!("{at}: id must be a unique [A-Za-z0-9_-] identifier"));
        }
        for (symbol, value) in preset.values {
            match info.port_index(symbol).and_then(|i| info.ports[i].as_param()) {
                None => err(format!("{at}: {symbol} is not a parameter")),
                Some(p) if p.sanitize(*value) != *value => {
                    err(format!("{at}: {symbol} = {value} is not a valid value"));
                }
                Some(_) => {}
            }
        }
    }

    // View references
    if let Some(view) = info.view {
        let param = |s: &str| info.port_index(s).and_then(|i| info.ports[i].as_param()).is_some();
        let mut refs = Vec::new();
        for curve in view.curves {
            if let CurveShape::Port(s, _) = curve.shape {
                refs.push(s);
            }
            for v in [Some(curve.frequency), curve.gain, curve.q].into_iter().flatten() {
                if let Value::Port(s) = v {
                    refs.push(s);
                }
            }
            if let Some(s) = curve.enable {
                refs.push(s);
            }
        }
        for s in refs {
            if !param(s) {
                err(format!("view: {s} is not a parameter"));
            }
        }
        let streams =
            |content| info.ports.iter().any(|p| matches!(p.kind, PortKind::Stream(s) if s.content == content));
        if view.kind == ViewKind::StepGrid && !streams(StreamContent::StepGrid) {
            err("view: a StepGridView needs a StepGrid stream".into());
        }
    }

    errors
}

/// Panic with every problem of `info` (for tests)
pub fn assert_valid(info: &PluginInfo) {
    let errors = check(info);
    if !errors.is_empty() {
        let mut msg = format!("{} is not valid:", info.uri);
        for e in errors {
            let _ = write!(msg, "\n  - {e}");
        }
        panic!("{msg}");
    }
}

/// A C identifier, as `lv2:symbol` requires
pub fn is_symbol(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desc::{Choice, Class, Param, Port, Preset, Version};
    use crate::view::{Finish, KnobStyle, PlateStyle};

    const CHOICES: &[Choice] = &[Choice::new(0.0, "A"), Choice::new(1.0, "B")];
    const GOOD: PluginInfo = PluginInfo {
        uri: "https://openrig.eu/plugins/test",
        name: "Test",
        description: "",
        class: Class::Utility,
        version: Version::parse("0.2"),
        license: "GPL-3.0-or-later",
        ports: &[
            Port::audio_in("in", "In"),
            Port::audio_out("out", "Out"),
            Port::param("gain", "Gain", Param::float(0.0, -12.0, 12.0).primary(1)),
            Port::param("mode", "Mode", Param::choice(0.0, CHOICES)),
            Port::enabled(),
        ],
        presets: &[Preset { name: "Loud", id: "loud", values: &[("gain", 6.0), ("mode", 1.0)] }],
        view: None,
        face: Face::basic("A test", &["gain", "mode"]),
        hard_rt: true,
    };

    #[test]
    fn good_info_passes() {
        assert_valid(&GOOD);
    }

    #[test]
    fn mistakes_are_found() {
        const BAD: PluginInfo = PluginInfo {
            uri: "http://x/",
            license: "MIT",
            version: Version::parse("0.1"),
            ports: &[
                Port::audio_in("in", "In"),
                Port::audio_out("in", "Out"),
                Port::param("9x", "X", Param::float(20.0, -12.0, 12.0)),
                Port::param("f", "F", Param::float(1.0, 0.0, 10.0).log()),
            ],
            presets: &[Preset { name: "P", id: "p", values: &[("nope", 1.0)] }],
            ..GOOD
        };
        let errors = check(&BAD);
        for needle in ["uri", "license", "minorVersion", "duplicate", "symbol must", "outside", "logarithmic", "nope"] {
            assert!(errors.iter().any(|e| e.contains(needle)), "{needle} not reported in {errors:#?}");
        }
    }

    #[test]
    fn step_grids_need_their_stream() {
        use crate::desc::Stream;
        use crate::view::{StreamContent, View, ViewKind};
        const fn grid(channels: u32) -> Port {
            Port::stream(
                "grid",
                "Grid",
                Stream { content: StreamContent::StepGrid, length: 48, channels, max_rate: 30.0 },
            )
        }
        const TWO: &[Port] = &[Port::audio_in("in", "In"), Port::audio_out("out", "Out"), grid(2)];
        const THREE: &[Port] = &[Port::audio_in("in", "In"), Port::audio_out("out", "Out"), grid(3)];
        let view = Some(View::new(ViewKind::StepGrid));
        let missing = check(&PluginInfo { view, ..GOOD });
        assert!(missing.iter().any(|e| e.contains("StepGridView")), "{missing:#?}");
        let rows = check(&PluginInfo { ports: TWO, ..GOOD });
        assert!(rows.iter().any(|e| e.contains("3 rows")), "{rows:#?}");
        let face = Face::basic("A test", &[]);
        let good = check(&PluginInfo { ports: THREE, view, face, presets: &[], ..GOOD });
        assert!(good.is_empty(), "{good:#?}");
    }

    #[test]
    fn face_mistakes_are_found() {
        let face = |face| check(&PluginInfo { face, ..GOOD });
        let bad = [
            (Face::basic("", &[]), "tagline"),
            (Face::basic(" A test", &[]), "tagline"),
            (Face::basic("A tagline far longer than any faceplate could show", &[]), "tagline"),
            (Face::signature("TEST", "A test", "#12345", KnobStyle::Domed, &[]), "colour"),
            (Face { color: None, ..Face::signature("TEST", "A test", "#123456", KnobStyle::Domed, &[]) }, "colour"),
            (Face::signature("", "A test", "#123456", KnobStyle::Domed, &[]), "plate name"),
            (Face::signature("THIRTEEN CHAR", "A test", "#123456", KnobStyle::Domed, &[]), "plate name"),
            (Face::signature("TEST", "A test", "#123456", KnobStyle::Domed, &[]).wear(1.5), "wear"),
            (Face { finish: None, ..Face::signature("TEST", "A test", "#123456", KnobStyle::Domed, &[]) }, "finish"),
            (Face::basic("A test", &["gain", "gain"]), "twice"),
            (Face::basic("A test", &["in"]), "in is not"),
            (Face::basic("A test", &[]).plate_style(PlateStyle::Glitch), "plate style"),
        ];
        for (f, needle) in bad {
            let errors = face(f);
            assert!(errors.iter().any(|e| e.contains(needle)), "{needle} not reported for {f:?}: {errors:#?}");
        }
        let good = Face::signature("TEST", "A test", "#2a4B3c", KnobStyle::Skirted, &["mode", "gain"]);
        assert!(face(good.finish(Finish::Brushed).wear(0.4)).is_empty());
        assert!(face(good.plate_style(PlateStyle::Glitch)).is_empty());
    }

    #[test]
    fn symbols() {
        assert!(is_symbol("rate_hz"));
        assert!(is_symbol("_x1"));
        assert!(!is_symbol("1x"));
        assert!(!is_symbol("a-b"));
        assert!(!is_symbol(""));
    }
}
