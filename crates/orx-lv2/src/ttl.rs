//! Turtle generation: the bundle's `manifest.ttl`, one description per
//! plugin and its presets, all from the [`PluginInfo`] in Rust.

use orx_plugin::desc::{ParamKind, PluginInfo, PortKind, Unit};
use orx_plugin::dynamic::Entry;
use orx_plugin::view::{self, CurveShape, Value};
use std::fmt::Write as _;

const PREFIXES: &str = "\
@prefix atom: <http://lv2plug.in/ns/ext/atom#> .
@prefix bufsz: <http://lv2plug.in/ns/ext/buf-size#> .
@prefix doap: <http://usefulinc.com/ns/doap#> .
@prefix foaf: <http://xmlns.com/foaf/0.1/> .
@prefix lv2: <http://lv2plug.in/ns/lv2core#> .
@prefix midi: <http://lv2plug.in/ns/ext/midi#> .
@prefix openrig: <https://openrig.eu/ns/view#> .
@prefix opts: <http://lv2plug.in/ns/ext/options#> .
@prefix pg: <http://lv2plug.in/ns/ext/port-groups#> .
@prefix pprops: <http://lv2plug.in/ns/ext/port-props#> .
@prefix pset: <http://lv2plug.in/ns/ext/presets#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rsz: <http://lv2plug.in/ns/ext/resize-port#> .
@prefix state: <http://lv2plug.in/ns/ext/state#> .
@prefix time: <http://lv2plug.in/ns/ext/time#> .
@prefix units: <http://lv2plug.in/ns/extensions/units#> .
@prefix urid: <http://lv2plug.in/ns/ext/urid#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
";

/// The project every plugin belongs to (`lv2:project`)
pub const PROJECT: &str = "https://openrig.eu/fx";

/// The maintainer of every plugin (`doap:maintainer`), described in each
/// plugin file (doap wants a foaf:Person)
const MAINTAINER: &str = "https://openrig.eu/#openrig";

/// What the generator needs besides the plugin descriptions
#[derive(Clone, Copy, Debug)]
pub struct Bundle<'a> {
    /// Shared library file name in the bundle, e.g. `or_tremolo.so`
    pub binary: &'a str,
}

/// A Turtle string literal
pub fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A float in Turtle decimal or double syntax, never in a form Turtle
/// rejects (`1e5` needs no dot, but `5` alone would be an integer)
pub fn number(v: f32) -> String {
    let s = format!("{v:?}");
    if s.contains(['.', 'e', 'E']) { s } else { format!("{s}.0") }
}

/// File name of a plugin's description: the last URI segment
pub fn ttl_name(info: &PluginInfo) -> String {
    format!("{}.ttl", slug(info))
}

pub fn slug(info: &PluginInfo) -> &'static str {
    info.uri.rsplit('/').next().unwrap_or(info.uri)
}

fn preset_uri(info: &PluginInfo, id: &str) -> String {
    format!("{}#preset-{id}", info.uri)
}

/// `manifest.ttl` of a bundle holding `plugins`
pub fn manifest(plugins: &[Entry], bundle: &Bundle<'_>) -> String {
    let mut s = String::from(PREFIXES);
    for info in plugins.iter().map(|e| e.info) {
        let _ = write!(
            s,
            "\n<{}>\n    a lv2:Plugin, lv2:{} ;\n    lv2:binary <{}> ;\n    rdfs:seeAlso <{}> .\n",
            info.uri,
            info.class.lv2_name(),
            bundle.binary,
            ttl_name(info)
        );
        for preset in info.presets {
            let _ = write!(
                s,
                "\n<{}>\n    a pset:Preset ;\n    lv2:appliesTo <{}> ;\n    rdfs:label {} ;\n    rdfs:seeAlso <{}> .\n",
                preset_uri(info, preset.id),
                info.uri,
                literal(preset.name),
                presets_name(info)
            );
        }
    }
    s
}

/// File name of a plugin's presets
pub fn presets_name(info: &PluginInfo) -> String {
    format!("{}-presets.ttl", slug(info))
}

/// The presets of a plugin: every parameter, defaults where the preset
/// says nothing
pub fn presets(info: &PluginInfo) -> String {
    let mut s = String::from(PREFIXES);
    for preset in info.presets {
        let _ = write!(
            s,
            "\n<{}>\n    a pset:Preset ;\n    lv2:appliesTo <{}> ;\n    rdfs:label {} ;\n    lv2:port",
            preset_uri(info, preset.id),
            info.uri,
            literal(preset.name)
        );
        let values: Vec<String> = info
            .preset_values(preset)
            .map(|(i, v)| {
                format!(
                    "[\n        lv2:symbol {} ;\n        pset:value {}\n    ]",
                    literal(info.ports[i].symbol),
                    number(v)
                )
            })
            .collect();
        let _ = writeln!(s, " {} .", values.join(" , "));
    }
    s
}

fn unit(u: Unit) -> Option<String> {
    u.lv2_name().map(|n| format!("units:{n}"))
}

/// The description of one plugin
pub fn plugin(entry: &Entry) -> String {
    let info = entry.info;
    let mut s = String::from(PREFIXES);
    let _ = write!(
        s,
        "\n<{MAINTAINER}>\n    a foaf:Person ;\n    foaf:name \"OpenRig\" ;\n    foaf:mbox <mailto:contact@openrig.eu> ;\n    foaf:homepage <https://openrig.eu> .\n\n<{PROJECT}>\n    a doap:Project ;\n    doap:name \"OpenRig FX\" ;\n    doap:homepage <https://openrig.eu> ;\n    doap:maintainer <{MAINTAINER}> .\n"
    );
    let needs_map = crate::instance::entry_needs_map(entry);
    // lv2:Plugin is a doap:Project through lv2:PluginBase; saying so lets
    // validators that do not follow rdfs:subClassOf check the doap terms
    let _ = write!(s, "\n<{}>\n    a lv2:Plugin, lv2:{}, doap:Project ;\n", info.uri, info.class.lv2_name());
    let _ = writeln!(s, "    doap:name {} ;", literal(info.name));
    let _ = writeln!(s, "    rdfs:comment {} ;", literal(info.description));
    let _ = writeln!(s, "    doap:license <http://spdx.org/licenses/{}> ;", info.license);
    let _ = writeln!(s, "    doap:maintainer <{MAINTAINER}> ;");
    let _ = writeln!(s, "    lv2:project <{PROJECT}> ;");
    let _ = writeln!(s, "    lv2:minorVersion {} ;\n    lv2:microVersion {} ;", info.version.minor, info.version.micro);
    if needs_map {
        let _ = writeln!(s, "    lv2:requiredFeature urid:map ;");
    }
    let mut optional = vec!["opts:options", "bufsz:boundedBlockLength"];
    if info.hard_rt {
        optional.insert(0, "lv2:hardRTCapable");
    }
    let _ = writeln!(s, "    lv2:optionalFeature {} ;", optional.join(" , "));
    let _ = writeln!(s, "    opts:supportedOption bufsz:maxBlockLength , bufsz:nominalBlockLength ;");
    if entry.has_state {
        let _ = writeln!(s, "    lv2:extensionData state:interface ;");
    }
    if let Some(v) = info.view {
        write_view(&mut s, &v);
    }
    write_face(&mut s, &info.face);
    s += "    lv2:port ";
    let ports: Vec<String> =
        info.ports.iter().enumerate().map(|(i, p)| port(i, p, info.face.order(p.symbol))).collect();
    s += &ports.join(" , ");
    s += " .\n";
    s
}

fn value(v: &Value) -> String {
    match v {
        Value::Fixed(x) => number(*x),
        Value::Port(sym) => literal(sym),
    }
}

fn write_view(s: &mut String, v: &view::View) {
    let _ = writeln!(s, "    openrig:view openrig:{} ;", v.kind.local_name());
    if let Some(w) = v.waveform {
        let _ = writeln!(s, "    openrig:waveform openrig:{} ;", w.local_name());
    }
    if let Some(d) = v.dynamics {
        let _ = writeln!(s, "    openrig:dynamics openrig:{} ;", d.local_name());
    }
    for c in v.curves {
        let mut props = Vec::new();
        match c.shape {
            CurveShape::Fixed(shape) => props.push(format!("openrig:shape openrig:{}", shape.local_name())),
            CurveShape::Port(sym, map) => {
                props.push(format!("openrig:shapePort {}", literal(sym)));
                for (val, shape) in map {
                    let shape = shape.map_or("openrig:Off".to_string(), |sh| format!("openrig:{}", sh.local_name()));
                    props.push(format!("openrig:shapeChoice [ rdf:value {} ; openrig:shape {shape} ]", number(*val)));
                }
            }
        }
        let field = |name: &str, v: &Value| match v {
            Value::Fixed(_) => format!("openrig:{name} {}", value(v)),
            Value::Port(_) => format!("openrig:{name}Port {}", value(v)),
        };
        props.push(field("frequency", &c.frequency));
        if let Some(g) = &c.gain {
            props.push(field("gain", g));
        }
        if let Some(q) = &c.q {
            props.push(field("q", q));
        }
        props.push(format!("openrig:qUnit openrig:{}", c.q_unit.local_name()));
        if let Some(e) = c.enable {
            props.push(format!("openrig:enablePort {}", literal(e)));
        }
        let _ = writeln!(s, "    openrig:curve [\n        {}\n    ] ;", props.join(" ;\n        "));
    }
}

fn write_face(s: &mut String, f: &view::Face) {
    let _ = writeln!(s, "    openrig:family openrig:{} ;", f.family.local_name());
    if let Some(c) = f.color {
        let _ = writeln!(s, "    openrig:faceColor {} ;", literal(c));
    }
    let _ = writeln!(s, "    openrig:knobStyle openrig:{} ;", f.knob.local_name());
    if let Some(finish) = f.finish {
        let _ = writeln!(s, "    openrig:finish openrig:{} ;", finish.local_name());
    }
    if let Some(w) = f.wear {
        let _ = writeln!(s, "    openrig:wear {} ;", number(w));
    }
    if let Some(name) = f.plate_name {
        let _ = writeln!(s, "    openrig:plateName {} ;", literal(name));
    }
    if let Some(style) = f.plate_style {
        let _ = writeln!(s, "    openrig:plateStyle openrig:{} ;", style.local_name());
    }
    let _ = writeln!(s, "    openrig:tagline {} ;", literal(f.tagline));
}

/// A port; `order`: its rank on the faceplate
fn port(index: usize, p: &orx_plugin::Port, order: Option<usize>) -> String {
    let mut props: Vec<String> = Vec::new();
    let class = match p.kind {
        PortKind::AudioIn => "lv2:InputPort , lv2:AudioPort",
        PortKind::AudioOut => "lv2:OutputPort , lv2:AudioPort",
        PortKind::Param(_) | PortKind::Enabled => "lv2:InputPort , lv2:ControlPort",
        PortKind::Meter(_) | PortKind::Latency => "lv2:OutputPort , lv2:ControlPort",
        PortKind::Events { .. } => "lv2:InputPort , atom:AtomPort",
        PortKind::Stream(_) => "lv2:OutputPort , atom:AtomPort",
    };
    props.push(format!("a {class}"));
    props.push(format!("lv2:index {index}"));
    props.push(format!("lv2:symbol {}", literal(p.symbol)));
    props.push(format!("lv2:name {}", literal(p.name)));
    if !p.comment.is_empty() {
        props.push(format!("rdfs:comment {}", literal(p.comment)));
    }
    match p.kind {
        PortKind::Param(param) => {
            props.push(format!("lv2:default {}", number(param.default)));
            props.push(format!("lv2:minimum {}", number(param.min)));
            props.push(format!("lv2:maximum {}", number(param.max)));
            if let Some(u) = unit(param.unit) {
                props.push(format!("units:unit {u}"));
            }
            let mut pp: Vec<&str> = Vec::new();
            match param.kind {
                ParamKind::Float => {}
                ParamKind::Int => pp.push("lv2:integer"),
                ParamKind::Toggle => pp.push("lv2:toggled"),
                ParamKind::Choice(choices) => {
                    pp.extend(["lv2:integer", "lv2:enumeration"]);
                    let points: Vec<String> = choices
                        .iter()
                        .map(|c| format!("[ rdfs:label {} ; rdf:value {} ]", literal(c.label), number(c.value)))
                        .collect();
                    props.push(format!("lv2:scalePoint {}", points.join(" , ")));
                }
            }
            if param.log {
                pp.push("pprops:logarithmic");
            }
            if !pp.is_empty() {
                props.push(format!("lv2:portProperty {}", pp.join(" , ")));
            }
            if let Some(steps) = param.steps {
                props.push(format!("pprops:rangeSteps {steps}"));
            }
            if let Some(role) = param.role {
                props.push(format!("openrig:role openrig:{}", role.local_name()));
            }
            if let Some(rank) = param.primary {
                props.push(format!("openrig:primary {rank}"));
            }
            if let Some(rank) = order {
                props.push(format!("openrig:order {rank}"));
            }
        }
        PortKind::Enabled => {
            props.push("lv2:designation lv2:enabled".into());
            props.push("lv2:default 1".into());
            props.push("lv2:minimum 0".into());
            props.push("lv2:maximum 1".into());
            props.push("lv2:portProperty lv2:toggled , lv2:integer".into());
        }
        PortKind::Meter(m) => {
            props.push(format!("lv2:minimum {}", number(m.min)));
            props.push(format!("lv2:maximum {}", number(m.max)));
            if let Some(u) = unit(m.unit) {
                props.push(format!("units:unit {u}"));
            }
            if let Some(role) = m.role {
                props.push(format!("openrig:role openrig:{}", role.local_name()));
            }
        }
        PortKind::Latency => {
            props.push("lv2:designation lv2:latency".into());
            props.push("lv2:portProperty lv2:reportsLatency , lv2:integer , pprops:notOnGUI".into());
            props.push("lv2:minimum 0".into());
            props.push("lv2:maximum 192000".into());
            props.push("units:unit units:frame".into());
        }
        PortKind::Events { midi, time } => {
            props.push("atom:bufferType atom:Sequence".into());
            let mut supports = Vec::new();
            if midi {
                supports.push("midi:MidiEvent");
            }
            if time {
                supports.push("time:Position");
            }
            props.push(format!("atom:supports {}", supports.join(" , ")));
            props.push("lv2:designation lv2:control".into());
        }
        PortKind::Stream(st) => {
            props.push("atom:bufferType atom:Sequence".into());
            props.push("atom:supports openrig:Frame".into());
            // Room for a full frame, twice, plus the sequence header
            let size = 16 + 2 * crate::atom::frame_event_size(st.values());
            props.push(format!("rsz:minimumSize {size}"));
            props.push("lv2:portProperty pprops:notOnGUI".into());
            props.push(format!(
                "openrig:stream [\n            openrig:content openrig:{} ;\n            openrig:length {} ;\n            openrig:channels {} ;\n            openrig:rate {}\n        ]",
                st.content.local_name(),
                st.length,
                st.channels,
                number(st.max_rate)
            ));
        }
        PortKind::AudioIn | PortKind::AudioOut => {}
    }
    format!("[\n        {}\n    ]", props.join(" ;\n        "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_numbers() {
        assert_eq!(literal("a \"b\"\n"), "\"a \\\"b\\\"\\n\"");
        assert_eq!(number(5.0), "5.0");
        assert_eq!(number(0.1), "0.1");
        assert_eq!(number(-12.0), "-12.0");
        assert_eq!(number(1e-7), "1e-7");
    }
}
