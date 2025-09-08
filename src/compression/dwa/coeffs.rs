//! DWA coefficient stream scaffolding.
//! This module will contain coefficient unpacking and 8x8 block reconstruction.

use crate::error::{Error, Result};
use super::helpers::{ZIGZAG_8X8, BitReader};

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

/// Place coefficients given in zig-zag order into a destination 8x8 i32 block in natural order.
/// Any missing coefficients (if src shorter than 64) are filled with 0; extra coefficients are ignored.
#[allow(dead_code)]
pub(crate) fn place_coefficients_izigzag_i32(dst: &mut [i32; 64], src: &[i32]) {
    // zero-initialize
    for v in dst.iter_mut() { *v = 0; }
    let n = core::cmp::min(64, src.len());
    for i in 0..n {
        let natural_idx = ZIGZAG_8X8[i];
        dst[natural_idx] = src[i];
    }
}

/// Scalar dequantization: multiply all dst coefficients by q.
#[allow(dead_code)]
pub(crate) fn dequant_apply_scalar(dst: &mut [i32; 64], q: i32) {
    for v in dst.iter_mut() { *v *= q; }
}

/// AC run-length pair used in JPEG-like streams: `run` zeros followed by `value`.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct AcPair { pub run: u8, pub value: i32 }

/// Apply AC run-length/value pairs into a destination block in zig-zag order (starting at index `start_idx`).
/// Returns the next zig-zag index after writing. Zeros are inserted for runs; remaining entries are unchanged.
#[allow(dead_code)]
pub(crate) fn apply_ac_pairs(dst: &mut [i32; 64], start_idx: usize, pairs: &[AcPair]) -> usize {
    let mut idx = start_idx.min(64);
    for &AcPair { run, value } in pairs {
        let advance = run as usize;
        if idx.saturating_add(advance) >= 64 { return 64; }
        idx += advance;
        let nat = ZIGZAG_8X8[idx];
        dst[nat] = value;
        idx += 1;
        if idx >= 64 { break; }
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::dwa::helpers::ZIGZAG_8X8;

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

    #[test]
    fn izigzag_places_natural_order() {
        // natural block is 0..63
        let natural: [i32; 64] = core::array::from_fn(|i| i as i32);
        // create zig-zag ordered src
        let mut src = vec![0i32; 64];
        for i in 0..64 { src[i] = natural[ZIGZAG_8X8[i]]; }
        let mut dst = [0i32; 64];
        place_coefficients_izigzag_i32(&mut dst, &src);
        assert_eq!(dst, natural);
    }

    #[test]
    fn izigzag_truncates_and_zero_fills() {
        let mut dst = [-1i32; 64];
        let src = [5i32; 10]; // shorter than 64
        place_coefficients_izigzag_i32(&mut dst, &src);
        // First 10 placed at zig positions
        for i in 0..10 { assert_eq!(dst[ZIGZAG_8X8[i]], 5); }
        // Others must be zero
        for i in 10..64 { assert_eq!(dst[ZIGZAG_8X8[i]], 0); }
    }
}
