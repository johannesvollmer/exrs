// src/compression/dwa_channeldata.rs
// Rust port of channeldata.c from OpenEXRCore
// Low-level translation with unsafe pointers preserved where necessary

use crate::compression::dwa::externals::{
    c_size_t, exr_coding_channel_info_t, exr_pixel_type_t, exr_result_t, CompressorScheme,
    EXR_ERR_OUT_OF_MEMORY, EXR_ERR_SUCCESS,
};
use std::ffi::c_char;
use std::ffi::c_void;
use std::os::raw::c_int;
use std::ptr;

#[repr(C, align(16))] // _SSE_ALIGNMENT assumed 16
pub struct DctCoderChannelData {
    pub _dctData: [f32; 64],
    pub _halfZigData: [u16; 64],

    pub _dc_comp: *mut u16,
    pub _rows: *mut *mut u8,
    pub _row_alloc_count: usize,
    pub _size: usize,
    pub _type: exr_pixel_type_t,

    pub _pad: [u8; 28],
}

pub unsafe fn DctCoderChannelData_construct(me: &mut DctCoderChannelData, t: exr_pixel_type_t) {
    ptr::write_bytes(
        me as *mut _ as *mut u8,
        0,
        std::mem::size_of::<DctCoderChannelData>(),
    );
    me._type = t;
}

pub unsafe fn DctCoderChannelData_destroy(
    me: &mut DctCoderChannelData,
    free_fn: unsafe extern "C" fn(*mut c_void),
) {
    if !me._rows.is_null() {
        free_fn(me._rows as *mut c_void);
    }
}

pub unsafe fn DctCoderChannelData_push_row(
    me: &mut DctCoderChannelData,
    alloc_fn: unsafe extern "C" fn(c_size_t) -> *mut c_void,
    free_fn: unsafe extern "C" fn(*mut c_void),
    r: *mut u8,
) -> exr_result_t {
    if me._size == me._row_alloc_count {
        let nsize = if me._size == 0 {
            16
        } else {
            (me._size * 3) / 2
        };
        let n = alloc_fn(nsize * std::mem::size_of::<*mut u8>()) as *mut *mut u8;
        if !n.is_null() {
            if !me._rows.is_null() {
                ptr::copy_nonoverlapping(me._rows, n, me._size);
                free_fn(me._rows as *mut c_void);
            }
            me._rows = n;
            me._row_alloc_count = nsize;
        } else {
            return EXR_ERR_OUT_OF_MEMORY;
        }
    }
    *me._rows.add(me._size) = r;
    me._size += 1;
    EXR_ERR_SUCCESS
}

/**************************************/
#[repr(C)]
pub struct ChannelData {
    pub _dctData: DctCoderChannelData,

    pub chan: *mut exr_coding_channel_info_t,

    // Incoming and outgoing data is scanline interleaved, and it's much
    // easier to operate on contiguous data.  Assuming the planare unc
    // buffer is to hold RLE data, we need to rearrange to make bytes
    // adjacent.
    pub planarUncBuffer: *mut u8,
    pub planarUncBufferEnd: *mut u8,

    pub planarUncRle: [*mut u8; 4],
    pub planarUncRleEnd: [*mut u8; 4],

    pub planarUncSize: usize,
    pub processed: c_int,
    pub compression: CompressorScheme,
    pub planarUncType: exr_pixel_type_t,
    pub _pad: [u8; 20],
}
/**************************************/
#[repr(C)]
pub struct CscChannelSet {
    pub idx: [c_int; 3],
}

#[repr(C)]
pub struct CscPrefixMapItem {
    pub name: *const c_char,
    pub prefix_len: usize,
    pub idx: [c_int; 3],
    pub pad: [u8; 4],
}

pub unsafe fn CscPrefixMapItem_find(
    mapl: *mut CscPrefixMapItem,
    max_size: c_int,
    cname: *const c_char,
    prefixlen: usize,
) -> *mut CscPrefixMapItem {
    let mut idx = 0;
    while idx < max_size {
        let item = mapl.add(idx as usize);
        if (*item).name.is_null() {
            (*item).name = cname;
            (*item).prefix_len = prefixlen;
            (*item).idx = [-1, -1, -1];
            break;
        }

        if (*item).prefix_len == prefixlen && libc::strncmp(cname, (*item).name, prefixlen) == 0 {
            break;
        }
        idx += 1;
    }
    mapl.add(idx as usize)
}
