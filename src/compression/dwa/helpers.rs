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

