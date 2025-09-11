// Port of dwa.c (from OpenEXR) to Rust
// SPDX-License-Identifier: BSD-3-Clause

use super::externals::*;
use std::ffi::CString;
use std::ptr;
use std::slice;

#[repr(C, align(16))] // _SSE_ALIGNMENT assumed 16
pub struct DctCoderChannelData {
    pub dct_data: [f32; 64],
    pub half_zig_data: [u16; 64],

    pub dc_comp: *mut u16,
    pub rows: *mut *mut u8,
    pub row_alloc_count: usize,
    pub size: usize,
    pub pixel_type: exr_pixel_type_t,

    pub pad: [u8; 28],
}

impl DctCoderChannelData {
    pub fn construct(pixel_type: exr_pixel_type_t) -> Self {
        Self {
            dct_data: [0.0; 64],
            half_zig_data: [0; 64],
            dc_comp: ptr::null_mut(),
            rows: ptr::null_mut(),
            row_alloc_count: 0,
            size: 0,
            pixel_type,
            pad: [0; 28],
        }
    }

    pub fn destroy<F: FnMut(*mut c_void)>(&mut self, mut free_fn: F) {
        if !self.rows.is_null() {
            free_fn(self.rows as *mut _);
            self.rows = ptr::null_mut();
        }
    }

    pub fn push_row<A: FnMut(usize) -> *mut u8, F: FnMut(*mut c_void)>(
        &mut self,
        mut alloc_fn: A,
        mut free_fn: F,
        r: *mut u8,
    ) -> exr_result_t {
        if self.size == self.row_alloc_count {
            let nsize = if self.size == 0 {
                16
            } else {
                (self.size * 3) / 2
            };
            let n = alloc_fn(nsize * size_of::<*mut u8>()) as *mut *mut u8;
            if !n.is_null() {
                unsafe {
                    if !self.rows.is_null() {
                        ptr::copy_nonoverlapping(self.rows, n, self.size);
                        free_fn(self.rows as *mut _);
                    }
                }
                self.rows = n;
                self.row_alloc_count = nsize;
            } else {
                return EXR_ERR_OUT_OF_MEMORY;
            }
        }
        unsafe {
            *self.rows.add(self.size) = r;
        }
        self.size += 1;
        EXR_ERR_OUT_OF_MEMORY
    }
}

#[repr(C)]
pub struct ChannelData {
    pub dct_data: DctCoderChannelData,
    pub chan: *mut exr_coding_channel_info_t,

    pub planar_unc_buffer: *mut u8,
    pub planar_unc_buffer_end: *mut u8,

    pub planar_unc_rle: [*mut u8; 4],
    pub planar_unc_rle_end: [*mut u8; 4],

    pub planar_unc_size: usize,
    pub processed: i32,
    pub compression: CompressorScheme,
    pub planar_unc_type: exr_pixel_type_t,
    pub pad: [u8; 20],
}

#[repr(C)]
pub struct CscChannelSet {
    pub idx: [i32; 3],
}

#[repr(C)]
pub struct CscPrefixMapItem {
    pub name: *const i8,
    pub prefix_len: usize,
    pub idx: [i32; 3],
    pub pad: [u8; 4],
}

pub unsafe fn CscPrefixMap_find(
    mapl: *mut CscPrefixMapItem,
    max_size: i32,
    cname: *const i8,
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

        let matches =
            (*item).prefix_len == prefixlen && libc::strncmp(cname, (*item).name, prefixlen) == 0;
        if matches {
            break;
        }
        idx += 1;
    }
    mapl.add(idx as usize)
}
