// all contents of this file will eventually be replaced by the existing types in the exrs library.

use crate::compression::dwa::classifier::Classifier;
use crate::compression::dwa::transform_8x8::{
    dct_inverse_8x8, dct_inverse_8x8_dc_only, f32_from_zig_zag_f16,
};
use crate::prelude::ChannelDescription;
pub use libc::{free, malloc};
use std::alloc::alloc;
pub use std::ffi::{c_char, c_void};
pub use std::mem::{size_of, zeroed};
pub use std::os::raw::c_int;
pub use std::ptr;
use half::f16;
use lebe::Endian;
use crate::block::samples::IntoNativeSample;

// --- Placeholder external types from OpenEXR ---
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum exr_pixel_type_t {
    UINT = 0,
    HALF = 1,
    FLOAT = 2,
}
pub const EXR_PIXEL_LAST_TYPE: u8 = 3;

pub type exr_result_t = i32;
pub const EXR_ERR_SUCCESS: exr_result_t = 0;

pub const EXR_ERR_OUT_OF_MEMORY: exr_result_t = -1;
pub const EXR_ERR_ARGUMENT_OUT_OF_RANGE: i32 = -2;
pub const EXR_ERR_INVALID_ARGUMENT: i32 = -3;
pub const EXR_ERR_CORRUPT_CHUNK: i32 = -4;
pub const EXR_ERR_BAD_CHUNK_LEADER: exr_result_t = -5;

#[repr(C)]
pub enum CompressionScheme {
    ZIP,
    PIZ,
    DWAA,
    DWAB,
}

pub type c_size_t = usize;

pub const DWA_CLASSIFIER_TRUE: i32 = 1;
pub const NUM_COMPRESSOR_SCHEMES: usize = 4;
pub const UNKNOWN: i32 = -1;

pub const NUM_SIZES_SINGLE: usize = 64;
pub const VERSION: usize = 0;
pub const UNKNOWN_UNCOMPRESSED_SIZE: usize = 1;
pub const UNKNOWN_COMPRESSED_SIZE: usize = 2;
pub const AC_COMPRESSED_SIZE: usize = 3;
pub const DC_COMPRESSED_SIZE: usize = 4;
pub const RLE_COMPRESSED_SIZE: usize = 5;
pub const RLE_UNCOMPRESSED_SIZE: usize = 6;
pub const RLE_RAW_SIZE: usize = 7;
pub const AC_UNCOMPRESSED_COUNT: usize = 8;
pub const DC_UNCOMPRESSED_COUNT: usize = 9;
pub const AC_COMPRESSION: usize = 10;

pub const NUM_COMPRESSOR_SCHEMES: usize = 4;
pub const UNKNOWN: usize = 0;
pub const RLE: usize = 1;
pub const AC_UNCOMPRESSED_COUNT: usize = 8;
pub const DC_UNCOMPRESSED_COUNT: usize = 9;
pub const AC_COMPRESSION: usize = 10;
pub const UNKNOWN_UNCOMPRESSED_SIZE: usize = 1;
pub const UNKNOWN_COMPRESSED_SIZE: usize = 2;
pub const AC_COMPRESSED_SIZE: usize = 3;
pub const DC_COMPRESSED_SIZE: usize = 4;
pub const RLE_COMPRESSED_SIZE: usize = 5;
pub const RLE_UNCOMPRESSED_SIZE: usize = 6;
pub const RLE_RAW_SIZE: usize = 7;

pub const NUM_COMPRESSOR_SCHEMES: usize = 4;
pub const RLE: usize = 1;
pub const LOSSY_DCT: c_int = 1;
pub const STATIC_HUFFMAN: c_int = 0;
pub const DEFLATE: c_int = 1;

// CompressorScheme enum
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub enum CompressorScheme {
    UNKNOWN = 0,
    LOSSY_DCT = 1,
    RLE = 2,
    ZIP = 3,
    NUM_COMPRESSOR_SCHEMES = 4,
}

pub type uint8_t = u8;
pub type uint16_t = u16;
pub type uint32_t = u32;
pub type uint64_t = u64;
pub type size_t = usize;

type uintptr_t = usize;

pub type intptr_t = isize;

const _SSE_ALIGNMENT: usize = 16;

pub use super::dwa::*;


#[repr(C)]
pub struct exr_chunk_t {
    pub start_x: i32,
    pub start_y: i32,
    pub width: i32,
    pub height: i32,
}

#[repr(C)]
pub struct exr_encode_pipeline_t {
    pub part_index: i32,
    pub channel_count: i32,
    pub channels: *mut exr_coding_channel_info_t,
    pub chunk: exr_chunk_t,
    pub compressed_alloc_size: u64,
    pub compressed_buffer: *mut u8,
    pub packed_buffer: *mut u8,
    pub packed_alloc_size: usize,
    pub packed_bytes: usize,
    pub packed_buffer_ptr: *mut u8,
    pub scratch_buffer_1: *mut u8,
    pub scratch_alloc_size_1: usize,
    pub packed_bytes_written: usize,
    // ... other fields may be present, placeholder only
}

#[repr(C)]
pub struct exr_decode_pipeline_t {
    pub channel_count: i32,
    pub channels: *mut exr_coding_channel_info_t,
    pub chunk: exr_chunk_t,
    pub bytes_decompressed: usize,
    context: (),
    // ... placeholder
}

pub type exr_memory_allocation_func_t =
    unsafe extern "C" fn(size: size_t) -> *mut std::os::raw::c_void;
pub type exr_memory_free_func_t = unsafe extern "C" fn(ptr: *mut std::os::raw::c_void);


pub type exr_coding_channel_info_t = ChannelDescription;

pub fn float_to_half(f: f32) -> uint16_t { f16::from_f32(f).to_bits() }
pub fn half_to_float(h: uint16_t) -> f32 { f16::from_bits(h).to_f32() }
pub fn one_from_native16(mut v: uint16_t) -> uint16_t { v.convert_current_to_little_endian(); v }
pub fn one_to_native16(mut v: uint16_t) -> uint16_t { v.convert_little_endian_to_current(); v }
pub fn one_from_native_float(mut f: f32) -> f32 { f.convert_current_to_little_endian(); f }
pub fn one_to_native_float(mut f: f32) -> f32 { f.convert_little_endian_to_current(); f }


pub fn exr_zip_compress_buffer(
    level: c_int,
    src: *const uint8_t,
    src_len: size_t,
    dst: *mut uint8_t,
    dst_len: size_t,
    out_written: *mut size_t,
) -> c_int {
    todo!()
}

pub fn exr_compress_max_buffer_size(n: size_t) -> size_t { todo!() }

pub fn internal_zip_deconstruct_bytes(dst: *mut uint8_t, src: *const uint8_t, n: size_t) { todo!() }

pub fn internal_rle_compress(
    dst: *mut uint8_t,
    dst_size: size_t,
    src: *const uint8_t,
    src_len: size_t,
) -> size_t { todo!() }

pub fn priv_from_native64(sizes: *mut uint64_t, n: usize) { todo!() }

pub fn priv_to_native64(s: *mut uint64_t, n: usize) { todo!() }
pub fn exr_zip_uncompress_buffer(
    src: *const uint8_t,
    src_len: size_t,
    dst: *mut uint8_t,
    dst_len: size_t,
    out_written: *mut size_t,
) -> exr_result_t {
    todo!()
}
pub fn internal_huf_compress(
    outCompressedSizePtr: *mut uint64_t,
    outDataPtr: *mut uint8_t,
    outDataSize: size_t,
    src: *const uint16_t,
    srcCount: uint64_t,
    scratch: *mut uint8_t,
    scratch_size: size_t,
) -> c_int {
    // todo: call piz/huffman
    todo!()
}

pub fn internal_huf_decompress(
    decode: *mut exr_decode_pipeline_t,
    src: *const uint8_t,
    src_len: uint64_t,
    dst: *mut uint16_t,
    dst_count: uint64_t,
    scratch: *mut uint8_t,
    scratch_size: size_t,
) -> exr_result_t {
    // todo: call piz/huffman
    todo!()
}

pub fn internal_decode_alloc_buffer(
    decode: *mut exr_decode_pipeline_t,
    which: c_int,
    out_ptr: *mut *mut uint8_t,
    out_size: *mut uint64_t,
    needed: size_t,
) -> exr_result_t {
    todo!()
}

/// Called after exr_zip_uncompress_buffer
pub fn internal_zip_reconstruct_bytes(dst: *mut uint8_t, src: *const uint8_t, n: size_t) {
    // todo: do the same operation that the zip compression does (interleave bytes)
    todo!()
}

pub fn internal_rle_decompress(
    dst: *mut uint8_t,
    dst_len: size_t,
    src: *const uint8_t,
    src_len: size_t,
) -> size_t {
    todo!()
}

// Byte interleaving of 2 byte arrays:
//    src0 = AAAA
//    src1 = BBBB
//    dst  = ABABABAB
// numBytes is the size of each of the source buffers
// this looks like it is the same as crate::compression::optimize_bytes::interleave_byte_blocks, but from two sources..?
// todo: is there no "undo" function of this?
pub fn interleaveByte2(dst: *mut uint8_t, src0: *mut uint8_t, src1: *mut uint8_t, width: c_int) {
    // for (int x = 0; x < numBytes; ++x)
    // {
    //     dst[2 * x]     = src0[x];
    //     dst[2 * x + 1] = src1[x];
    // }
}

pub fn priv_to_native16(data: *mut uint16_t, n: usize) {
    // todo: for each, call one_to_native (optimize later)
}

pub fn simd_align_pointer(p: *mut uint8_t) -> *mut uint8_t {
    // we do nothing because we don't want to support SIMD explicitly
    p
}

pub fn fromHalfZigZag(src: *const uint16_t, dst: *mut f32) {
    f32_from_zig_zag_f16(todo!(), todo!())
}

pub fn dctInverse8x8DcOnly(dst: *mut f32) {
    dct_inverse_8x8_dc_only(todo!())
}

pub fn dctInverse8x8(dst: *mut f32) {
    dct_inverse_8x8(todo!(), 0)
}
