//! DWAA/DWAB decoder (ported from OpenEXR Core internal_dwa.c and internal_dwa_decoder.h)
//!
//! This is a work-in-progress implementation. Initial commits provide the structure and
//! function signatures; full decoding will be implemented incrementally.

use crate::compression::ByteVec;
use crate::error::{Error, Result};
use crate::meta::attribute::{ChannelList, IntegerBounds};

pub(crate) fn decompress(
    _channels: &ChannelList,
    _compressed_le: ByteVec,
    _pixel_section: IntegerBounds,
    _expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    Err(Error::unsupported("DWA decompression not yet implemented"))
}
