//! DWA coefficient stream scaffolding.
//! This module will contain coefficient unpacking and 8x8 block reconstruction.

use crate::error::{Error, Result};

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BlockHeader {
    pub block_x: u16,
    pub block_y: u16,
    pub channel_index: u16,
}

/// Parse a BlockHeader from little-endian byte slice.
/// Returns Some((header, bytes_consumed)) on success (6 bytes), None if not enough data.
#[allow(dead_code)]
pub(crate) fn parse_block_header_le(data: &[u8]) -> Option<(BlockHeader, usize)> {
    if data.len() < 6 { return None; }
    let bx = u16::from_le_bytes([data[0], data[1]]);
    let by = u16::from_le_bytes([data[2], data[3]]);
    let ch = u16::from_le_bytes([data[4], data[5]]);
    Some((BlockHeader { block_x: bx, block_y: by, channel_index: ch }, 6))
}

#[allow(dead_code)]
pub(crate) fn decompress_blocks(_data: &[u8]) -> Result<()> {
    Err(Error::unsupported("DWA coefficient decoding not yet implemented"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_block_header_ok() {
        let bytes = [0x34, 0x12, 0x78, 0x56, 0xab, 0xcd];
        let (hdr, used) = parse_block_header_le(&bytes).expect("should parse");
        assert_eq!(used, 6);
        assert_eq!(hdr, BlockHeader { block_x: 0x1234, block_y: 0x5678, channel_index: 0xcdab });
    }

    #[test]
    fn parse_block_header_short() {
        assert!(parse_block_header_le(&[0u8; 0]).is_none());
        assert!(parse_block_header_le(&[1,2,3,4,5]).is_none());
    }
}
