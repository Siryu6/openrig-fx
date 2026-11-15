//! Reading event sequences and writing view frames, on raw host buffers.
//!
//! Everything is bounds-checked against the sizes the host gave: a
//! malformed sequence ends the parse, a full output buffer drops frames.

use crate::sys::{
    LV2_Atom, LV2_Atom_Event, LV2_Atom_Object_Body, LV2_Atom_Property_Body, LV2_Atom_Sequence, LV2_Atom_Vector_Body,
    pad8,
};
use crate::uris::Urids;
use orx_plugin::{EventKind, Position};
use std::mem::size_of;
use std::ptr;

/// Visit the events of an input sequence: `f(frame, kind)`, in order
///
/// # Safety
///
/// `seq` is null or points to a sequence the host wrote, readable for
/// `8 + seq.atom.size` bytes.
pub unsafe fn read_sequence(seq: *const LV2_Atom_Sequence, u: &Urids, mut f: impl FnMut(i64, EventKind)) {
    if seq.is_null() {
        return;
    }
    // SAFETY: non-null, points to at least an atom header (caller)
    let header = unsafe { ptr::read_unaligned(seq) };
    if header.atom.type_ != u.atom_sequence || (header.atom.size as usize) < size_of::<LV2_Atom>() {
        return;
    }
    let base = seq.cast::<u8>();
    let end = size_of::<LV2_Atom>() + header.atom.size as usize;
    let mut at = size_of::<LV2_Atom_Sequence>();
    while at + size_of::<LV2_Atom_Event>() <= end {
        // SAFETY: the event header is within the sequence (checked above)
        let ev = unsafe { ptr::read_unaligned(base.add(at).cast::<LV2_Atom_Event>()) };
        let body_at = at + size_of::<LV2_Atom_Event>();
        let size = ev.body.size as usize;
        if body_at + size > end {
            return;
        }
        // SAFETY: body_at..body_at + size is within the sequence
        let body = unsafe { std::slice::from_raw_parts(base.add(body_at), size) };
        if let Some(kind) = parse_event(ev.body.type_, body, u) {
            f(ev.frames, kind);
        }
        at = body_at + pad8(size);
    }
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_ne_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn number(type_: u32, b: &[u8], u: &Urids) -> Option<f64> {
    let get = |n: usize| b.get(..n);
    if type_ == u.atom_float {
        Some(f64::from(f32::from_ne_bytes(get(4)?.try_into().ok()?)))
    } else if type_ == u.atom_double {
        Some(f64::from_ne_bytes(get(8)?.try_into().ok()?))
    } else if type_ == u.atom_int || type_ == u.atom_bool {
        Some(f64::from(i32::from_ne_bytes(get(4)?.try_into().ok()?)))
    } else if type_ == u.atom_long {
        Some(i64::from_ne_bytes(get(8)?.try_into().ok()?) as f64)
    } else {
        None
    }
}

fn parse_event(type_: u32, body: &[u8], u: &Urids) -> Option<EventKind> {
    if type_ == u.midi_event {
        if body.is_empty() || body.len() > 3 {
            return None;
        }
        let mut data = [0u8; 3];
        data[..body.len()].copy_from_slice(body);
        return Some(EventKind::Midi { data, len: body.len() as u8 });
    }
    if type_ != u.atom_object && type_ != u.atom_blank && type_ != u.atom_resource {
        return None;
    }
    if u32_at(body, 4)? != u.time_position {
        return None;
    }
    let mut p = Position::default();
    let mut at = size_of::<LV2_Atom_Object_Body>();
    while at + size_of::<LV2_Atom_Property_Body>() <= body.len() {
        let key = u32_at(body, at)?;
        let size = u32_at(body, at + 8)? as usize;
        let vtype = u32_at(body, at + 12)?;
        let value_at = at + size_of::<LV2_Atom_Property_Body>();
        let value = body.get(value_at..value_at + size)?;
        if let Some(v) = number(vtype, value, u).filter(|v| v.is_finite()) {
            if key == u.time_bpm {
                p.bpm = Some(v as f32);
            } else if key == u.time_speed {
                p.speed = Some(v as f32);
            } else if key == u.time_bar_beat {
                p.bar_beat = Some(v as f32);
            } else if key == u.time_beats_per_bar {
                p.beats_per_bar = Some(v as f32);
            } else if key == u.time_beat_unit {
                p.beat_unit = Some(v as i32);
            }
        }
        at = value_at + pad8(size);
    }
    Some(EventKind::Position(p))
}

/// An output sequence being filled
pub struct SequenceWriter {
    base: *mut u8,
    /// Bytes available after the atom header (the host's capacity)
    capacity: usize,
    /// Bytes used after the atom header
    used: usize,
}

impl SequenceWriter {
    /// Start an empty sequence in the host buffer
    ///
    /// # Safety
    ///
    /// `seq` is null or points to a buffer whose `atom.size` the host set
    /// to its capacity (after the header), writable for that many bytes.
    pub unsafe fn start(seq: *mut LV2_Atom_Sequence, u: &Urids) -> Option<Self> {
        if seq.is_null() {
            return None;
        }
        // SAFETY: valid header (caller)
        let capacity = unsafe { ptr::read_unaligned(seq) }.atom.size as usize;
        if capacity < size_of::<LV2_Atom_Sequence>() - size_of::<LV2_Atom>() {
            return None;
        }
        let mut w = Self { base: seq.cast(), capacity, used: 8 };
        w.write_header(u.atom_sequence);
        Some(w)
    }

    fn write_header(&mut self, sequence: u32) {
        let header = LV2_Atom_Sequence {
            atom: LV2_Atom { size: self.used as u32, type_: sequence },
            body: crate::sys::LV2_Atom_Sequence_Body { unit: 0, pad: 0 },
        };
        // SAFETY: the header fits (checked in `start`)
        unsafe { ptr::write_unaligned(self.base.cast::<LV2_Atom_Sequence>(), header) };
    }

    /// Append an `openrig:Frame` object at `frame`; false when it does not fit
    pub fn frame(&mut self, u: &Urids, frame: u32, channels: u32, position: f32, values: &[f32]) -> bool {
        let prop = size_of::<LV2_Atom_Property_Body>();
        let vector = size_of::<LV2_Atom_Vector_Body>() + 4 * values.len();
        let object = size_of::<LV2_Atom_Object_Body>() + 2 * pad8(prop + 4) + pad8(prop + vector);
        let event = size_of::<LV2_Atom_Event>() + object;
        if self.used + pad8(event) > self.capacity {
            return false;
        }
        let mut out = Cursor { base: self.base, at: size_of::<LV2_Atom>() + self.used };
        out.put(LV2_Atom_Event {
            frames: i64::from(frame),
            body: LV2_Atom { size: object as u32, type_: u.atom_object },
        });
        out.put(LV2_Atom_Object_Body { id: 0, otype: u.frame });
        let property = |key, size: usize, type_| LV2_Atom_Property_Body {
            key,
            context: 0,
            value: LV2_Atom { size: size as u32, type_ },
        };
        out.put(property(u.channels, 4, u.atom_int));
        out.put(channels as i32);
        out.pad();
        out.put(property(u.position, 4, u.atom_float));
        out.put(position);
        out.pad();
        out.put(property(u.values, vector, u.atom_vector));
        out.put(LV2_Atom_Vector_Body { child_size: 4, child_type: u.atom_float });
        for v in values {
            out.put(*v);
        }
        out.pad();
        self.used += pad8(event);
        self.write_header(u.atom_sequence);
        true
    }
}

struct Cursor {
    base: *mut u8,
    at: usize,
}

impl Cursor {
    fn put<T: Copy>(&mut self, v: T) {
        // SAFETY: `frame` checked that the whole event fits the capacity
        unsafe { ptr::write_unaligned(self.base.add(self.at).cast::<T>(), v) };
        self.at += size_of::<T>();
    }

    fn pad(&mut self) {
        while !self.at.is_multiple_of(8) {
            self.put(0u8);
        }
    }
}

/// Bytes of an output sequence holding one frame of `values` values
pub const fn frame_event_size(values: usize) -> usize {
    let prop = size_of::<LV2_Atom_Property_Body>();
    let vector = size_of::<LV2_Atom_Vector_Body>() + 4 * values;
    size_of::<LV2_Atom_Event>() + size_of::<LV2_Atom_Object_Body>() + 2 * pad8(prop + 4) + pad8(prop + vector)
}
