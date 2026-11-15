//! The `openrig:` RDF vocabulary (`ns/openrig-view.ttl`), generated from
//! the enums of [`orx_plugin::view`] so that the schema, the plugin
//! descriptions and the code cannot disagree. `docs/OPENRIG-VIEWS.md` is
//! the human-readable specification of the same terms.

use crate::ttl::literal;
use orx_plugin::view::{
    DynamicsKind, Family, FilterShape, Finish, KnobStyle, NS, PlateStyle, QUnit, Role, StreamContent, ViewKind,
    Waveform,
};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// A class of the vocabulary
struct Class {
    name: &'static str,
    comment: &'static str,
}

/// A property: name, domain, range, comment
struct Property {
    name: &'static str,
    domain: &'static str,
    range: &'static str,
    comment: &'static str,
}

const CLASSES: &[Class] = &[
    Class { name: "View", comment: "A kind of native view a host draws for a plugin. Instances are the views below." },
    Class {
        name: "Role",
        comment: "What a port means to the view. Instances are lower camel case individuals; each states its value convention.",
    },
    Class {
        name: "Waveform",
        comment: "An LFO waveform with an exact definition, so the host draws the curve the plugin plays.",
    },
    Class {
        name: "FilterShape",
        comment: "A filter response with the RBJ Audio EQ Cookbook definition, for openrig:curve bands. openrig:Off is a band that contributes nothing.",
    },
    Class { name: "QUnit", comment: "How the Q value of an openrig:curve band reads." },
    Class { name: "DynamicsKind", comment: "The kind of processor a DynamicsView draws." },
    Class { name: "Curve", comment: "One band of a frequency response the host computes from port values." },
    Class { name: "ShapeChoice", comment: "One value of an openrig:shapePort and the band shape it selects." },
    Class { name: "Stream", comment: "Description of the frames an atom output port sends." },
    Class { name: "StreamContent", comment: "What the values of a stream frame are." },
    Class {
        name: "Frame",
        comment: "Atom object sent on a stream port: openrig:channels (Int), openrig:position (Float) and openrig:values (Vector of Float, channel after channel).",
    },
    Class { name: "Family", comment: "How a host presents a plugin: in its own look, or as a faceplate of its own." },
    Class { name: "KnobStyle", comment: "The knob shape a faceplate draws." },
    Class { name: "Finish", comment: "The surface of a faceplate." },
    Class { name: "PlateStyle", comment: "How the plate name of a faceplate is lettered." },
];

const PROPERTIES: &[Property] = &[
    Property {
        name: "view",
        domain: "lv2:Plugin",
        range: "openrig:View",
        comment: "The view a host should draw above the plugin's controls.",
    },
    Property {
        name: "waveform",
        domain: "lv2:Plugin",
        range: "openrig:Waveform",
        comment: "Waveform of an LfoView; openrig:lfoShape morphs it where the waveform says so.",
    },
    Property {
        name: "dynamics",
        domain: "lv2:Plugin",
        range: "openrig:DynamicsKind",
        comment: "Kind of processor of a DynamicsView.",
    },
    Property {
        name: "curve",
        domain: "lv2:Plugin",
        range: "openrig:Curve",
        comment: "A band of the frequency response of an EqView; the response is the product of all bands.",
    },
    Property {
        name: "shape",
        domain: "openrig:Curve",
        range: "openrig:FilterShape",
        comment: "Fixed shape of a band, or the shape an openrig:ShapeChoice selects.",
    },
    Property {
        name: "shapePort",
        domain: "openrig:Curve",
        range: "xsd:string",
        comment: "Symbol of the port choosing the band shape, with one openrig:shapeChoice per value.",
    },
    Property {
        name: "shapeChoice",
        domain: "openrig:Curve",
        range: "openrig:ShapeChoice",
        comment: "Maps one rdf:value of the openrig:shapePort to an openrig:shape.",
    },
    Property { name: "frequency", domain: "openrig:Curve", range: "xsd:decimal", comment: "Fixed band frequency, Hz." },
    Property {
        name: "frequencyPort",
        domain: "openrig:Curve",
        range: "xsd:string",
        comment: "Symbol of the port holding the band frequency, Hz.",
    },
    Property { name: "gain", domain: "openrig:Curve", range: "xsd:decimal", comment: "Fixed band gain, dB." },
    Property {
        name: "gainPort",
        domain: "openrig:Curve",
        range: "xsd:string",
        comment: "Symbol of the port holding the band gain, dB.",
    },
    Property {
        name: "q",
        domain: "openrig:Curve",
        range: "xsd:decimal",
        comment: "Fixed band Q, read per openrig:qUnit.",
    },
    Property {
        name: "qPort",
        domain: "openrig:Curve",
        range: "xsd:string",
        comment: "Symbol of the port holding the band Q, read per openrig:qUnit.",
    },
    Property { name: "qUnit", domain: "openrig:Curve", range: "openrig:QUnit", comment: "How the band Q reads." },
    Property {
        name: "enablePort",
        domain: "openrig:Curve",
        range: "xsd:string",
        comment: "Symbol of a toggle port; the band is flat while it is 0.",
    },
    Property {
        name: "family",
        domain: "lv2:Plugin",
        range: "openrig:Family",
        comment: "The plugin's family; a host without faceplates draws every family alike.",
    },
    Property {
        name: "faceColor",
        domain: "lv2:Plugin",
        range: "xsd:string",
        comment: "Faceplate colour, \"#RRGGBB\" in sRGB. The host keeps its own accent, text and texture over it, and may darken or lighten it to keep labels readable.",
    },
    Property {
        name: "knobStyle",
        domain: "lv2:Plugin",
        range: "openrig:KnobStyle",
        comment: "The knob shape of the faceplate.",
    },
    Property {
        name: "finish",
        domain: "lv2:Plugin",
        range: "openrig:Finish",
        comment: "The surface of the faceplate.",
    },
    Property {
        name: "wear",
        domain: "lv2:Plugin",
        range: "xsd:decimal",
        comment: "How worn the faceplate looks, 0 (new) to 1, fixed for the plugin. The host draws the wear from a seed it derives from the plugin URI, so a plugin always looks the same.",
    },
    Property {
        name: "plateName",
        domain: "lv2:Plugin",
        range: "xsd:string",
        comment: "The short name printed on the faceplate, 12 characters at most.",
    },
    Property {
        name: "plateStyle",
        domain: "lv2:Plugin",
        range: "openrig:PlateStyle",
        comment: "The lettering of the plate name, openrig:Silkscreen when absent. Signature plugins only.",
    },
    Property {
        name: "tagline",
        domain: "lv2:Plugin",
        range: "xsd:string",
        comment: "A few words under the plugin's name, 48 characters at most.",
    },
    Property {
        name: "order",
        domain: "lv2:Port",
        range: "xsd:positiveInteger",
        comment: "Place (1 first) of the control on the faceplate, in reading order. Controls without one stay in the host's full control list.",
    },
    Property { name: "role", domain: "lv2:Port", range: "openrig:Role", comment: "What the port means to the view." },
    Property {
        name: "primary",
        domain: "lv2:Port",
        range: "xsd:positiveInteger",
        comment: "Rank (1 first) of the port among the controls a compact host shows first, such as the knob strip of a footswitch screen.",
    },
    Property {
        name: "stream",
        domain: "lv2:Port",
        range: "openrig:Stream",
        comment: "The frames this atom output sends.",
    },
    Property {
        name: "content",
        domain: "openrig:Stream",
        range: "openrig:StreamContent",
        comment: "What the values of each frame are.",
    },
    Property {
        name: "length",
        domain: "openrig:Stream",
        range: "xsd:positiveInteger",
        comment: "Values per channel in each frame.",
    },
    Property {
        name: "channels",
        domain: "openrig:Stream",
        range: "xsd:positiveInteger",
        comment: "Channels in each frame. Also the key of the channel count in an openrig:Frame.",
    },
    Property {
        name: "rate",
        domain: "openrig:Stream",
        range: "xsd:decimal",
        comment: "Most frames per second the plugin sends, Hz.",
    },
    Property {
        name: "position",
        domain: "openrig:Frame",
        range: "xsd:float",
        comment: "A position in the frame (the current LFO phase of a ModulationCycle), per openrig:content.",
    },
    Property {
        name: "values",
        domain: "openrig:Frame",
        range: "atom:Vector",
        comment: "The values of the frame, channel after channel.",
    },
];

const WAVEFORM_COMMENTS: [(Waveform, &str); 6] = [
    (Waveform::Sine, "sin(2 pi phase)."),
    (Waveform::Triangle, "Triangle, 0 at phase 0, 1 at 1/4, -1 at 3/4."),
    (Waveform::Square, "1 for phase below 1/2, -1 above."),
    (Waveform::SawUp, "2 phase - 1."),
    (Waveform::SawDown, "1 - 2 phase."),
    (
        Waveform::SineToSquare,
        "tanh(k sin(2 pi phase)) / tanh(k) with k = 16 s^2, s the openrig:lfoShape position on its range from 0 to 1. A pure sine at s = 0.",
    ),
];

const QUNITS: [(QUnit, &str); 3] = [
    (QUnit::Q, "Cookbook Q."),
    (QUnit::Octaves, "Bandwidth in octaves (cookbook BW)."),
    (QUnit::Slope, "Shelf slope S (cookbook); 1 is the steepest without overshoot."),
];

const DYNAMICS: [DynamicsKind; 4] =
    [DynamicsKind::Compressor, DynamicsKind::Limiter, DynamicsKind::Gate, DynamicsKind::Expander];

fn individual(s: &mut String, name: &str, class: &str, label: &str, comment: &str) {
    let _ = writeln!(
        s,
        "\nopenrig:{name}\n    a openrig:{class} ;\n    rdfs:label {} ;\n    rdfs:comment {} .",
        literal(label),
        literal(comment)
    );
}

/// The vocabulary as Turtle
pub fn turtle() -> String {
    let mut s = format!(
        "@prefix atom: <http://lv2plug.in/ns/ext/atom#> .\n\
         @prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
         @prefix openrig: <{NS}> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
         \n\
         # Generated from orx_plugin::view by `cargo xtask vocab`: do not edit.\n\
         # Specification: docs/OPENRIG-VIEWS.md\n\
         \n\
         <{NS}>\n    a owl:Ontology ;\n    rdfs:label \"OpenRig native views\" ;\n    \
         rdfs:comment \"Metadata letting a host draw a native view of a plugin: the view, port roles, exact curves and data streams.\" ;\n    \
         owl:versionInfo \"1\" .\n"
    );
    for c in CLASSES {
        let _ = writeln!(
            s,
            "\nopenrig:{}\n    a rdfs:Class ;\n    rdfs:label {} ;\n    rdfs:comment {} .",
            c.name,
            literal(c.name),
            literal(c.comment)
        );
    }
    for p in PROPERTIES {
        let kind = if p.range.starts_with("xsd:") { "owl:DatatypeProperty" } else { "owl:ObjectProperty" };
        let _ = writeln!(
            s,
            "\nopenrig:{}\n    a rdf:Property , {kind} ;\n    rdfs:label {} ;\n    rdfs:domain {} ;\n    rdfs:range {} ;\n    rdfs:comment {} .",
            p.name,
            literal(p.name),
            p.domain,
            p.range,
            literal(p.comment)
        );
    }
    for v in ViewKind::ALL {
        individual(&mut s, v.local_name(), "View", v.local_name(), v.comment());
    }
    for r in Role::ALL {
        individual(&mut s, r.local_name(), "Role", r.local_name(), r.comment());
    }
    for (w, comment) in WAVEFORM_COMMENTS {
        individual(&mut s, w.local_name(), "Waveform", w.local_name(), comment);
    }
    for f in FilterShape::ALL {
        individual(&mut s, f.local_name(), "FilterShape", f.local_name(), "RBJ Audio EQ Cookbook response.");
    }
    individual(&mut s, "Off", "FilterShape", "Off", "No band: a flat response of 0 dB.");
    for (q, comment) in QUNITS {
        individual(&mut s, q.local_name(), "QUnit", q.local_name(), comment);
    }
    for d in DYNAMICS {
        individual(&mut s, d.local_name(), "DynamicsKind", d.local_name(), "Kind of dynamics processor.");
    }
    for c in StreamContent::ALL {
        individual(&mut s, c.local_name(), "StreamContent", c.local_name(), c.comment());
    }
    for f in Family::ALL {
        individual(&mut s, f.local_name(), "Family", f.local_name(), f.comment());
    }
    for k in KnobStyle::ALL {
        individual(&mut s, k.local_name(), "KnobStyle", k.local_name(), k.comment());
    }
    for f in Finish::ALL {
        individual(&mut s, f.local_name(), "Finish", f.local_name(), f.comment());
    }
    for p in PlateStyle::ALL {
        individual(&mut s, p.local_name(), "PlateStyle", p.local_name(), p.comment());
    }
    s
}

/// Every term the vocabulary defines (local names)
pub fn terms() -> BTreeSet<String> {
    let mut t: BTreeSet<String> = CLASSES.iter().map(|c| c.name.to_string()).collect();
    t.extend(PROPERTIES.iter().map(|p| p.name.to_string()));
    t.extend(ViewKind::ALL.iter().map(|v| v.local_name().to_string()));
    t.extend(Role::ALL.iter().map(|r| r.local_name().to_string()));
    t.extend(WAVEFORM_COMMENTS.iter().map(|(w, _)| w.local_name().to_string()));
    t.extend(FilterShape::ALL.iter().map(|f| f.local_name().to_string()));
    t.insert("Off".into());
    t.extend(QUNITS.iter().map(|(q, _)| q.local_name().to_string()));
    t.extend(DYNAMICS.iter().map(|d| d.local_name().to_string()));
    t.extend(StreamContent::ALL.iter().map(|c| c.local_name().to_string()));
    t.extend(Family::ALL.iter().map(|f| f.local_name().to_string()));
    t.extend(KnobStyle::ALL.iter().map(|k| k.local_name().to_string()));
    t.extend(Finish::ALL.iter().map(|f| f.local_name().to_string()));
    t.extend(PlateStyle::ALL.iter().map(|p| p.local_name().to_string()));
    t
}

/// The `openrig:` terms used in `ttl` (prefixed names) that the
/// vocabulary does not define
pub fn undefined_terms(ttl: &str) -> BTreeSet<String> {
    let known = terms();
    ttl.match_indices("openrig:")
        .map(|(i, m)| {
            let rest = &ttl[i + m.len()..];
            let end = rest.find(|c: char| !c.is_ascii_alphanumeric() && c != '_').unwrap_or(rest.len());
            rest[..end].to_string()
        })
        .filter(|name| !name.is_empty() && !known.contains(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vocabulary_defines_what_it_uses() {
        let ttl = turtle();
        assert_eq!(undefined_terms(&ttl), BTreeSet::new());
        // Each term is the subject of exactly one description
        for term in terms() {
            assert_eq!(ttl.matches(&format!("\nopenrig:{term}\n")).count(), 1, "{term}");
        }
    }

    #[test]
    fn the_committed_schema_is_current() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../ns/openrig-view.ttl");
        let committed = std::fs::read_to_string(path).unwrap_or_default();
        assert!(committed == turtle(), "ns/openrig-view.ttl is stale: run `cargo xtask vocab`");
    }

    #[test]
    fn the_specification_covers_every_term() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/OPENRIG-VIEWS.md");
        let doc = std::fs::read_to_string(path).unwrap_or_default();
        let missing: Vec<String> = terms().into_iter().filter(|t| !doc.contains(&format!("`openrig:{t}`"))).collect();
        assert!(missing.is_empty(), "docs/OPENRIG-VIEWS.md does not cover {missing:?}");
        assert!(undefined_terms(&doc).is_empty(), "docs/OPENRIG-VIEWS.md uses {:?}", undefined_terms(&doc));
    }

    #[test]
    fn every_waveform_is_described() {
        assert_eq!(WAVEFORM_COMMENTS.map(|(w, _)| w), Waveform::ALL);
    }
}
