//! URIs and their mapped URIDs.

use crate::sys::{LV2_URID, LV2_URID_Map};
use std::ffi::{CStr, CString};

pub const URID_MAP: &CStr = c"http://lv2plug.in/ns/ext/urid#map";
pub const OPTIONS: &CStr = c"http://lv2plug.in/ns/ext/options#options";
pub const STATE_INTERFACE: &CStr = c"http://lv2plug.in/ns/ext/state#interface";

/// URIDs the wrapper needs, mapped once at instantiation
#[derive(Debug, Clone, Copy, Default)]
pub struct Urids {
    pub atom_sequence: LV2_URID,
    pub atom_object: LV2_URID,
    pub atom_blank: LV2_URID,
    pub atom_resource: LV2_URID,
    pub atom_float: LV2_URID,
    pub atom_double: LV2_URID,
    pub atom_int: LV2_URID,
    pub atom_long: LV2_URID,
    pub atom_bool: LV2_URID,
    pub atom_vector: LV2_URID,
    pub atom_chunk: LV2_URID,
    pub atom_string: LV2_URID,
    pub midi_event: LV2_URID,
    pub time_position: LV2_URID,
    pub time_bpm: LV2_URID,
    pub time_speed: LV2_URID,
    pub time_bar_beat: LV2_URID,
    pub time_beats_per_bar: LV2_URID,
    pub time_beat_unit: LV2_URID,
    pub max_block: LV2_URID,
    pub nominal_block: LV2_URID,
    pub frame: LV2_URID,
    pub channels: LV2_URID,
    pub values: LV2_URID,
    pub position: LV2_URID,
}

/// A host's URID map
pub struct Mapper<'a>(pub &'a LV2_URID_Map);

impl Mapper<'_> {
    pub fn map(&self, uri: &str) -> LV2_URID {
        let Some(f) = self.0.map else { return 0 };
        let Ok(c) = CString::new(uri) else { return 0 };
        // SAFETY: the host's map function with its handle and a valid C
        // string, as the URID specification defines
        unsafe { f(self.0.handle, c.as_ptr()) }
    }
}

const ATOM: &str = "http://lv2plug.in/ns/ext/atom#";
const TIME: &str = "http://lv2plug.in/ns/ext/time#";
const BUF: &str = "http://lv2plug.in/ns/ext/buf-size#";

impl Urids {
    pub fn new(m: &Mapper<'_>) -> Self {
        let atom = |s: &str| m.map(&format!("{ATOM}{s}"));
        let time = |s: &str| m.map(&format!("{TIME}{s}"));
        let view = |s: &str| m.map(&format!("{}{s}", orx_plugin::view::NS));
        Self {
            atom_sequence: atom("Sequence"),
            atom_object: atom("Object"),
            atom_blank: atom("Blank"),
            atom_resource: atom("Resource"),
            atom_float: atom("Float"),
            atom_double: atom("Double"),
            atom_int: atom("Int"),
            atom_long: atom("Long"),
            atom_bool: atom("Bool"),
            atom_vector: atom("Vector"),
            atom_chunk: atom("Chunk"),
            atom_string: atom("String"),
            midi_event: m.map("http://lv2plug.in/ns/ext/midi#MidiEvent"),
            time_position: time("Position"),
            time_bpm: time("beatsPerMinute"),
            time_speed: time("speed"),
            time_bar_beat: time("barBeat"),
            time_beats_per_bar: time("beatsPerBar"),
            time_beat_unit: time("beatUnit"),
            max_block: m.map(&format!("{BUF}maxBlockLength")),
            nominal_block: m.map(&format!("{BUF}nominalBlockLength")),
            frame: view("Frame"),
            channels: view("channels"),
            values: view("values"),
            position: view("position"),
        }
    }
}
