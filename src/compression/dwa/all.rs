// src/compression/dwa.rs
// First-port, single-file DWA implementation for exrs (prototype -> stepwise refinement)
// - All code in one file as requested
// - Implements DWAA/DWAB block handling, delta transform across scanlines,
//   exact-deflate payloads, and full roundtrip unit tests.
// - This is a stepping stone toward a byte-for-byte OpenEXRCore-compatible
//   implementation. Next steps will be to port exact bit-packing and header
//   layout from OpenEXRCore C sources.

use crate::compression::ByteVec;
use crate::meta::attribute::ChannelList;
use crate::prelude::{Error, IntegerBounds};

/// DWA compression variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DwaVariant {
    Dwaa, // 32 scanlines
    Dwab, // 256 scanlines
}

impl DwaVariant {
    pub fn block_size_lines(&self) -> usize {
        match self {
            DwaVariant::Dwaa => 32,
            DwaVariant::Dwab => 256,
        }
    }
}

// === API glue: hook into exrs' existing compress/decompress entry points ===
// These wrappers compute the scanline geometry from the given parameters and
// forward to our internal single-file compressor/decompressor.

/// Decompress DWA payload into native-endian pixel bytes.
pub(crate) fn decompress(
    _channels: &ChannelList,
    compressed_le: ByteVec,
    pixel_section: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> crate::error::Result<ByteVec> {
    let scanlines = pixel_section.size.y().max(1);
    if expected_byte_size % scanlines != 0 {
        return Err(Error::invalid("expected_byte_size is not a multiple of scanline count"));
    }
    let bytes_per_scanline = expected_byte_size / scanlines;

    // Variant does not affect decoding in this prototype because each block
    // stores its own line count. Use DWAA as a neutral default.
    let out = decompress_dwa(DwaVariant::Dwaa, bytes_per_scanline, scanlines, &compressed_le)?;
    Ok(out)
}

/// Compress a native-endian pixel block into DWA (DWAA/DWAB) encoded little-endian bytes.
pub(crate) fn compress(
    _channels: &ChannelList,
    uncompressed_ne: ByteVec,
    pixel_section: IntegerBounds,
    is_dwab: bool,
    level: Option<f32>,
) -> crate::error::Result<ByteVec> {
    let scanlines = pixel_section.size.y().max(1);
    if uncompressed_ne.len() % scanlines != 0 {
        return Err(Error::invalid("input byte length is not a multiple of scanline count"));
    }
    let bytes_per_scanline = uncompressed_ne.len() / scanlines;
    let variant = if is_dwab { DwaVariant::Dwab } else { DwaVariant::Dwaa };
    let quality = map_level_option_to_quality(level);

    let out = compress_dwa(variant, quality, bytes_per_scanline, scanlines, &uncompressed_ne)?;
    Ok(out)
}


/// High-level public compress API (single-file first port)
/// - `variant`: DWAA or DWAB
/// - `level`: 0..=100 quality parameter (mapped to deflate level for now)
/// - `bytes_per_scanline`: bytes in one scanline (width * bytes_per_pixel)
/// - `scanlines`: number of scanlines in input
/// - `input`: contiguous scanline bytes top-to-bottom
pub fn compress_dwa(
    variant: DwaVariant,
    level: u8,
    bytes_per_scanline: usize,
    scanlines: usize,
    input: &[u8],
) -> Result<Vec<u8>, Error> {
    if input.len() != bytes_per_scanline
        .checked_mul(scanlines)
        .ok_or_else(|| Error::invalid("size overflow"))?
    {
        return Err(Error::invalid("input length mismatch"));
    }

    let deflate_level = map_quality_to_deflate(level);
    let mut out = Vec::new();
    let block_lines = variant.block_size_lines();
    let mut offset = 0usize;

    while offset < scanlines {
        let lines_in_block = std::cmp::min(block_lines, scanlines - offset);
        let bytes_in_block = lines_in_block * bytes_per_scanline;
        let block_slice = &input[offset * bytes_per_scanline..offset * bytes_per_scanline + bytes_in_block];

        // transform
        let transformed = delta_transform_block(bytes_per_scanline, lines_in_block, block_slice);

        // pack (simple contiguous layout for v1)
        let packed = transformed;

        // compress
        let compressed = deflate_compress(&packed, deflate_level)?;

        // Block header (simple; exact OpenEXR layout will be implemented later)
        // [u32 lines][u32 uncompressed_len][u32 compressed_len]
        out.extend_from_slice(&(lines_in_block as u32).to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
        out.extend_from_slice(&compressed);

        offset += lines_in_block;
    }

    Ok(out)
}

/// High-level decompress API
pub fn decompress_dwa(
    _variant: DwaVariant,
    bytes_per_scanline: usize,
    scanlines: usize,
    input: &[u8],
) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(bytes_per_scanline * scanlines);
    let mut cursor = 0usize;
    let mut produced_lines = 0usize;

    while cursor < input.len() {
        if cursor + 12 > input.len() {
            return Err(Error::invalid("truncated block header"));
        }
        let lines_in_block =
            u32::from_le_bytes([input[cursor], input[cursor + 1], input[cursor + 2], input[cursor + 3]]) as usize;
        let uncompressed_len = u32::from_le_bytes([
            input[cursor + 4],
            input[cursor + 5],
            input[cursor + 6],
            input[cursor + 7],
        ]) as usize;
        let compressed_len = u32::from_le_bytes([
            input[cursor + 8],
            input[cursor + 9],
            input[cursor + 10],
            input[cursor + 11],
        ]) as usize;
        cursor += 12;

        if cursor + compressed_len > input.len() {
            return Err(Error::invalid("truncated compressed payload"));
        }

        let compressed = &input[cursor..cursor + compressed_len];
        cursor += compressed_len;

        let inflated = deflate_decompress(compressed, uncompressed_len)?;
        let block_bytes = inverse_delta_transform_block(bytes_per_scanline, lines_in_block, &inflated)?;

        out.extend_from_slice(&block_bytes);
        produced_lines += lines_in_block;
    }

    if produced_lines != scanlines {
        return Err(Error::invalid("mismatch in produced scanline count"));
    }

    Ok(out)
}

fn map_level_option_to_quality(level: Option<f32>) -> u8 {
    let clamped = level.unwrap_or(45.0).clamp(0.0, 100.0);
    clamped as u8
}

fn map_quality_to_deflate(level: u8) -> u8 {
    // Placeholder mapping: tune later to match OpenEXR defaults
    match level {
        0..=10 => 2,
        11..=50 => 4,
        51..=80 => 6,
        81..=100 => 8,
        _ => 4,
    }
}

fn delta_transform_block(bytes_per_scanline: usize, lines_in_block: usize, block: &[u8]) -> Vec<u8> {
    assert_eq!(block.len(), bytes_per_scanline * lines_in_block);
    let mut out = Vec::with_capacity(block.len());
    for pos in 0..bytes_per_scanline {
        let first = block[pos];
        out.push(first);
        let mut prev = first;
        for line in 1..lines_in_block {
            let idx = line * bytes_per_scanline + pos;
            let this = block[idx];
            let delta = this.wrapping_sub(prev);
            out.push(delta);
            prev = this;
        }
    }
    out
}

fn inverse_delta_transform_block(
    bytes_per_scanline: usize,
    lines_in_block: usize,
    transformed: &[u8],
) -> Result<Vec<u8>, Error> {
    if transformed.len() != bytes_per_scanline * lines_in_block {
        return Err(Error::invalid("transformed length mismatch"));
    }
    let mut out = vec![0u8; bytes_per_scanline * lines_in_block];
    for pos in 0..bytes_per_scanline {
        let base = pos * lines_in_block;
        let first = transformed[base];
        out[pos] = first;
        let mut prev = first;
        for line in 1..lines_in_block {
            let t = transformed[base + line];
            let val = prev.wrapping_add(t);
            let idx = line * bytes_per_scanline + pos;
            out[idx] = val;
            prev = val;
        }
    }
    Ok(out)
}

fn deflate_compress(input: &[u8], level: u8) -> Result<Vec<u8>, Error> {
    Ok(miniz_oxide::deflate::compress_to_vec_zlib(input, level))
}

fn deflate_decompress(input: &[u8], expected_uncompressed_len: usize) -> Result<Vec<u8>, Error> {
    let options = zune_inflate::DeflateOptions::default()
        // .set_limit(expected_uncompressed_len)
        .set_size_hint(expected_uncompressed_len);

    let mut decoder = zune_inflate::DeflateDecoder::new_with_options(input, options);

    decoder
        .decode_zlib()
        .map_err(|_| Error::invalid("zlib-compressed data malformed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    #[test]
    fn test_delta_roundtrip_small() {
        let bytes_per_scanline = 7;
        let lines = 5;
        let mut rng = StdRng::seed_from_u64(42);
        let mut buf = vec![0u8; bytes_per_scanline * lines];
        for b in buf.iter_mut() {
            *b = rng.gen();
        }

        let t = delta_transform_block(bytes_per_scanline, lines, &buf);
        let inv = inverse_delta_transform_block(bytes_per_scanline, lines, &t).unwrap();
        assert_eq!(buf, inv);
    }

    #[test]
    fn test_single_block_roundtrip() {
        let bytes_per_scanline = 16;
        let lines = 32;
        let mut rng = StdRng::seed_from_u64(12345);
        let mut buf = vec![0u8; bytes_per_scanline * lines];
        for b in buf.iter_mut() {
            *b = rng.gen();
        }

        let cmp = compress_dwa(DwaVariant::Dwaa, 45, bytes_per_scanline, lines, &buf).unwrap();
        let dec = decompress_dwa(DwaVariant::Dwaa, bytes_per_scanline, lines, &cmp).unwrap();
        assert_eq!(buf, dec);
    }

    #[test]
    fn test_multi_block_roundtrip() {
        let bytes_per_scanline = 8;
        let lines = 100;
        let mut rng = StdRng::seed_from_u64(2024);
        let mut buf = vec![0u8; bytes_per_scanline * lines];
        for b in buf.iter_mut() {
            *b = rng.gen();
        }

        let cmp = compress_dwa(DwaVariant::Dwaa, 45, bytes_per_scanline, lines, &buf).unwrap();
        let dec = decompress_dwa(DwaVariant::Dwaa, bytes_per_scanline, lines, &cmp).unwrap();
        assert_eq!(buf, dec);
    }

    #[test]
    fn test_dwab_block_roundtrip() {
        let bytes_per_scanline = 12;
        let lines = 260;
        let mut rng = StdRng::seed_from_u64(2025);
        let mut buf = vec![0u8; bytes_per_scanline * lines];
        for b in buf.iter_mut() {
            *b = rng.gen();
        }

        let cmp = compress_dwa(DwaVariant::Dwab, 60, bytes_per_scanline, lines, &buf).unwrap();
        let dec = decompress_dwa(DwaVariant::Dwab, bytes_per_scanline, lines, &cmp).unwrap();
        assert_eq!(buf, dec);
    }
}
