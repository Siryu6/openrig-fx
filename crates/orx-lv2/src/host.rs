//! A minimal LV2 host, to test plugins through their C entry points.
//!
//! It provides what OpenRig's host provides (URID map, options with the
//! block length, bounded block length) and drives one instance: control
//! values, audio in and out (separate or in place), transport events, view
//! frames, state. Buffers are allocated up front: [`Instance::run`] does
//! not allocate, so it can run inside an allocation trap.

use crate::sys::*;
use orx_plugin::{PluginInfo, PortKind};
use std::ffi::{CStr, CString, c_char, c_void};
use std::mem::size_of;
use std::ptr;
use std::sync::Mutex;

/// Bytes of each atom buffer, as in OpenRig
pub const ATOM_CAPACITY: usize = 8192;

struct UridTable(Mutex<Vec<CString>>);

unsafe extern "C" fn map_uri(handle: *mut c_void, uri: *const c_char) -> LV2_URID {
    if handle.is_null() || uri.is_null() {
        return 0;
    }
    // SAFETY: handle is the host's UridTable, uri a C string
    let (table, uri) = unsafe { (&*handle.cast::<UridTable>(), CStr::from_ptr(uri)) };
    let Ok(mut uris) = table.0.lock() else { return 0 };
    if let Some(i) = uris.iter().position(|u| u.as_c_str() == uri) {
        return i as u32 + 1;
    }
    uris.push(uri.to_owned());
    uris.len() as u32
}

/// The host side: features shared by its instances
pub struct Host {
    table: Box<UridTable>,
    map: Box<LV2_URID_Map>,
    block: Box<i32>,
    options: Box<[LV2_Options_Option; 3]>,
    feature_list: Box<[LV2_Feature; 4]>,
    feature_ptrs: Box<[*const LV2_Feature; 5]>,
    pub sample_rate: f64,
    pub max_block: usize,
}

impl Host {
    pub fn new(sample_rate: f64, max_block: usize) -> Box<Self> {
        let table = Box::new(UridTable(Mutex::new(Vec::new())));
        let map =
            Box::new(LV2_URID_Map { handle: (&*table as *const UridTable).cast_mut().cast(), map: Some(map_uri) });
        let mut host = Box::new(Self {
            table,
            map,
            block: Box::new(max_block as i32),
            // SAFETY: zeroed options are valid (and the terminator)
            options: Box::new(unsafe { std::mem::zeroed() }),
            // SAFETY: filled below before use
            feature_list: Box::new(unsafe { std::mem::zeroed() }),
            feature_ptrs: Box::new([ptr::null(); 5]),
            sample_rate,
            max_block,
        });
        let int = host.urid("http://lv2plug.in/ns/ext/atom#Int");
        let block_ptr: *const i32 = &*host.block;
        for (k, key) in ["maxBlockLength", "nominalBlockLength"].into_iter().enumerate() {
            host.options[k] = LV2_Options_Option {
                context: 0,
                subject: 0,
                key: host.urid(&format!("http://lv2plug.in/ns/ext/buf-size#{key}")),
                size: 4,
                type_: int,
                value: block_ptr.cast(),
            };
        }
        let map_ptr: *mut LV2_URID_Map = &mut *host.map;
        let options_ptr: *mut LV2_Options_Option = host.options.as_mut_ptr();
        let uris: [&CStr; 4] = [
            c"http://lv2plug.in/ns/ext/urid#map",
            c"http://lv2plug.in/ns/ext/options#options",
            c"http://lv2plug.in/ns/ext/buf-size#boundedBlockLength",
            c"http://lv2plug.in/ns/lv2core#isLive",
        ];
        for (k, uri) in uris.iter().enumerate() {
            let data: *mut c_void = match k {
                0 => map_ptr.cast(),
                1 => options_ptr.cast(),
                _ => ptr::null_mut(),
            };
            host.feature_list[k] = LV2_Feature { uri: uri.as_ptr(), data };
        }
        for k in 0..4 {
            host.feature_ptrs[k] = &host.feature_list[k];
        }
        host
    }

    /// The URID of `uri` in this host's map
    pub fn urid(&self, uri: &str) -> LV2_URID {
        let c = CString::new(uri).unwrap_or_default();
        // SAFETY: our own map function and table
        unsafe { map_uri((&*self.table as *const UridTable).cast_mut().cast(), c.as_ptr()) }
    }

    /// The URI of `urid`
    pub fn uri(&self, urid: LV2_URID) -> Option<String> {
        let uris = self.table.0.lock().ok()?;
        uris.get((urid as usize).checked_sub(1)?).map(|c| c.to_string_lossy().into_owned())
    }

    /// Instantiate the plugin of `descriptor`, described by `info`
    ///
    /// # Safety
    ///
    /// `descriptor` is a valid LV2 descriptor whose ports match `info`.
    pub unsafe fn instantiate<'h>(
        &'h self,
        descriptor: *const LV2_Descriptor,
        info: &'static PluginInfo,
        in_place: bool,
    ) -> Option<Instance<'h>> {
        // SAFETY: (caller)
        let d = unsafe { descriptor.as_ref()? };
        let bundle = c"/nonexistent/";
        // SAFETY: LV2 instantiate with a null-terminated feature array
        let handle = unsafe { (d.instantiate?)(d, self.sample_rate, bundle.as_ptr(), self.feature_ptrs.as_ptr()) };
        if handle.is_null() {
            return None;
        }
        let ports = info
            .ports
            .iter()
            .map(|p| match p.kind {
                PortKind::AudioIn | PortKind::AudioOut => Buffer::Audio(vec![0.0; self.max_block]),
                PortKind::Events { .. } | PortKind::Stream(_) => Buffer::Atom(vec![0u64; ATOM_CAPACITY / 8]),
                PortKind::Param(param) => Buffer::Control(Box::new(param.default)),
                PortKind::Enabled => Buffer::Control(Box::new(1.0)),
                PortKind::Meter(_) | PortKind::Latency => Buffer::Control(Box::new(0.0)),
            })
            .collect();
        let mut instance = Instance { host: self, descriptor: d, handle, info, ports, events_used: 0, in_place };
        instance.connect();
        Some(instance)
    }
}

enum Buffer {
    Audio(Vec<f32>),
    Control(Box<f32>),
    /// u64 for the 8-byte alignment of atoms
    Atom(Vec<u64>),
}

/// A view frame read from a stream output
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub time: i64,
    pub channels: i32,
    pub position: f32,
    pub values: Vec<f32>,
}

pub struct Instance<'h> {
    host: &'h Host,
    descriptor: &'h LV2_Descriptor,
    handle: LV2_Handle,
    info: &'static PluginInfo,
    ports: Vec<Buffer>,
    /// Bytes of events written in the input sequence
    events_used: usize,
    in_place: bool,
}

impl Instance<'_> {
    fn connect(&mut self) {
        let Some(connect) = self.descriptor.connect_port else { return };
        let outs: Vec<usize> = self.info.indices(|k| matches!(k, PortKind::AudioOut)).collect();
        let ins: Vec<usize> = self.info.indices(|k| matches!(k, PortKind::AudioIn)).collect();
        for i in 0..self.ports.len() {
            let target = if self.in_place {
                // Outputs share the buffer of the input of the same rank
                outs.iter().position(|o| *o == i).and_then(|k| ins.get(k)).copied().unwrap_or(i)
            } else {
                i
            };
            let data: *mut c_void = match &mut self.ports[target] {
                Buffer::Audio(v) => v.as_mut_ptr().cast(),
                Buffer::Control(v) => (&mut **v as *mut f32).cast(),
                Buffer::Atom(v) => v.as_mut_ptr().cast(),
            };
            // SAFETY: a live instance and a buffer that outlives it
            unsafe { connect(self.handle, i as u32, data) };
        }
    }

    pub fn activate(&mut self) {
        if let Some(f) = self.descriptor.activate {
            // SAFETY: live instance
            unsafe { f(self.handle) };
        }
    }

    /// Set the control input at port `index`
    pub fn set(&mut self, index: usize, value: f32) {
        if let Some(Buffer::Control(v)) = self.ports.get_mut(index) {
            **v = value;
        }
    }

    /// Value of the control port at `index`
    pub fn get(&self, index: usize) -> f32 {
        match self.ports.get(index) {
            Some(Buffer::Control(v)) => **v,
            _ => f32::NAN,
        }
    }

    fn audio(&mut self, kind: fn(&PortKind) -> bool, k: usize) -> &mut [f32] {
        let i = self.info.indices(kind).nth(k).unwrap_or(usize::MAX);
        match self.ports.get_mut(i) {
            Some(Buffer::Audio(v)) => v,
            _ => &mut [],
        }
    }

    /// The `k`-th audio input buffer
    pub fn input(&mut self, k: usize) -> &mut [f32] {
        self.audio(|p| matches!(p, PortKind::AudioIn), k)
    }

    /// The `k`-th audio output buffer (the input's when in place)
    pub fn output(&mut self, k: usize) -> &mut [f32] {
        if self.in_place {
            let n = self.info.audio_inputs();
            return self.input(k.min(n.saturating_sub(1)));
        }
        self.audio(|p| matches!(p, PortKind::AudioOut), k)
    }

    fn atom_in(&mut self) -> Option<&mut [u8]> {
        let i = self.info.ports.iter().position(|p| matches!(p.kind, PortKind::Events { .. }))?;
        match self.ports.get_mut(i) {
            Some(Buffer::Atom(v)) => {
                // SAFETY: viewing u64 storage as bytes
                Some(unsafe { std::slice::from_raw_parts_mut(v.as_mut_ptr().cast(), v.len() * 8) })
            }
            _ => None,
        }
    }

    /// Queue a transport position for the next run, as OpenRig sends it
    pub fn push_position(&mut self, frame: i64, bpm: f32, speed: f32) {
        let h = self.host;
        let float = h.urid("http://lv2plug.in/ns/ext/atom#Float");
        let long = h.urid("http://lv2plug.in/ns/ext/atom#Long");
        let int = h.urid("http://lv2plug.in/ns/ext/atom#Int");
        let time = |s: &str| h.urid(&format!("http://lv2plug.in/ns/ext/time#{s}"));
        let mut body = Vec::new();
        body.extend_from_slice(&0u32.to_ne_bytes());
        body.extend_from_slice(&time("Position").to_ne_bytes());
        let mut prop = |key: u32, type_: u32, value: &[u8]| {
            for x in [key, 0, value.len() as u32, type_] {
                body.extend_from_slice(&x.to_ne_bytes());
            }
            body.extend_from_slice(value);
            body.resize(pad8(body.len()), 0);
        };
        prop(time("speed"), float, &speed.to_ne_bytes());
        prop(time("beatsPerMinute"), float, &bpm.to_ne_bytes());
        prop(time("bar"), long, &0i64.to_ne_bytes());
        prop(time("barBeat"), float, &0f32.to_ne_bytes());
        prop(time("beatUnit"), int, &4i32.to_ne_bytes());
        prop(time("beatsPerBar"), float, &4f32.to_ne_bytes());
        let object = h.urid("http://lv2plug.in/ns/ext/atom#Object");
        self.push_event(frame, object, &body);
    }

    /// Queue a MIDI message for the next run
    pub fn push_midi(&mut self, frame: i64, data: &[u8]) {
        let midi = self.host.urid("http://lv2plug.in/ns/ext/midi#MidiEvent");
        self.push_event(frame, midi, data);
    }

    fn push_event(&mut self, frame: i64, type_: u32, body: &[u8]) {
        let used = self.events_used;
        let Some(buf) = self.atom_in() else { return };
        let at = 16 + used;
        if at + 16 + body.len() > buf.len() {
            return;
        }
        buf[at..at + 8].copy_from_slice(&frame.to_ne_bytes());
        buf[at + 8..at + 12].copy_from_slice(&(body.len() as u32).to_ne_bytes());
        buf[at + 12..at + 16].copy_from_slice(&type_.to_ne_bytes());
        buf[at + 16..at + 16 + body.len()].copy_from_slice(body);
        self.events_used += pad8(16 + body.len());
    }

    /// Run `n` frames (does not allocate)
    pub fn run(&mut self, n: usize) {
        let sequence = self.host_sequence_urid();
        let chunk = self.host_chunk_urid();
        let used = self.events_used;
        for (port, buf) in self.info.ports.iter().zip(self.ports.iter_mut()) {
            let Buffer::Atom(v) = buf else { continue };
            let header = v.as_mut_ptr().cast::<LV2_Atom_Sequence>();
            let value = match port.kind {
                // Input: the queued events
                PortKind::Events { .. } => LV2_Atom_Sequence {
                    atom: LV2_Atom { size: (8 + used) as u32, type_: sequence },
                    body: LV2_Atom_Sequence_Body { unit: 0, pad: 0 },
                },
                // Output: the capacity, as OpenRig sets it
                _ => LV2_Atom_Sequence {
                    atom: LV2_Atom { size: (ATOM_CAPACITY - 8) as u32, type_: chunk },
                    body: LV2_Atom_Sequence_Body { unit: 0, pad: 0 },
                },
            };
            // SAFETY: the buffer holds ATOM_CAPACITY bytes
            unsafe { header.write(value) };
        }
        if let Some(run) = self.descriptor.run {
            // SAFETY: live, connected instance
            unsafe { run(self.handle, n as u32) };
        }
        self.events_used = 0;
    }

    fn host_sequence_urid(&self) -> u32 {
        // Mapped at host creation order: cheap lookups would allocate, so
        // the URIDs are found by scanning without allocating
        self.find_urid(c"http://lv2plug.in/ns/ext/atom#Sequence")
    }

    fn host_chunk_urid(&self) -> u32 {
        self.find_urid(c"http://lv2plug.in/ns/ext/atom#Chunk")
    }

    fn find_urid(&self, uri: &CStr) -> u32 {
        match self.host.table.0.lock() {
            Ok(uris) => uris.iter().position(|u| u.as_c_str() == uri).map_or(0, |i| i as u32 + 1),
            Err(_) => 0,
        }
    }

    /// Frames the plugin wrote to the stream output at port `index` in the
    /// last run
    pub fn frames(&self, index: usize) -> Vec<Frame> {
        let Some(Buffer::Atom(v)) = self.ports.get(index) else { return Vec::new() };
        // SAFETY: viewing u64 storage as bytes
        let b: &[u8] = unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), v.len() * 8) };
        let u32_at = |i: usize| u32::from_ne_bytes(b[i..i + 4].try_into().unwrap_or_default());
        let h = self.host;
        let (seq, object, frame, int, float, vector) = (
            h.urid("http://lv2plug.in/ns/ext/atom#Sequence"),
            h.urid("http://lv2plug.in/ns/ext/atom#Object"),
            h.urid(&format!("{}Frame", orx_plugin::view::NS)),
            h.urid("http://lv2plug.in/ns/ext/atom#Int"),
            h.urid("http://lv2plug.in/ns/ext/atom#Float"),
            h.urid("http://lv2plug.in/ns/ext/atom#Vector"),
        );
        let (k_channels, k_position, k_values) = (
            h.urid(&format!("{}channels", orx_plugin::view::NS)),
            h.urid(&format!("{}position", orx_plugin::view::NS)),
            h.urid(&format!("{}values", orx_plugin::view::NS)),
        );
        let mut out = Vec::new();
        if u32_at(4) != seq {
            return out;
        }
        let end = (8 + u32_at(0) as usize).min(b.len());
        let mut at = 16;
        while at + 16 <= end {
            let time = i64::from_ne_bytes(b[at..at + 8].try_into().unwrap_or_default());
            let (size, type_) = (u32_at(at + 8) as usize, u32_at(at + 12));
            let body = at + 16;
            if type_ == object && u32_at(body + 4) == frame {
                let mut f = Frame { time, channels: 0, position: f32::NAN, values: Vec::new() };
                let mut p = body + 8;
                while p + 16 <= body + size {
                    let (key, vsize, vtype) = (u32_at(p), u32_at(p + 8) as usize, u32_at(p + 12));
                    let v = p + 16;
                    if key == k_channels && vtype == int {
                        f.channels = u32_at(v) as i32;
                    } else if key == k_position && vtype == float {
                        f.position = f32::from_bits(u32_at(v));
                    } else if key == k_values && vtype == vector {
                        let count = (vsize - 8) / 4;
                        f.values = (0..count).map(|i| f32::from_bits(u32_at(v + 8 + 4 * i))).collect();
                    }
                    p = v + pad8(vsize);
                }
                out.push(f);
            }
            at = body + pad8(size);
        }
        out
    }

    /// The state interface, if the plugin has one
    fn state(&self) -> Option<&LV2_State_Interface> {
        let ext = self.descriptor.extension_data?;
        // SAFETY: LV2 extension_data with a C string
        let p = unsafe { ext(c"http://lv2plug.in/ns/ext/state#interface".as_ptr()) };
        // SAFETY: the state extension data is an LV2_State_Interface
        unsafe { p.cast::<LV2_State_Interface>().as_ref() }
    }

    /// Save the plugin's state: (key URI, type URI, bytes)
    pub fn save(&self) -> Option<Vec<(String, String, Vec<u8>)>> {
        let save = self.state()?.save?;
        let mut items: Vec<(u32, u32, Vec<u8>)> = Vec::new();
        unsafe extern "C" fn store(
            h: LV2_State_Handle,
            key: u32,
            v: *const c_void,
            size: usize,
            t: u32,
            _f: u32,
        ) -> u32 {
            // SAFETY: our item list, and `size` readable bytes at v
            let (items, bytes) = unsafe {
                (&mut *h.cast::<Vec<(u32, u32, Vec<u8>)>>(), std::slice::from_raw_parts(v.cast::<u8>(), size))
            };
            items.push((key, t, bytes.to_vec()));
            LV2_STATE_SUCCESS
        }
        // SAFETY: live instance, our store function and handle
        let status = unsafe {
            save(self.handle, Some(store), (&mut items as *mut Vec<(u32, u32, Vec<u8>)>).cast(), 0, ptr::null())
        };
        (status == LV2_STATE_SUCCESS).then(|| {
            items
                .into_iter()
                .map(|(k, t, b)| (self.host.uri(k).unwrap_or_default(), self.host.uri(t).unwrap_or_default(), b))
                .collect()
        })
    }

    /// Restore a state saved by [`Instance::save`]; the LV2 status
    pub fn restore(&mut self, items: &[(String, String, Vec<u8>)]) -> Option<u32> {
        let restore = self.state()?.restore?;
        let mapped: Vec<(u32, u32, Vec<u8>)> =
            items.iter().map(|(k, t, b)| (self.host.urid(k), self.host.urid(t), b.clone())).collect();
        unsafe extern "C" fn retrieve(
            h: LV2_State_Handle,
            key: u32,
            size: *mut usize,
            t: *mut u32,
            flags: *mut u32,
        ) -> *const c_void {
            // SAFETY: our item list
            let items = unsafe { &*h.cast::<Vec<(u32, u32, Vec<u8>)>>() };
            match items.iter().find(|(k, _, _)| *k == key) {
                Some((_, ty, b)) => {
                    // SAFETY: out-pointers given by the plugin
                    unsafe {
                        *size = b.len();
                        *t = *ty;
                        *flags = LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE;
                    }
                    b.as_ptr().cast()
                }
                None => ptr::null(),
            }
        }
        // SAFETY: live instance, our retrieve function and handle
        Some(unsafe {
            restore(
                self.handle,
                Some(retrieve),
                (&mapped as *const Vec<(u32, u32, Vec<u8>)>).cast_mut().cast(),
                0,
                ptr::null(),
            )
        })
    }
}

impl Drop for Instance<'_> {
    fn drop(&mut self) {
        // SAFETY: live instance, cleaned once
        unsafe {
            if let Some(f) = self.descriptor.deactivate {
                f(self.handle);
            }
            if let Some(f) = self.descriptor.cleanup {
                f(self.handle);
            }
        }
    }
}

const _: () = assert!(size_of::<LV2_Atom_Sequence>() == 16);
