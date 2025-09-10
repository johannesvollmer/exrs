// src/compression/dwa_channeldata.rs
// Rust port of channeldata.c from OpenEXRCore
// Low-level translation with unsafe pointers preserved where necessary

use std::ffi::c_char;
use std::ffi::c_void;
use std::os::raw::{c_int, c_size_t};
use std::ptr;

// Placeholder definitions for external types
type ExrPixelType = i32;
type ExrResult = i32;
const EXR_ERR_SUCCESS: ExrResult = 0;
const EXR_ERR_OUT_OF_MEMORY: ExrResult = -1;

type CompressorScheme = i32;
#[allow(non_camel_case_types)]
pub struct exr_coding_channel_info_t;

#[repr(C, align(16))] // _SSE_ALIGNMENT assumed 16
pub struct DctCoderChannelData {
    pub _dctData: [f32; 64],
    pub _halfZigData: [u16; 64],

    pub _dc_comp: *mut u16,
    pub _rows: *mut *mut u8,
    pub _row_alloc_count: usize,
    pub _size: usize,
    pub _type: ExrPixelType,

    pub _pad: [u8; 28],
}

impl DctCoderChannelData {
    pub unsafe fn construct(&mut self, t: ExrPixelType) {
        ptr::write_bytes(self as *mut _ as *mut u8, 0, std::mem::size_of::<DctCoderChannelData>());
        self._type = t;
    }

    pub unsafe fn destroy(&mut self, free_fn: unsafe extern "C" fn(*mut c_void)) {
        if !self._rows.is_null() {
            free_fn(self._rows as *mut c_void);
        }
    }

    pub unsafe fn push_row(
        &mut self,
        alloc_fn: unsafe extern "C" fn(c_size_t) -> *mut c_void,
        free_fn: unsafe extern "C" fn(*mut c_void),
        r: *mut u8,
    ) -> ExrResult {
        if self._size == self._row_alloc_count {
            let nsize = if self._size == 0 { 16 } else { (self._size * 3) / 2 };
            let n = alloc_fn(nsize * std::mem::size_of::<*mut u8>()) as *mut *mut u8;
            if !n.is_null() {
                if !self._rows.is_null() {
                    ptr::copy_nonoverlapping(self._rows, n, self._size);
                    free_fn(self._rows as *mut c_void);
                }
                self._rows = n;
                self._row_alloc_count = nsize;
            } else {
                return EXR_ERR_OUT_OF_MEMORY;
            }
        }
        *self._rows.add(self._size) = r;
        self._size += 1;
        EXR_ERR_SUCCESS
    }
}

#[repr(C)]
pub struct ChannelData {
    pub _dctData: DctCoderChannelData,

    pub chan: *mut exr_coding_channel_info_t,

    pub planarUncBuffer: *mut u8,
    pub planarUncBufferEnd: *mut u8,

    pub planarUncRle: [*mut u8; 4],
    pub planarUncRleEnd: [*mut u8; 4],

    pub planarUncSize: usize,
    pub processed: c_int,
    pub compression: CompressorScheme,
    pub planarUncType: ExrPixelType,
    pub _pad: [u8; 20],
}

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

impl CscPrefixMapItem {
    pub unsafe fn find(
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

            if (*item).prefix_len == prefixlen &&
                libc::strncmp(cname, (*item).name, prefixlen) == 0 {
                break;
            }
            idx += 1;
        }
        mapl.add(idx as usize)
    }
}
