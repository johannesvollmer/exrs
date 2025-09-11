use std::alloc::alloc;
use std::ffi::c_void;
use std::os::raw::c_int;
use libc::{free, malloc};

// --- Placeholder external types from OpenEXR ---
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum exr_pixel_type_t {
    UINT = 0,
    HALF = 1,
    FLOAT = 2,
}

// #[repr(C)]
// #[derive(Copy, Clone)]
// pub enum exr_result_t {
//     EXR_ERR_SUCCESS = 0,
//     EXR_ERR_OUT_OF_MEMORY = 1,
// }
pub type exr_result_t = i32;
pub const EXR_ERR_OUT_OF_MEMORY: exr_result_t = -1;
pub const EXR_ERR_SUCCESS: exr_result_t = 0;

#[repr(C)]
pub enum CompressionScheme {
    ZIP,
    PIZ,
    DWAA,
    DWAB,
}

#[repr(C)]
pub struct exr_coding_channel_info_t {
    pub dummy: i32,
}

pub type c_size_t = usize;



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


pub const EXR_PIXEL_LAST_TYPE: u8 = 3;

pub type uint8_t = u8;
pub type uint16_t = u16;
pub type uint32_t = u32;
pub type uint64_t = u64;
pub type size_t = usize;


const _SSE_ALIGNMENT: usize = 16;

/// External helpers expected elsewhere in the port:
pub fn float_to_half(f: f32) -> uint16_t {}
pub fn half_to_float(h: uint16_t) -> f32 {}
pub fn one_from_native16(v: uint16_t) -> uint16_t {}
pub fn one_to_native16(v: uint16_t) -> uint16_t {}
pub fn one_to_native_float(f: f32) -> f32 {}
pub fn dctForward8x8(data: *mut f32) {}
pub fn csc709Forward64(r: *mut f32, g: *mut f32, b: *mut f32) {}
pub fn convertFloatToHalf64(dst: *mut uint16_t, src: *const f32) {}

// memory helpers
// alloc_fn: extern "C" fn(size_t) -> *mut c_void
// free_fn: extern "C" fn(*mut c_void)
const alloc_fn: fn(size_t) -> *mut c_void = malloc;
const free_fn: fn(*mut c_void) = free;

/// Minimal stubs for things referenced from previous ports (fill in real definitions elsewhere).
#[repr(C)]
pub struct DctCoderChannelData {
    pub _dctData: *mut f32,
    pub _halfZigData: *mut uint16_t,
    pub _dc_comp: *mut uint16_t,
    pub _rows: *mut *mut uint8_t,
    pub _row_alloc_count: size_t,
    pub _size: size_t,
    pub _type: c_int,
}
