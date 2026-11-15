//! The LV2 state interface, for plugins with `HAS_STATE`.
//!
//! Keys are `<plugin URI>#<key>`; values are stored as atoms (Chunk,
//! String, Float, Long), flagged POD and portable.

use crate::instance::Instance;
use crate::sys::*;
use crate::uris::{Mapper, Urids};
use orx_plugin::{Plugin, StateSink, StateSource, StateValue};
use std::ffi::c_void;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Iface<P>(PhantomData<P>);

impl<P: Plugin> Iface<P> {
    const IFACE: LV2_State_Interface = LV2_State_Interface { save: Some(save::<P>), restore: Some(restore::<P>) };
}

pub(crate) fn interface<P: Plugin>() -> *const c_void {
    let iface: &'static LV2_State_Interface = &Iface::<P>::IFACE;
    (iface as *const LV2_State_Interface).cast()
}

fn key_urid(map: &LV2_URID_Map, uri: &str, key: &str) -> u32 {
    Mapper(map).map(&format!("{uri}#{key}"))
}

struct Store<'a, P> {
    urids: Urids,
    map: &'a LV2_URID_Map,
    store: unsafe extern "C" fn(LV2_State_Handle, u32, *const c_void, usize, u32, u32) -> LV2_State_Status,
    handle: LV2_State_Handle,
    status: LV2_State_Status,
    _plugin: PhantomData<P>,
}

impl<P: Plugin> StateSink for Store<'_, P> {
    fn put(&mut self, key: &str, value: StateValue<'_>) {
        let u = &self.urids;
        let k = key_urid(self.map, P::INFO.uri, key);
        let string;
        let (ptr, size, type_): (*const c_void, usize, u32) = match value {
            StateValue::Bytes(b) => (b.as_ptr().cast(), b.len(), u.atom_chunk),
            StateValue::Str(s) => {
                // atom:String includes the terminating NUL
                string = [s.as_bytes(), &[0]].concat();
                (string.as_ptr().cast(), string.len(), u.atom_string)
            }
            StateValue::F32(ref v) => ((v as *const f32).cast(), 4, u.atom_float),
            StateValue::I64(ref v) => ((v as *const i64).cast(), 8, u.atom_long),
        };
        // SAFETY: the host's store function; the value lives for the call
        let status =
            unsafe { (self.store)(self.handle, k, ptr, size, type_, LV2_STATE_IS_POD | LV2_STATE_IS_PORTABLE) };
        if status != LV2_STATE_SUCCESS {
            self.status = status;
        }
    }
}

struct Retrieve<'a, P> {
    urids: Urids,
    map: &'a LV2_URID_Map,
    retrieve: unsafe extern "C" fn(LV2_State_Handle, u32, *mut usize, *mut u32, *mut u32) -> *const c_void,
    handle: LV2_State_Handle,
    _plugin: PhantomData<P>,
}

impl<P: Plugin> StateSource for Retrieve<'_, P> {
    fn get(&self, key: &str) -> Option<StateValue<'_>> {
        let u = &self.urids;
        let k = key_urid(self.map, P::INFO.uri, key);
        let (mut size, mut type_, mut flags) = (0usize, 0u32, 0u32);
        // SAFETY: the host's retrieve function
        let p = unsafe { (self.retrieve)(self.handle, k, &mut size, &mut type_, &mut flags) };
        if p.is_null() {
            return None;
        }
        // SAFETY: the host keeps `size` bytes at p valid during restore
        let bytes = unsafe { std::slice::from_raw_parts(p.cast::<u8>(), size) };
        if type_ == u.atom_chunk {
            Some(StateValue::Bytes(bytes))
        } else if type_ == u.atom_string {
            let s = bytes.split(|b| *b == 0).next().unwrap_or(&[]);
            std::str::from_utf8(s).ok().map(StateValue::Str)
        } else if type_ == u.atom_float && size == 4 {
            Some(StateValue::F32(f32::from_ne_bytes(bytes.try_into().ok()?)))
        } else if type_ == u.atom_long && size == 8 {
            Some(StateValue::I64(i64::from_ne_bytes(bytes.try_into().ok()?)))
        } else if type_ == u.atom_int && size == 4 {
            Some(StateValue::I64(i64::from(i32::from_ne_bytes(bytes.try_into().ok()?))))
        } else {
            None
        }
    }
}

unsafe extern "C" fn save<P: Plugin>(
    h: LV2_Handle,
    store: LV2_State_Store_Function,
    handle: LV2_State_Handle,
    _flags: u32,
    _features: *const *const LV2_Feature,
) -> LV2_State_Status {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: a live instance of P (LV2 contract)
        let Some(instance) = (unsafe { h.cast::<Instance<P>>().as_ref() }) else { return LV2_STATE_ERR_UNKNOWN };
        let (Some(store), Some(map)) = (store, instance.map.as_ref()) else { return LV2_STATE_ERR_UNKNOWN };
        let mut sink =
            Store { urids: instance.urids, map, store, handle, status: LV2_STATE_SUCCESS, _plugin: PhantomData::<P> };
        instance.runner.plugin().save_state(&mut sink);
        sink.status
    }))
    .unwrap_or(LV2_STATE_ERR_UNKNOWN)
}

unsafe extern "C" fn restore<P: Plugin>(
    h: LV2_Handle,
    retrieve: LV2_State_Retrieve_Function,
    handle: LV2_State_Handle,
    _flags: u32,
    _features: *const *const LV2_Feature,
) -> LV2_State_Status {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: a live instance of P, not running (LV2 contract)
        let Some(instance) = (unsafe { h.cast::<Instance<P>>().as_mut() }) else { return LV2_STATE_ERR_UNKNOWN };
        let (Some(retrieve), Some(map)) = (retrieve, instance.map) else { return LV2_STATE_ERR_UNKNOWN };
        let source = Retrieve { urids: instance.urids, map: &map, retrieve, handle, _plugin: PhantomData::<P> };
        let result = instance.runner.plugin_mut().load_state(&source);
        match result {
            Ok(()) => LV2_STATE_SUCCESS,
            Err(_) => LV2_STATE_ERR_NO_PROPERTY,
        }
    }))
    .unwrap_or(LV2_STATE_ERR_UNKNOWN)
}
