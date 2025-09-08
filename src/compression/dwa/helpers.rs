#![allow(dead_code, trivial_numeric_casts)]
//! Helpers for DWA encoder/decoder (ported from OpenEXR Core internal_dwa_helpers.h)
//! Bit I/O, zig-zag, and small math utilities used by the DWA codec.

use core::cmp::{max, min};

/// Simple bit reader over a byte slice (little-endian bytes, MSB-first within each byte).
/// This matches the common packing used in OpenEXR's DWA bitstreams where codes are read MSB-first.
#[derive(Clone, Debug)]
pub(crate) struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize, // absolute bit index into data (0..data.len()*8)
}

impl<'a> BitReader<'a> {
    /// Create a new reader over the given data
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    /// Current bit offset
    pub(crate) fn bit_position(&self) -> usize { self.bit_pos }

    /// Total number of bits available
    pub(crate) fn bit_len(&self) -> usize { self.data.len() * 8 }

    /// Remaining bits
    pub(crate) fn remaining_bits(&self) -> usize { self.bit_len().saturating_sub(self.bit_pos) }

    /// Is the current position on a byte boundary?
    pub(crate) fn is_byte_aligned(&self) -> bool { (self.bit_pos & 7) == 0 }

    /// Remaining whole bytes from the current position (rounded down)
    pub(crate) fn remaining_bytes(&self) -> usize { self.remaining_bits() / 8 }

    /// Align to next byte boundary
    pub(crate) fn align_to_byte(&mut self) {
        let rem = self.bit_pos & 7;
        if rem != 0 { self.bit_pos += 8 - rem; }
    }

    /// Read up to 32 bits (MSB-first) and return as u32. Returns None if not enough bits.
    pub(crate) fn read_bits(&mut self, n: u8) -> Option<u32> {
        debug_assert!(n <= 32);
        if n == 0 { return Some(0); }
        if self.remaining_bits() < n as usize { return None; }

        let mut bits_left = n as usize;
        let mut value: u32 = 0;

        while bits_left > 0 {
            let byte_index = self.bit_pos >> 3;
            let bit_index_in_byte = self.bit_pos & 7; // 0..7 (0 is MSB)
            let available_in_byte = 8 - bit_index_in_byte;
            let take = min(bits_left, available_in_byte);

            let byte = self.data[byte_index];
            // Extract [bit_index_in_byte .. bit_index_in_byte+take) MSB-first
            let shift = (available_in_byte - take) as u32;
            let mask = (!0u8 >> (8 - take)) as u8;
            let part = ((byte >> shift) & mask) as u32;

            value = (value << take) | part;
            self.bit_pos += take;
            bits_left -= take;
        }

        Some(value)
    }

    /// Read a signed value of `bits` width and return as i32, using two's complement sign extension.
    pub(crate) fn read_signed(&mut self, bits: u8) -> Option<i32> {
        if bits == 0 { return Some(0); }
        if bits > 31 { return None; }
        let raw = self.read_bits(bits)?;
        Some(sign_extend(raw, bits))
    }

    /// Peek up to 32 bits without advancing. Returns None if not enough bits.
    pub(crate) fn peek_bits(&self, n: u8) -> Option<u32> {
        debug_assert!(n <= 32);
        if n == 0 { return Some(0); }
        if self.remaining_bits() < n as usize { return None; }
        let mut pos = self.bit_pos;
        let mut bits_left = n as usize;
        let mut value: u32 = 0;
        while bits_left > 0 {
            let byte_index = pos >> 3;
            let bit_index_in_byte = pos & 7;
            let available_in_byte = 8 - bit_index_in_byte;
            let take = core::cmp::min(bits_left, available_in_byte);
            let byte = self.data[byte_index];
            let shift = (available_in_byte - take) as u32;
            let mask = (!0u8 >> (8 - take)) as u8;
            let part = ((byte >> shift) & mask) as u32;
            value = (value << take) | part;
            pos += take;
            bits_left -= take;
        }
        Some(value)
    }

    /// Skip n bits; returns false if not enough bits to skip all, true if skipped fully.
    pub(crate) fn skip_bits(&mut self, n: usize) -> bool {
        if self.remaining_bits() < n { return false; }
        self.bit_pos += n;
        true
    }

    /// Skip whole bytes; only works on byte boundary.
    pub(crate) fn skip_bytes(&mut self, n: usize) -> bool {
        if !self.is_byte_aligned() { return false; }
        let bits = n.checked_mul(8).unwrap_or(usize::MAX);
        self.skip_bits(bits)
    }

    /// Read a single bit
    pub(crate) fn read_bit(&mut self) -> Option<u32> { self.read_bits(1) }

    /// Read bytes directly when on byte boundary
    pub(crate) fn read_bytes(&mut self, out: &mut [u8]) -> Option<()> {
        if (self.bit_pos & 7) != 0 { return None; }
        let start = self.bit_pos >> 3;
        let end = start.checked_add(out.len())?;
        if end > self.data.len() { return None; }
        out.copy_from_slice(&self.data[start..end]);
        self.bit_pos += out.len() * 8;
        Some(())
    }

    /// Get a slice from the current position (must be byte-aligned)
    pub(crate) fn as_slice_from_byte(&self) -> Option<&'a [u8]> {
        if !self.is_byte_aligned() { return None; }
        let start = self.bit_pos >> 3;
        Some(&self.data[start..])
    }
}


/// Clamp helper
#[inline]
pub(crate) fn clamp_i32(v: i32, lo: i32, hi: i32) -> i32 { max(lo, min(hi, v)) }

/// Sign-extend a value with given bit width to i32
#[inline]
pub(crate) fn sign_extend(value: u32, bits: u8) -> i32 {
    debug_assert!(bits > 0 && bits <= 31);
    let shift = 32 - bits;
    ((value << shift) as i32) >> shift
}

/// Apply dequantization scale (placeholder for now)
#[inline]
pub(crate) fn dequant(coef: i32, q: i32) -> i32 { coef * q }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitreader_read_and_peek() {
        // bytes: 0b1010_1010, 0b1100_0000
        let data = [0b1010_1010u8, 0b1100_0000u8];
        let mut br = BitReader::new(&data);
        assert_eq!(br.peek_bits(4), Some(0b1010));
        assert_eq!(br.read_bits(4), Some(0b1010));
        assert_eq!(br.read_bits(4), Some(0b1010));
        // Next two bits span into next byte: expect 11
        assert_eq!(br.peek_bits(2), Some(0b11));
        assert_eq!(br.read_bits(2), Some(0b11));
        // Skip the remaining bits and ensure we are at the end
        let rem = br.remaining_bits();
        assert!(br.skip_bits(rem));
        assert_eq!(br.remaining_bits(), 0);
    }

    #[test]
    fn bitreader_align_and_bytes() {
        let data = [0x12u8, 0x34u8, 0x56u8];
        let mut br = BitReader::new(&data);
        assert!(br.is_byte_aligned());
        // Read 3 bits (value depends on MSB ordering; we don't validate it here)
        assert!(br.read_bits(3).is_some());
        // The key is alignment behavior
        assert!(!br.is_byte_aligned());
        br.align_to_byte();
        assert!(br.is_byte_aligned());
        // Now we should be at the second byte (since 3 bits then aligned to 8)
        let mut out = [0u8; 2];
        assert!(br.read_bytes(&mut out).is_some());
        assert_eq!(out, [0x34, 0x56]);
        assert_eq!(br.remaining_bits(), 0);
    }

    #[test]
    fn sign_extend_and_clamp() {
        // For 5 bits, 0b11111 should be -1
        assert_eq!(sign_extend(0b1_1111, 5), -1);
        // For 5 bits, 0b01010 should be 10
        assert_eq!(sign_extend(0b0_1010, 5), 10);
        assert_eq!(clamp_i32(10, 0, 5), 5);
        assert_eq!(clamp_i32(-3, 0, 5), 0);
    }

    // Pack bits MSB-first into a Vec<u8> (testing helper)
    fn pack_bits_msb_first(bits: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cur: u8 = 0;
        let mut n: u8 = 0;
        for &b in bits {
            cur = (cur << 1) | (b & 1);
            n += 1;
            if n == 8 { out.push(cur); cur = 0; n = 0; }
        }
        if n != 0 { out.push(cur << (8 - n)); }
        out
    }

    #[test]
    fn bitreader_read_signed_basic() {
        // Encode two 5-bit signed values: -1 (11111) and +10 (01010)
        let bits = [1,1,1,1,1, 0,1,0,1,0];
        let data = pack_bits_msb_first(&bits);
        let mut br = BitReader::new(&data);
        let a = br.read_signed(5).unwrap();
        let b = br.read_signed(5).unwrap();
        assert_eq!(a, -1);
        assert_eq!(b, 10);
    }
}



/// Zig-zag scan order for an 8x8 block (maps zig-zag index -> natural linear index 0..63).
pub(crate) static ZIGZAG_8X8: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// Inverse zig-zag: returns (row, col) from zig-zag index
#[inline]
pub(crate) fn inverse_zigzag_index(idx: usize) -> (usize, usize) {
    let lin = ZIGZAG_8X8[idx];
    (lin / 8, lin % 8)
}


/// Parse unsigned LEB128 from a byte slice; returns (value, bytes_consumed) or None on overflow/truncation.
pub(crate) fn parse_uleb128(mut input: &[u8]) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut used = 0usize;
    while !input.is_empty() {
        let byte = input[0] as u64;
        input = &input[1..];
        used += 1;
        let low = byte & 0x7F;
        if shift >= 64 || (low << shift) >> shift != low { return None; }
        result |= low << shift;
        if (byte & 0x80) == 0 { return Some((result, used)); }
        shift += 7;
        if shift >= 64 { return None; }
    }
    None
}

#[cfg(test)]
mod uleb_tests {
    use super::parse_uleb128;

    #[test]
    fn parse_small_values() {
        assert_eq!(parse_uleb128(&[0x00]), Some((0,1)));
        assert_eq!(parse_uleb128(&[0x7f]), Some((127,1)));
        assert_eq!(parse_uleb128(&[0x80, 0x01]), Some((128,2)));
        assert_eq!(parse_uleb128(&[0xE5, 0x8E, 0x26]), Some((624485,3)));
    }

    #[test]
    fn parse_truncated() {
        // Continuation bit set but missing next byte
        assert_eq!(parse_uleb128(&[0x80]), None);
    }
}
