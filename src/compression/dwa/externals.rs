use crate::prelude::ChannelDescription;
use libc::{free, malloc};
use std::alloc::alloc;
use std::ffi::{c_char, c_void};
use std::mem::{size_of, zeroed};
use std::os::raw::c_int;
use std::ptr;

// --- Placeholder external types from OpenEXR ---
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum exr_pixel_type_t {
    UINT = 0,
    HALF = 1,
    FLOAT = 2,
}
#[repr(C)]
pub enum exr_pixel_type_t_2 {
    EXR_PIXEL_UINT = 0,
    EXR_PIXEL_HALF = 1,
    EXR_PIXEL_FLOAT = 2,
}

// #[repr(C)]
// #[derive(Copy, Clone)]
// pub enum exr_result_t {
//     EXR_ERR_SUCCESS = 0,
//     EXR_ERR_OUT_OF_MEMORY = 1,
// }
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

// Minimal external constants/types assumed from surrounding code
pub const DWA_CLASSIFIER_FALSE: u16 = 0;
pub const DWA_CLASSIFIER_TRUE: u16 = 1;


pub const DWA_CLASSIFIER_TRUE: i32 = 1;
pub const NUM_COMPRESSOR_SCHEMES: usize = 4;
pub const UNKNOWN: i32 = -1;

// Size-table related constants (placeholders; keep names used in C)
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

// constants used
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

pub const EXR_PIXEL_LAST_TYPE: u8 = 3;

pub type uint8_t = u8;
pub type uint16_t = u16;
pub type uint32_t = u32;
pub type uint64_t = u64;
pub type size_t = usize;

type uintptr_t = usize;

pub type intptr_t = isize;

const _SSE_ALIGNMENT: usize = 16;


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

pub type exr_memory_allocation_func_t = unsafe extern "C" fn(size: usize) -> *mut c_void;
pub type exr_memory_free_func_t = unsafe extern "C" fn(ptr: *mut c_void);

#[repr(C)]
#[derive(Copy, Clone)]
pub enum AcCompression {
    STATIC_HUFFMAN = 0,
    DEFLATE = 1,
}

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
    // ... placeholder
}

// Minimal placeholder types used in the function body
#[repr(C)]
pub struct exr_encode_pipeline_t {
    pub compressed_alloc_size: uint64_t,
    pub compressed_buffer: *mut uint8_t,
    pub packed_buffer: *mut uint8_t,
    pub packed_alloc_size: size_t,
    pub packed_bytes: size_t,
    pub scratch_buffer_1: *mut uint8_t,
    pub scratch_alloc_size_1: size_t,
    pub context: *const exr_const_context_t,
    // other fields omitted
}

#[repr(C)]
pub struct CscChannelSet {
    pub idx: [c_int; 3],
}

pub type exr_memory_allocation_func_t =
unsafe extern "C" fn(size: size_t) -> *mut std::os::raw::c_void;
pub type exr_memory_free_func_t = unsafe extern "C" fn(ptr: *mut std::os::raw::c_void);


#[repr(C)]
pub struct DwaCompressor {
    pub _decode: *mut exr_decode_pipeline_t,
    pub _packedAcBuffer: *mut uint8_t,
    pub _packedAcBufferSize: uint64_t,
    pub _packedDcBuffer: *mut uint8_t,
    pub _packedDcBufferSize: uint64_t,
    pub _rleBuffer: *mut uint8_t,
    pub _rleBufferSize: uint64_t,
    pub _planarUncBuffer: [*mut uint8_t; NUM_COMPRESSOR_SCHEMES],
    pub _planarUncBufferSize: [uint64_t; NUM_COMPRESSOR_SCHEMES],
    pub _channelData: *mut ChannelData,
    pub _numChannels: c_int,
    pub _cscChannelSets: *mut CscChannelSet,
    pub _numCscChannelSets: c_int,
    pub _channelRules: *mut Classifier,
    pub _channelRuleCount: size_t,
    pub _min: [c_int; 2],
    pub _max: [c_int; 2],
    pub _numScanLines: c_int,
    pub alloc_fn: exr_memory_allocation_func_t,
    pub free_fn: exr_memory_free_func_t,
}

#[repr(C)]
pub struct CscChannelSet {
    pub idx: [c_int; 3],
}

// externs for rule tables
pub static mut sLegacyChannelRules: [Classifier; 1];
pub static mut sDefaultChannelRules: [Classifier; 1];

pub type exr_coding_channel_info_t = ChannelDescription;

#[repr(C)]
pub struct CscPrefixMapItem {
    pub name: *const c_char,
    pub prefix_len: size_t,
    pub idx: [c_int; 3],
    pub pad: [u8; 4],
}



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

fn internal_exr_alloc_aligned(
    alloc_fn: exr_memory_allocation_func_t,
    out_mem: *mut *mut std::os::raw::c_void,
    size: usize,
    align: usize,
) -> *mut ChannelData;
fn exr_get_zip_compression_level(
    context: *const exr_const_context_t,
    part_index: i32,
    out_level: *mut i32,
) -> exr_result_t;
fn exr_get_dwa_compression_level(
    context: *const exr_const_context_t,
    part_index: i32,
    out_level: *mut f32,
) -> exr_result_t;

pub fn CscPrefixMap_find(
    mapl: *mut CscPrefixMapItem,
    maxSize: c_int,
    cname: *const c_char,
    prefixlen: size_t,
) -> *mut CscPrefixMapItem;

fn DctCoderChannelData_construct(d: *mut DctCoderChannelData, t: exr_pixel_type_t);

fn DctCoderChannelData_destroy(
    free_fn: unsafe extern "C" fn(*mut std::os::raw::c_void),
    d: *mut std::os::raw::c_void,
);
fn DwaCompressor_initializeBuffers(me: *mut DwaCompressor, out: *mut size_t) -> c_int;
fn internal_encode_alloc_buffer(
    encode: *mut exr_encode_pipeline_t,
    which: c_int,
    out_ptr: *mut *mut uint8_t,
    out_size: *mut uint64_t,
    needed: size_t,
) -> c_int;
fn DwaCompressor_writeRelevantChannelRules(
    me: *mut DwaCompressor,
    out_ptr: *mut *mut uint8_t,
    nAvail: uint64_t,
    nWritten: *mut uint64_t,
) -> c_int;
fn DwaCompressor_setupChannelData(me: *mut DwaCompressor) -> c_int;
fn DctCoderChannelData_push_row(
    alloc_fn: exr_memory_allocation_func_t,
    free_fn: exr_memory_free_func_t,
    d: *mut DctCoderChannelData,
    r: *mut uint8_t,
) -> c_int;
fn LossyDctEncoderCsc_construct(
    enc: *mut LossyDctEncoder,
    level: f32,
    a: *mut DctCoderChannelData,
    b: *mut DctCoderChannelData,
    c: *mut DctCoderChannelData,
    packedAcEnd: *mut uint8_t,
    packedDcEnd: *mut uint8_t,
    lut: *const uint16_t,
    width: c_int,
    height: c_int,
) -> c_int;
fn LossyDctEncoder_construct(
    enc: *mut LossyDctEncoder,
    level: f32,
    dct: *mut DctCoderChannelData,
    packedAcEnd: *mut uint8_t,
    packedDcEnd: *mut uint8_t,
    nonlinearLut: *const uint16_t,
    width: c_int,
    height: c_int,
) -> c_int;
fn LossyDctEncoder_execute(
    alloc_fn: exr_memory_allocation_func_t,
    free_fn: exr_memory_free_func_t,
    enc: *mut LossyDctEncoder,
) -> c_int;

fn internal_huf_compress(
    outCompressedSizePtr: *mut uint64_t,
    outDataPtr: *mut uint8_t,
    outDataSize: size_t,
    src: *const uint16_t,
    srcCount: uint64_t,
    scratch: *mut uint8_t,
    scratch_size: size_t,
) -> c_int;

fn exr_compress_buffer(
    context: *const exr_const_context_t,
    level: c_int,
    src: *const uint8_t,
    src_len: size_t,
    dst: *mut uint8_t,
    dst_len: size_t,
    out_written: *mut size_t,
) -> c_int;

fn exr_compress_max_buffer_size(n: size_t) -> size_t;

fn internal_zip_deconstruct_bytes(dst: *mut uint8_t, src: *const uint8_t, n: size_t);

fn internal_rle_compress(
    dst: *mut uint8_t,
    dst_size: size_t,
    src: *const uint8_t,
    src_len: size_t,
) -> size_t;

fn priv_from_native64(sizes: *mut uint64_t, n: usize);


// exr_const globals referenced in code
pub static mut sDefaultChannelRules: [Classifier; 1]; // actual size elsewhere
pub static mut sLegacyChannelRules: [Classifier; 1];

pub fn priv_to_native64(s: *mut uint64_t, n: usize);
pub fn exr_uncompress_buffer(
    context: *const exr_const_context_t,
    src: *const uint8_t,
    src_len: size_t,
    dst: *mut uint8_t,
    dst_len: size_t,
    out_written: *mut size_t,
) -> exr_result_t;
pub fn internal_huf_decompress(
    decode: *mut exr_decode_pipeline_t,
    src: *const uint8_t,
    src_len: uint64_t,
    dst: *mut uint16_t,
    dst_count: uint64_t,
    scratch: *mut uint8_t,
    scratch_size: size_t,
) -> exr_result_t;
pub fn internal_decode_alloc_buffer(
    decode: *mut exr_decode_pipeline_t,
    which: c_int,
    out_ptr: *mut *mut uint8_t,
    out_size: *mut uint64_t,
    needed: size_t,
) -> exr_result_t;
pub fn internal_zip_reconstruct_bytes(dst: *mut uint8_t, src: *const uint8_t, n: size_t);
pub fn internal_rle_decompress(
    dst: *mut uint8_t,
    dst_len: size_t,
    src: *const uint8_t,
    src_len: size_t,
) -> size_t;
pub fn DwaCompressor_readChannelRules(
    me: *mut DwaCompressor,
    inptr: *mut *const uint8_t,
    nAvail: *mut uint64_t,
    outRuleSize: *mut uint64_t,
) -> exr_result_t;
pub fn DwaCompressor_initializeBuffers(me: *mut DwaCompressor, out: *mut size_t) -> exr_result_t;
pub fn DwaCompressor_setupChannelData(me: *mut DwaCompressor) -> exr_result_t;
pub fn DctCoderChannelData_push_row(
    alloc_fn: exr_memory_allocation_func_t,
    free_fn: exr_memory_free_func_t,
    d: *mut DctCoderChannelData,
    r: *mut uint8_t,
) -> exr_result_t;
pub fn LossyDctDecoderCsc_construct(
    decoder: *mut LossyDctDecoder,
    a: *mut DctCoderChannelData,
    b: *mut DctCoderChannelData,
    c: *mut DctCoderChannelData,
    packedAcBegin: *mut uint8_t,
    packedAcEnd: *mut uint8_t,
    packedDcBegin: *mut uint8_t,
    totalDcUncompressedCount: uint64_t,
    lut: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t;
pub fn LossyDctDecoder_construct(
    decoder: *mut LossyDctDecoder,
    dct: *mut DctCoderChannelData,
    packedAcBegin: *mut uint8_t,
    packedAcEnd: *mut uint8_t,
    packedDcBegin: *mut uint8_t,
    totalDcUncompressedCount: uint64_t,
    lut: *const uint16_t,
    width: c_int,
    height: c_int,
) -> exr_result_t;
pub fn LossyDctDecoder_execute(
    alloc_fn: exr_memory_allocation_func_t,
    free_fn: exr_memory_free_func_t,
    decoder: *mut LossyDctDecoder,
) -> exr_result_t;
pub fn dwaCompressorToLinear() -> *const uint16_t;
pub fn dwaCompressorToNonlinear() -> *const uint16_t;
pub fn interleaveByte2(dst: *mut uint8_t, src0: *mut uint8_t, src1: *mut uint8_t, width: c_int);

pub type exr_memory_allocation_func_t =
    unsafe extern "C" fn(size: size_t) -> *mut std::os::raw::c_void;
pub type exr_memory_free_func_t = unsafe extern "C" fn(ptr: *mut std::os::raw::c_void);



// helpers used above or earlier in the translation
pub fn exr_compress_max_buffer_size(n: size_t) -> size_t;
pub fn one_from_native16(v: uint16_t) -> uint16_t;
pub fn one_to_native16(v: uint16_t) -> uint16_t;
pub fn Classifier_size(me: *const Classifier) -> uint64_t;
pub fn Classifier_write(me: *const Classifier, ptr: *mut *mut uint8_t) -> uint64_t;
pub fn Classifier_find_suffix(channel_name: *const c_char) -> *const c_char;
pub fn Classifier_match(me: *const Classifier, suffix: *const c_char, t: exr_pixel_type_t)
                        -> c_int;
pub fn Classifier_read(
    alloc_fn: exr_memory_allocation_func_t,
    out: *mut Classifier,
    ptr: *mut *const uint8_t,
    size: *mut uint64_t,
) -> exr_result_t;
pub fn Classifier_destroy(free_fn: exr_memory_free_func_t, c: *mut Classifier);

