//! DWAA/DWAB decoder (ported from OpenEXR Core internal_dwa.c and internal_dwa_decoder.h)
//!
//! This is a work-in-progress implementation. Initial commits provide the structure and
//! function signatures; full decoding will be implemented incrementally.

use crate::compression::ByteVec;
use crate::error::{Error, Result};
use crate::meta::attribute::{ChannelList, IntegerBounds};
use super::helpers::BitReader;

pub(crate) fn decompress(
    _channels: &ChannelList,
    compressed_le: ByteVec,
    _pixel_section: IntegerBounds,
    _expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    // Begin port: set up bit reader and perform minimal sanity checks.
    let mut br = BitReader::new(&compressed_le);
    // Align to byte boundary in case upstream provided byte-aligned blocks.
    br.align_to_byte();
    // For now, we do not attempt to parse further; return a precise NotSupported.
    Err(Error::unsupported("DWA decompression header parsing WIP"))
}
