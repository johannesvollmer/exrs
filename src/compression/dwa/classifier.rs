// src/compression/dwa_classifier.rs
// Rust translation of classifier.c from OpenEXRCore (internal DWA)
// SPDX-License-Identifier: BSD-3-Clause

#![allow(non_camel_case_types)]
#![allow(dead_code)]
#![allow(non_snake_case)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uchar, c_uint};
use std::ptr;
use std::slice;

// Minimal external constants/types assumed from surrounding code
pub const DWA_CLASSIFIER_FALSE: u16 = 0;
pub const DWA_CLASSIFIER_TRUE: u16 = 1;

// CompressorScheme enum (partial)
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum CompressorScheme {
    LOSSY_DCT = 0,
    RLE = 1,
    // other schemes omitted
}

pub const NUM_COMPRESSOR_SCHEMES: usize = 4; // placeholder

// pixel types (partial)
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum exr_pixel_type_t {
    EXR_PIXEL_UINT = 0,
    EXR_PIXEL_HALF = 1,
    EXR_PIXEL_FLOAT = 2,
}

pub const EXR_PIXEL_LAST_TYPE: u8 = 3;

#[repr(C)]
pub struct Classifier {
    pub _suffix: *const c_char,
    pub _scheme: CompressorScheme,
    pub _type: exr_pixel_type_t,
    pub _cscIdx: c_int,
    pub _caseInsensitive: u16,
    pub _stringStatic: u16,
}

impl Default for Classifier {
    fn default() -> Self {
        Self {
            _suffix: ptr::null(),
            _scheme: CompressorScheme::LOSSY_DCT,
            _type: exr_pixel_type_t::EXR_PIXEL_HALF,
            _cscIdx: -1,
            _caseInsensitive: DWA_CLASSIFIER_FALSE,
            _stringStatic: DWA_CLASSIFIER_FALSE,
        }
    }
}

// clang-format off equivalent: static tables
static mut sDefaultChannelRules: [Classifier; 15] = [
    Classifier { _suffix: b"R\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 0, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"R\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 0, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"G\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"G\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"B\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"B\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"Y\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"Y\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"BY\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"BY\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"RY\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"RY\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"A\0".as_ptr() as *const c_char, _scheme: CompressorScheme::RLE, _type: exr_pixel_type_t::EXR_PIXEL_UINT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"A\0".as_ptr() as *const c_char, _scheme: CompressorScheme::RLE, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"A\0".as_ptr() as *const c_char, _scheme: CompressorScheme::RLE, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_FALSE, _stringStatic: DWA_CLASSIFIER_TRUE },
];

static mut sLegacyChannelRules: [Classifier; 25] = [
    Classifier { _suffix: b"r\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 0, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"r\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 0, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"red\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 0, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"red\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 0, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"g\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"g\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"grn\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"grn\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"green\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"green\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"b\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"b\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"blu\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"blu\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"blue\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"blue\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: 2, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"y\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"y\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"by\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"by\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"ry\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"ry\0".as_ptr() as *const c_char, _scheme: CompressorScheme::LOSSY_DCT, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"a\0".as_ptr() as *const c_char, _scheme: CompressorScheme::RLE, _type: exr_pixel_type_t::EXR_PIXEL_UINT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"a\0".as_ptr() as *const c_char, _scheme: CompressorScheme::RLE, _type: exr_pixel_type_t::EXR_PIXEL_HALF, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
    Classifier { _suffix: b"a\0".as_ptr() as *const c_char, _scheme: CompressorScheme::RLE, _type: exr_pixel_type_t::EXR_PIXEL_FLOAT, _cscIdx: -1, _caseInsensitive: DWA_CLASSIFIER_TRUE, _stringStatic: DWA_CLASSIFIER_TRUE },
];

// clang-format on

// Functions

pub unsafe fn Classifier_destroy(free_fn: unsafe extern "C" fn(*mut c_void), p: *mut Classifier) {
    if p.is_null() { return; }
    let pc = &mut *p;
    if !pc._suffix.is_null() && pc._stringStatic == DWA_CLASSIFIER_FALSE {
        free_fn(pc._suffix as *mut c_void);
    }
}

pub unsafe fn Classifier_read(
    alloc_fn: unsafe extern "C" fn(usize) -> *mut c_void,
    out: *mut Classifier,
    ptr: &mut *const u8,
    size: &mut usize,
) -> Result<(), i32> {
    if *size <= 3 { return Err(-2); /* EXR_ERR_CORRUPT_CHUNK */ }
    let curin = *ptr;
    let mut len: usize = 0;

    // read suffix up to 128+1
    let mut suffix_buf = [0u8; 129];
    let mut found = false;
    while len < 129 {
        if len > (*size - 3) { return Err(-2); }
        let b = *curin.add(len);
        suffix_buf[len] = b;
        if b == 0 { found = true; break; }
        len += 1;
    }
    if !found { return Err(-2); }
    // include null
    len += 1;

    let mem = alloc_fn(len) as *mut c_char;
    if mem.is_null() { return Err(-1); }
    ptr::copy_nonoverlapping(suffix_buf.as_ptr() as *const c_char, mem, len);

    // advance
    let mut cur = curin.add(len);
    if *size < len + 2 { return Err(-2); }
    let value = *cur; cur = cur.add(1);
    let type_byte = *cur; cur = cur.add(1);

    *ptr = cur;
    *size -= len + 2;

    let outc = &mut *out;
    outc._suffix = mem as *const c_char;
    outc._stringStatic = DWA_CLASSIFIER_FALSE;

    outc._cscIdx = ((value >> 4) as i32) - 1;
    if outc._cscIdx < -1 || outc._cscIdx >= 3 { return Err(-2); }

    outc._scheme = match (value >> 2) & 3 {
        0 => CompressorScheme::LOSSY_DCT,
        1 => CompressorScheme::RLE,
        _ => return Err(-2),
    };

    outc._caseInsensitive = if (value & 1) != 0 { DWA_CLASSIFIER_TRUE } else { DWA_CLASSIFIER_FALSE };

    if (type_byte as u8) >= EXR_PIXEL_LAST_TYPE { return Err(-2); }
    outc._type = match type_byte as u8 {
        0 => exr_pixel_type_t::EXR_PIXEL_UINT,
        1 => exr_pixel_type_t::EXR_PIXEL_HALF,
        2 => exr_pixel_type_t::EXR_PIXEL_FLOAT,
        _ => return Err(-2),
    };

    Ok(())
}

pub unsafe fn Classifier_match(me: *const Classifier, suffix: *const c_char, r#type: exr_pixel_type_t) -> i32 {
    if me.is_null() { return DWA_CLASSIFIER_FALSE as i32; }
    let me_ref = &*me;
    if me_ref._type != r#type { return DWA_CLASSIFIER_FALSE as i32; }

    if me_ref._caseInsensitive == DWA_CLASSIFIER_TRUE {
        #[cfg(unix)]
        {
            if libc::strcasecmp(suffix, me_ref._suffix) == 0 { return DWA_CLASSIFIER_TRUE as i32; }
        }
        #[cfg(windows)]
        {
            // on windows, the equivalent is _stricmp; libc may provide it
            if libc::stricmp(suffix, me_ref._suffix) == 0 { return DWA_CLASSIFIER_TRUE as i32; }
        }
        return DWA_CLASSIFIER_FALSE as i32;
    }

    if libc::strcmp(suffix, me_ref._suffix) == 0 { return DWA_CLASSIFIER_TRUE as i32; }
    DWA_CLASSIFIER_FALSE as i32
}

pub unsafe fn Classifier_size(me: *const Classifier) -> usize {
    if me.is_null() { return 0; }
    let s = libc::strlen((*me)._suffix);
    s + 1 + 2 * std::mem::size_of::<u8>()
}

pub unsafe fn Classifier_write(me: *const Classifier, ptr: &mut *mut u8) -> usize {
    if me.is_null() { return 0; }
    let outptr = *ptr;
    let size_bytes = libc::strlen((*me)._suffix) as usize + 1;
    ptr::copy_nonoverlapping((*me)._suffix as *const u8, outptr, size_bytes);
    let mut out = outptr.add(size_bytes);

    let mut value: u8 = 0;
    value |= ((((*me)._cscIdx + 1) as u8) & 15) << 4;
    value |= ((me.as_ref().unwrap()._scheme as u8) & 3) << 2;
    value |= (me.as_ref().unwrap()._caseInsensitive as u8) & 1;

    *out = value; out = out.add(1);
    *out = (*me)._type as u8; out = out.add(1);
    *ptr = out;
    size_bytes + 2
}

pub unsafe fn Classifier_find_suffix(channel_name: *const c_char) -> *const c_char {
    if channel_name.is_null() { return ptr::null(); }
    let s = libc::strrchr(channel_name, b'.' as i32);
    if s.is_null() { channel_name } else { s.add(1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_find_suffix() {
        let cname = CString::new("foo.R").unwrap();
        unsafe {
            let s = Classifier_find_suffix(cname.as_ptr());
            let sstr = CStr::from_ptr(s).to_str().unwrap();
            assert_eq!(sstr, "R");
        }
    }
}
