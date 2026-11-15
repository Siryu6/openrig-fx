//! The C types of the LV2 specifications this crate uses (core, URID,
//! options, atom, state), written from the headers of LV2 1.18.

#![allow(non_camel_case_types)]

use std::ffi::{c_char, c_void};

pub type LV2_Handle = *mut c_void;
pub type LV2_URID = u32;

#[repr(C)]
pub struct LV2_Feature {
    pub uri: *const c_char,
    pub data: *mut c_void,
}

#[repr(C)]
pub struct LV2_Descriptor {
    pub uri: *const c_char,
    pub instantiate: Option<
        unsafe extern "C" fn(
            descriptor: *const LV2_Descriptor,
            sample_rate: f64,
            bundle_path: *const c_char,
            features: *const *const LV2_Feature,
        ) -> LV2_Handle,
    >,
    pub connect_port: Option<unsafe extern "C" fn(instance: LV2_Handle, port: u32, data: *mut c_void)>,
    pub activate: Option<unsafe extern "C" fn(instance: LV2_Handle)>,
    pub run: Option<unsafe extern "C" fn(instance: LV2_Handle, sample_count: u32)>,
    pub deactivate: Option<unsafe extern "C" fn(instance: LV2_Handle)>,
    pub cleanup: Option<unsafe extern "C" fn(instance: LV2_Handle)>,
    pub extension_data: Option<unsafe extern "C" fn(uri: *const c_char) -> *const c_void>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LV2_URID_Map {
    pub handle: *mut c_void,
    pub map: Option<unsafe extern "C" fn(handle: *mut c_void, uri: *const c_char) -> LV2_URID>,
}

#[repr(C)]
pub struct LV2_Options_Option {
    /// 0: instance, 1: resource, 2: blank, 3: port
    pub context: u32,
    pub subject: u32,
    pub key: LV2_URID,
    pub size: u32,
    pub type_: LV2_URID,
    pub value: *const c_void,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LV2_Atom {
    pub size: u32,
    pub type_: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LV2_Atom_Sequence_Body {
    pub unit: u32,
    pub pad: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LV2_Atom_Sequence {
    pub atom: LV2_Atom,
    pub body: LV2_Atom_Sequence_Body,
}

/// Event header: time in frames (sequences without a beat unit)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LV2_Atom_Event {
    pub frames: i64,
    pub body: LV2_Atom,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LV2_Atom_Object_Body {
    pub id: u32,
    pub otype: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LV2_Atom_Property_Body {
    pub key: u32,
    pub context: u32,
    pub value: LV2_Atom,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct LV2_Atom_Vector_Body {
    pub child_size: u32,
    pub child_type: u32,
}

pub type LV2_State_Handle = *mut c_void;
pub type LV2_State_Status = u32;
pub const LV2_STATE_SUCCESS: LV2_State_Status = 0;
pub const LV2_STATE_ERR_UNKNOWN: LV2_State_Status = 1;
pub const LV2_STATE_ERR_NO_PROPERTY: LV2_State_Status = 5;
pub const LV2_STATE_IS_POD: u32 = 1;
pub const LV2_STATE_IS_PORTABLE: u32 = 2;

pub type LV2_State_Store_Function = Option<
    unsafe extern "C" fn(
        handle: LV2_State_Handle,
        key: u32,
        value: *const c_void,
        size: usize,
        type_: u32,
        flags: u32,
    ) -> LV2_State_Status,
>;

pub type LV2_State_Retrieve_Function = Option<
    unsafe extern "C" fn(
        handle: LV2_State_Handle,
        key: u32,
        size: *mut usize,
        type_: *mut u32,
        flags: *mut u32,
    ) -> *const c_void,
>;

#[repr(C)]
pub struct LV2_State_Interface {
    pub save: Option<
        unsafe extern "C" fn(
            instance: LV2_Handle,
            store: LV2_State_Store_Function,
            handle: LV2_State_Handle,
            flags: u32,
            features: *const *const LV2_Feature,
        ) -> LV2_State_Status,
    >,
    pub restore: Option<
        unsafe extern "C" fn(
            instance: LV2_Handle,
            retrieve: LV2_State_Retrieve_Function,
            handle: LV2_State_Handle,
            flags: u32,
            features: *const *const LV2_Feature,
        ) -> LV2_State_Status,
    >,
}

/// Round up to the 64-bit alignment atoms use
#[inline]
pub const fn pad8(n: usize) -> usize {
    (n + 7) & !7
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn layouts_match_the_headers() {
        assert_eq!(size_of::<LV2_Atom>(), 8);
        assert_eq!(size_of::<LV2_Atom_Sequence>(), 16);
        assert_eq!(size_of::<LV2_Atom_Event>(), 16);
        assert_eq!(size_of::<LV2_Atom_Property_Body>(), 16);
        // Five u32, then the pointer at its alignment
        assert_eq!(
            size_of::<LV2_Options_Option>(),
            (20usize).next_multiple_of(size_of::<usize>()) + size_of::<usize>()
        );
        assert_eq!(size_of::<LV2_Descriptor>(), size_of::<[usize; 8]>());
    }
}
