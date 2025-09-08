//! DWAa/DWAb compression (Industrial Light & Magic / OpenEXR)
//!
//! Placeholder module for DWA compression algorithms. This will be implemented by
//! porting OpenEXR's DwaCompressor.{h,cpp} to Rust. The implementation will live here
//! and expose `compress` and `decompress` entry points wired into the crate.
//!
//! Until fully implemented, these functions return NotSupported with a message, so
//! callers/tests can be wired to require support specifically for DWAB.

mod helpers;
mod channeldata;
mod classifier;
mod tables;
mod decoder;

use crate::compression::ByteVec;
use crate::error::{Error, Result};
use crate::meta::attribute::{ChannelList, IntegerBounds};

/// Decompress DWA (DWAA/DWAB) compressed bytes into native-endian pixel bytes.
///
/// `expected_byte_size` is the size of the uncompressed pixel block.
/// If `pedantic` is true, additional bytes after decoding will be considered an error.
pub(crate) fn decompress(
    channels: &ChannelList,
    compressed_le: ByteVec,
    pixel_section: IntegerBounds,
    expected_byte_size: usize,
    pedantic: bool,
) -> Result<ByteVec> {
    decoder::decompress(channels, compressed_le, pixel_section, expected_byte_size, pedantic)
}

/// Compress a native-endian pixel block into DWA (DWAA/DWAB) encoded little-endian bytes.
pub(crate) fn compress(
    _channels: &ChannelList,
    _uncompressed_ne: ByteVec,
    _pixel_section: IntegerBounds,
    _is_dwab: bool,
    _level: Option<f32>,
) -> Result<ByteVec> {
    Err(Error::unsupported("DWA compression not yet implemented"))
}
