//! The LV2 entry points of a plugin, generic over [`Plugin`].
//!
//! Every `extern "C"` function catches panics: unwinding into the host is
//! undefined behaviour. A panic in `run` (a bug) leaves the instance
//! passing its input through unchanged from then on, so a live rig keeps
//! its signal.

use crate::atom::{SequenceWriter, read_sequence};
use crate::sys::*;
use crate::uris::{self, Mapper, Urids};
use orx_plugin::dynamic::Entry;
use orx_plugin::process::StreamSink;
use orx_plugin::runner::{MAX_CHANNELS, Runner};
use orx_plugin::{Config, Event, Plugin, PortKind};
use std::ffi::{CStr, CString, c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

/// Block size assumed when the host gives none
pub const DEFAULT_MAX_BLOCK: usize = 8192;
/// Most stream ports per plugin
const MAX_STREAMS: usize = 8;

pub(crate) struct Instance<P: Plugin> {
    pub(crate) runner: Runner<P>,
    ports: Box<[*mut c_void]>,
    pub(crate) urids: Urids,
    pub(crate) map: Option<LV2_URID_Map>,
    audio_in: Box<[usize]>,
    audio_out: Box<[usize]>,
    controls_in: Box<[usize]>,
    controls_out: Box<[usize]>,
    events_in: Option<usize>,
    streams: Box<[usize]>,
    /// Copies of inputs that alias an output, or silence for unconnected
    /// ones (one per input, `max_block` long)
    scratch_in: Box<[Box<[f32]>]>,
    /// Destination of unconnected outputs
    scratch_out: Box<[Box<[f32]>]>,
    max_block: usize,
    poisoned: bool,
}

/// Descriptor of a plugin with its owned URI
pub struct Descriptor {
    _uri: CString,
    pub raw: LV2_Descriptor,
}

// SAFETY: the descriptor is immutable after creation; its pointers are to
// its own CString and to functions
unsafe impl Send for Descriptor {}
// SAFETY: as above
unsafe impl Sync for Descriptor {}

/// The LV2 descriptor of `P`
pub fn descriptor<P: Plugin>() -> Descriptor {
    let uri = CString::new(P::INFO.uri).unwrap_or_default();
    let raw = LV2_Descriptor {
        uri: uri.as_ptr(),
        instantiate: Some(instantiate::<P>),
        connect_port: Some(connect_port::<P>),
        activate: Some(activate::<P>),
        run: Some(run::<P>),
        deactivate: Some(deactivate::<P>),
        cleanup: Some(cleanup::<P>),
        extension_data: Some(extension_data::<P>),
    };
    Descriptor { _uri: uri, raw }
}

/// Does `P` need the host's URID map?
pub fn needs_map<P: Plugin>() -> bool {
    entry_needs_map(&Entry::of::<P>())
}

/// Does this plugin need the host's URID map? (state keys and atom types
/// are URIDs)
pub fn entry_needs_map(entry: &Entry) -> bool {
    entry.has_state || entry.info.ports.iter().any(|p| matches!(p.kind, PortKind::Events { .. } | PortKind::Stream(_)))
}

struct Features {
    map: Option<LV2_URID_Map>,
    options: *const LV2_Options_Option,
}

/// # Safety
///
/// `features` is null or a null-terminated array of valid features.
unsafe fn features(features: *const *const LV2_Feature) -> Features {
    let mut out = Features { map: None, options: ptr::null() };
    if features.is_null() {
        return out;
    }
    let mut i = 0;
    loop {
        // SAFETY: null-terminated array (caller)
        let f = unsafe { *features.add(i) };
        if f.is_null() {
            break;
        }
        // SAFETY: a valid feature (caller)
        let f = unsafe { &*f };
        if !f.uri.is_null() {
            // SAFETY: feature URIs are C strings
            let uri = unsafe { CStr::from_ptr(f.uri) };
            if uri == uris::URID_MAP && !f.data.is_null() {
                // SAFETY: the URID map feature data is an LV2_URID_Map
                out.map = Some(unsafe { ptr::read(f.data.cast::<LV2_URID_Map>()) });
            } else if uri == uris::OPTIONS {
                out.options = f.data.cast();
            }
        }
        i += 1;
    }
    out
}

/// Largest block from the options (max, else nominal)
///
/// # Safety
///
/// `options` is null or an array of options ended by a zero key.
unsafe fn block_from_options(options: *const LV2_Options_Option, u: &Urids) -> Option<usize> {
    if options.is_null() || u.max_block == 0 {
        return None;
    }
    let (mut max, mut nominal) = (None, None);
    let mut i = 0;
    loop {
        // SAFETY: array ended by a zero key (caller)
        let o = unsafe { &*options.add(i) };
        if o.key == 0 {
            break;
        }
        if (o.key == u.max_block || o.key == u.nominal_block) && !o.value.is_null() {
            let v = if o.type_ == u.atom_int && o.size >= 4 {
                // SAFETY: an Int option points to an i32
                Some(i64::from(unsafe { ptr::read_unaligned(o.value.cast::<i32>()) }))
            } else if o.type_ == u.atom_long && o.size >= 8 {
                // SAFETY: a Long option points to an i64
                Some(unsafe { ptr::read_unaligned(o.value.cast::<i64>()) })
            } else {
                None
            };
            let v = v.filter(|v| *v > 0).map(|v| v as usize);
            if o.key == u.max_block {
                max = v;
            } else {
                nominal = v;
            }
        }
        i += 1;
    }
    max.or(nominal)
}

impl<P: Plugin> Instance<P> {
    fn new(sample_rate: f64, f: Features) -> Option<Self> {
        if needs_map::<P>() && f.map.is_none() {
            return None;
        }
        let urids = f.map.as_ref().map(|m| Urids::new(&Mapper(m))).unwrap_or_default();
        // SAFETY: options from the host's feature array
        let max_block = unsafe { block_from_options(f.options, &urids) }.unwrap_or(DEFAULT_MAX_BLOCK).clamp(1, 1 << 16);
        let info = P::INFO;
        let idx = |f: fn(&PortKind) -> bool| info.indices(f).collect::<Box<[usize]>>();
        let audio_in = idx(|k| matches!(k, PortKind::AudioIn));
        let audio_out = idx(|k| matches!(k, PortKind::AudioOut));
        if audio_in.len() > MAX_CHANNELS || audio_out.len() > MAX_CHANNELS {
            return None;
        }
        let streams = idx(|k| matches!(k, PortKind::Stream(_)));
        if streams.len() > MAX_STREAMS {
            return None;
        }
        Some(Self {
            runner: Runner::new(Config { sample_rate, max_block }),
            ports: vec![ptr::null_mut(); info.ports.len()].into_boxed_slice(),
            urids,
            map: f.map,
            scratch_in: audio_in.iter().map(|_| orx_dsp::alloc::zeroed(max_block)).collect(),
            scratch_out: audio_out.iter().map(|_| orx_dsp::alloc::zeroed(max_block)).collect(),
            audio_in,
            audio_out,
            controls_in: idx(|k| matches!(k, PortKind::Param(_) | PortKind::Enabled)),
            controls_out: idx(|k| matches!(k, PortKind::Meter(_) | PortKind::Latency)),
            events_in: info.ports.iter().position(|p| matches!(p.kind, PortKind::Events { .. })),
            streams,
            max_block,
            poisoned: false,
        })
    }

    /// # Safety
    ///
    /// Connected ports point to buffers of at least `n` frames (audio) or
    /// one `f32` (control), and atom buffers per the atom specification.
    unsafe fn run(&mut self, n: usize) {
        let ports = &self.ports;
        for &i in self.controls_in.iter() {
            let p = ports[i].cast::<f32>();
            if !p.is_null() {
                // SAFETY: control input buffer (caller)
                self.runner.set_param(i, unsafe { p.read() });
            }
        }

        let mut sink =
            Sink { writers: [const { None }; MAX_STREAMS], ports: [usize::MAX; MAX_STREAMS], urids: self.urids };
        for (k, &i) in self.streams.iter().enumerate() {
            sink.ports[k] = i;
            // SAFETY: atom output buffer (caller)
            sink.writers[k] = unsafe { SequenceWriter::start(ports[i].cast(), &self.urids) };
        }
        let events = self.events_in.map_or(ptr::null(), |i| ports[i].cast::<LV2_Atom_Sequence>().cast_const());

        let mut start = 0;
        while start < n {
            let len = (n - start).min(self.max_block);
            // Events of this piece, relative to it
            let runner = &mut self.runner;
            // SAFETY: atom input buffer (caller)
            unsafe {
                read_sequence(events, &self.urids, |frame, kind| {
                    let frame = frame.max(0) as usize;
                    // Late events (frame >= n) go to the last piece
                    if frame >= start && (frame < start + len || start + len == n) {
                        let frame = (frame - start).min(len - 1) as u32;
                        runner.push_event(Event { frame, kind });
                    }
                });
            }

            let mut ins: [&[f32]; MAX_CHANNELS] = [&[]; MAX_CHANNELS];
            let mut outs: [&mut [f32]; MAX_CHANNELS] = Default::default();
            let out_ptrs: [*mut f32; MAX_CHANNELS] =
                std::array::from_fn(|k| self.audio_out.get(k).map_or(ptr::null_mut(), |&i| ports[i].cast::<f32>()));
            let overlaps = |p: *const f32| {
                out_ptrs.iter().take(self.audio_out.len()).any(|o| {
                    let (a, b) = (p as usize, *o as usize);
                    !o.is_null() && a < b + 4 * n && b < a + 4 * n
                })
            };
            for ((slot, &port), scratch) in ins.iter_mut().zip(self.audio_in.iter()).zip(self.scratch_in.iter_mut()) {
                let p = ports[port].cast::<f32>().cast_const();
                let scratch = &mut scratch[..len];
                if p.is_null() {
                    scratch.fill(0.0);
                    *slot = scratch;
                } else if overlaps(p) {
                    // In-place host: copy before the outputs are written
                    // SAFETY: `len` readable frames at p + start (caller)
                    unsafe { ptr::copy_nonoverlapping(p.add(start), scratch.as_mut_ptr(), len) };
                    *slot = scratch;
                } else {
                    // SAFETY: readable, and no output overlaps it
                    *slot = unsafe { std::slice::from_raw_parts(p.add(start), len) };
                }
            }
            for ((slot, &p), scratch) in outs.iter_mut().zip(out_ptrs.iter()).zip(self.scratch_out.iter_mut()) {
                *slot = if p.is_null() {
                    &mut scratch[..len]
                } else {
                    // SAFETY: writable (caller); outputs do not overlap each
                    // other, and overlapping inputs were copied
                    unsafe { std::slice::from_raw_parts_mut(p.add(start), len) }
                };
            }
            let (n_in, n_out) = (self.audio_in.len(), self.audio_out.len());
            self.runner.process(len, &ins[..n_in], &mut outs[..n_out], &mut sink);
            start += len;
        }

        for &i in self.controls_out.iter() {
            let p = ports[i].cast::<f32>();
            if !p.is_null() {
                // SAFETY: control output buffer (caller)
                unsafe { p.write(self.runner.meter(i)) };
            }
        }
    }

    /// After a panic: copy inputs to outputs (raw pointers only)
    ///
    /// # Safety
    ///
    /// As [`Instance::run`].
    unsafe fn pass_through(&self, n: usize) {
        for (k, &o) in self.audio_out.iter().enumerate() {
            let out = self.ports[o].cast::<f32>();
            if out.is_null() {
                continue;
            }
            let src = self.audio_in.get(k.min(self.audio_in.len().saturating_sub(1))).map(|&i| self.ports[i]);
            match src.map(|p| p.cast::<f32>().cast_const()) {
                // SAFETY: both hold n frames; `copy` allows overlap
                Some(inp) if !inp.is_null() => unsafe { ptr::copy(inp, out, n) },
                // SAFETY: n writable frames
                _ => unsafe { ptr::write_bytes(out, 0, n) },
            }
        }
    }
}

struct Sink {
    writers: [Option<SequenceWriter>; MAX_STREAMS],
    ports: [usize; MAX_STREAMS],
    urids: Urids,
}

impl StreamSink for Sink {
    fn connected(&self, index: usize) -> bool {
        self.ports.iter().zip(&self.writers).any(|(p, w)| *p == index && w.is_some())
    }

    fn send(&mut self, index: usize, frame: u32, channels: u32, position: f32, values: &[f32]) -> bool {
        let Some(k) = self.ports.iter().position(|p| *p == index) else { return false };
        let urids = self.urids;
        self.writers[k].as_mut().is_some_and(|w| w.frame(&urids, frame, channels, position, values))
    }
}

/// Run `f`, turning a panic into `fallback`
fn guard<R>(fallback: R, f: impl FnOnce() -> R) -> R {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

unsafe extern "C" fn instantiate<P: Plugin>(
    _descriptor: *const LV2_Descriptor,
    sample_rate: f64,
    _bundle_path: *const c_char,
    features_ptr: *const *const LV2_Feature,
) -> LV2_Handle {
    guard(ptr::null_mut(), || {
        // SAFETY: the host's feature array
        let f = unsafe { features(features_ptr) };
        match Instance::<P>::new(sample_rate, f) {
            Some(i) => Box::into_raw(Box::new(i)).cast(),
            None => ptr::null_mut(),
        }
    })
}

/// # Safety
///
/// `h` is a handle from `instantiate::<P>`, not yet cleaned up.
unsafe fn instance<'a, P: Plugin>(h: LV2_Handle) -> Option<&'a mut Instance<P>> {
    // SAFETY: (caller)
    unsafe { h.cast::<Instance<P>>().as_mut() }
}

unsafe extern "C" fn connect_port<P: Plugin>(h: LV2_Handle, port: u32, data: *mut c_void) {
    guard((), || {
        // SAFETY: LV2 contract
        if let Some(i) = unsafe { instance::<P>(h) }
            && let Some(slot) = i.ports.get_mut(port as usize)
        {
            *slot = data;
        }
    });
}

unsafe extern "C" fn activate<P: Plugin>(h: LV2_Handle) {
    guard((), || {
        // SAFETY: LV2 contract
        if let Some(i) = unsafe { instance::<P>(h) } {
            i.runner.activate();
        }
    });
}

unsafe extern "C" fn run<P: Plugin>(h: LV2_Handle, n: u32) {
    // SAFETY: LV2 contract
    let Some(i) = (unsafe { instance::<P>(h) }) else { return };
    let n = n as usize;
    if !i.poisoned {
        // SAFETY: LV2 contract: ports connected to valid buffers
        let ok = catch_unwind(AssertUnwindSafe(|| unsafe { i.run(n) })).is_ok();
        if ok {
            return;
        }
        i.poisoned = true;
    }
    // SAFETY: as above
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe { i.pass_through(n) }));
}

unsafe extern "C" fn deactivate<P: Plugin>(_h: LV2_Handle) {}

unsafe extern "C" fn cleanup<P: Plugin>(h: LV2_Handle) {
    if h.is_null() {
        return;
    }
    guard((), || {
        // SAFETY: created by Box::into_raw in instantiate, cleaned once
        drop(unsafe { Box::from_raw(h.cast::<Instance<P>>()) });
    });
}

unsafe extern "C" fn extension_data<P: Plugin>(uri: *const c_char) -> *const c_void {
    guard(ptr::null(), || {
        if uri.is_null() || !P::HAS_STATE {
            return ptr::null();
        }
        // SAFETY: extension URIs are C strings
        if unsafe { CStr::from_ptr(uri) } == uris::STATE_INTERFACE {
            crate::state::interface::<P>()
        } else {
            ptr::null()
        }
    })
}
