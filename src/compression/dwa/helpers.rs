// helpers.rs -- translation of helpers.h

#![allow(non_camel_case_types, dead_code)]

use std::cmp;

/// Types used elsewhere in the port
pub type size_t = usize;
pub type uint64_t = u64;
pub type uint32_t = u32;
pub type uint16_t = u16;
pub type uint8_t = u8;

/// Forward declaration / import of lookups (provide in your port)
extern "C" {
    // e.g. pub static dwaCompressorToNonlinear: *const uint16_t;
    // pub static dwaCompressorToLinear: *const uint16_t;
}

/// AcCompression enum
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AcCompression {
    STATIC_HUFFMAN = 0,
    DEFLATE = 1,
}

/// CompressorScheme enum (keeps UNKNOWN = 0)
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CompressorScheme {
    UNKNOWN = 0,
    LOSSY_DCT = 1,
    RLE = 2,
    NUM_COMPRESSOR_SCHEMES = 3,
}

/// DataSizesSingle enum (one value per chunk)
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DataSizesSingle {
    VERSION = 0,
    UNKNOWN_UNCOMPRESSED_SIZE = 1,
    UNKNOWN_COMPRESSED_SIZE = 2,
    AC_COMPRESSED_SIZE = 3,
    DC_COMPRESSED_SIZE = 4,
    RLE_COMPRESSED_SIZE = 5,
    RLE_UNCOMPRESSED_SIZE = 6,
    RLE_RAW_SIZE = 7,
    AC_UNCOMPRESSED_COUNT = 8,
    DC_UNCOMPRESSED_COUNT = 9,
    AC_COMPRESSION = 10,
    NUM_SIZES_SINGLE = 11,
}

/// Expose numeric constant for NUM_SIZES_SINGLE
pub const NUM_SIZES_SINGLE: usize = DataSizesSingle::NUM_SIZES_SINGLE as usize;

/// tiny helper that mirrors the C inline std_max
#[inline]
pub fn std_max(a: usize, b: usize) -> usize {
    if a < b { b } else { a }
}
