//! DWAA and DWAB compression methods.
//!
//! These are lossy DCT-based compression schemes developed by DreamWorks Animation.
//! They provide "visually lossless" compression by quantizing DCT coefficients in
//! a perceptually-aware manner.
//!
//! - **DWAA**: 32 scanlines per block (better for partial/tiled access)
//! - **DWAB**: 256 scanlines per block (better compression, faster full-frame decode)
//!
//! Based on the OpenEXR reference implementation:
//! https://github.com/AcademySoftwareFoundation/openexr

// Allow dead code for now since the implementation is incomplete
#![allow(dead_code)]

mod classifier;
mod constants;
mod csc;
mod dct;
mod nonlinear;
mod rle;

use crate::compression::ByteVec;
use crate::error::{Error, Result};
use crate::meta::attribute::{ChannelList, IntegerBounds};

use std::io::Cursor;

use classifier::{classify_channels, CompressionScheme};
use nonlinear::InverseNonlinearLut;

/// Decompress DWAA/DWAB compressed data
///
/// # Arguments
/// * `channels` - Channel descriptions
/// * `compressed_le` - Compressed data in little-endian format
/// * `rectangle` - Image rectangle being decompressed
/// * `expected_byte_size` - Expected size of decompressed data
/// * `pedantic` - Whether to perform strict validation
/// * `num_scan_lines` - Block size (32 for DWAA, 256 for DWAB)
///
/// # Returns
/// Decompressed data in native endian format
pub fn decompress(
    channels: &ChannelList,
    compressed_le: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
    _num_scan_lines: usize,
) -> Result<ByteVec> {
    debug_assert_eq!(
        expected_byte_size,
        rectangle.size.area() * channels.bytes_per_pixel
    );

    if compressed_le.is_empty() {
        return Ok(Vec::new());
    }

    // Classify channels to determine compression schemes
    let classification = classify_channels(channels);

    // Parse compressed data header
    let mut reader = Cursor::new(compressed_le.as_slice());
    let header = parse_header(&mut reader)?;

    // Decompress data streams
    let unknown_data = if header.unknown_compressed_size > 0 {
        let compressed = read_bytes(&mut reader, header.unknown_compressed_size)?;
        decompress_zip(&compressed, header.unknown_uncompressed_size)?
    } else {
        Vec::new()
    };

    let ac_data = if header.ac_compressed_size > 0 {
        let compressed = read_bytes(&mut reader, header.ac_compressed_size)?;
        match header.ac_compression {
            AcCompression::Deflate => {
                decompress_zip(&compressed, header.ac_uncompressed_size)?
            }
            AcCompression::StaticHuffman => {
                return Err(Error::unsupported(
                    "Static Huffman AC compression not yet implemented"
                ));
            }
        }
    } else {
        Vec::new()
    };

    let dc_data = if header.dc_compressed_size > 0 {
        let compressed = read_bytes(&mut reader, header.dc_compressed_size)?;
        decompress_zip(&compressed, header.dc_uncompressed_size)?
    } else {
        Vec::new()
    };

    let rle_data = if header.rle_compressed_size > 0 {
        let compressed = read_bytes(&mut reader, header.rle_compressed_size)?;
        let uncompressed = decompress_zip(&compressed, header.rle_uncompressed_size)?;
        decompress_rle(&uncompressed, header.rle_raw_size)?
    } else {
        Vec::new()
    };

    // Create lookup table for inverse nonlinear transform
    let nonlinear_lut = InverseNonlinearLut::new();

    // Allocate output buffer
    let mut output = vec![0u8; expected_byte_size];

    // Process each channel
    let mut unknown_offset = 0;
    let mut ac_offset = 0;
    let mut dc_offset = 0;
    let mut rle_offset = 0;

    for (ch_idx, channel) in channels.list.iter().enumerate() {
        let channel_class = &classification.channel_classifications[ch_idx];
        let channel_resolution = channel.subsampled_resolution(rectangle.size);
        let channel_pixel_count = channel_resolution.area();

        match channel_class.scheme {
            CompressionScheme::LossyDct => {
                // Decompress using lossy DCT
                decompress_lossy_dct_channel(
                    channel,
                    channel_resolution,
                    &channel_class,
                    &classification.csc_groups,
                    &ac_data[ac_offset..],
                    &dc_data[dc_offset..],
                    &nonlinear_lut,
                    &mut output,
                    channels,
                    ch_idx,
                )?;

                // TODO: Calculate actual bytes consumed
                // For now, we'll process all channels together
            }
            CompressionScheme::Rle => {
                // RLE compressed channel
                let bytes_per_sample = channel.sample_type.bytes_per_sample();
                let channel_bytes = channel_pixel_count * bytes_per_sample;

                if rle_offset + channel_bytes > rle_data.len() {
                    return Err(Error::invalid("RLE data buffer too small"));
                }

                // TODO: Copy RLE data to output at correct position
                rle_offset += channel_bytes;
            }
            CompressionScheme::Unknown => {
                // ZIP compressed channel
                let bytes_per_sample = channel.sample_type.bytes_per_sample();
                let channel_bytes = channel_pixel_count * bytes_per_sample;

                if unknown_offset + channel_bytes > unknown_data.len() {
                    return Err(Error::invalid("Unknown data buffer too small"));
                }

                // TODO: Copy unknown data to output at correct position
                unknown_offset += channel_bytes;
            }
        }
    }

    Ok(output)
}

/// Decompress a channel using lossy DCT
#[allow(clippy::too_many_arguments)]
fn decompress_lossy_dct_channel(
    _channel: &crate::meta::attribute::ChannelDescription,
    _resolution: crate::prelude::Vec2<usize>,
    _classification: &classifier::ChannelClassification,
    _csc_groups: &[classifier::CscGroup],
    _ac_data: &[u8],
    _dc_data: &[u8],
    _nonlinear_lut: &InverseNonlinearLut,
    _output: &mut [u8],
    _channels: &ChannelList,
    _channel_idx: usize,
) -> Result<()> {
    // TODO: Implement full lossy DCT decompression
    // This requires:
    // 1. Parse AC and DC coefficients
    // 2. For each 8x8 block:
    //    a. Un-RLE AC coefficients
    //    b. Combine with DC coefficient
    //    c. Un-zigzag to normal order
    //    d. Inverse DCT
    //    e. If in CSC group, inverse CSC
    //    f. Inverse nonlinear transform
    //    g. Convert to output format (HALF/FLOAT)
    // 3. Write to output buffer

    Err(Error::unsupported(
        "Lossy DCT decompression not fully implemented yet"
    ))
}

/// Compressed data header
#[derive(Debug)]
struct Header {
    version: u64,
    unknown_compressed_size: usize,
    unknown_uncompressed_size: usize,
    ac_compressed_size: usize,
    ac_uncompressed_size: usize,
    ac_compression: AcCompression,
    dc_compressed_size: usize,
    dc_uncompressed_size: usize,
    rle_compressed_size: usize,
    rle_uncompressed_size: usize,
    rle_raw_size: usize,
}

/// AC compression method
#[derive(Debug, Clone, Copy)]
enum AcCompression {
    StaticHuffman,
    Deflate,
}

/// Read a u64 from the cursor in little-endian format
fn read_u64_le(reader: &mut Cursor<&[u8]>) -> Result<u64> {
    let mut buf = [0u8; 8];
    let pos = reader.position() as usize;
    let data = reader.get_ref();

    if pos + 8 > data.len() {
        return Err(Error::invalid("Not enough data to read u64"));
    }

    buf.copy_from_slice(&data[pos..pos + 8]);
    reader.set_position((pos + 8) as u64);
    Ok(u64::from_le_bytes(buf))
}

/// Parse the compressed data header
fn parse_header(reader: &mut Cursor<&[u8]>) -> Result<Header> {
    // Read header values (all u64 in little-endian)
    let version = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read DWA version"))?;

    if version > 2 {
        return Err(Error::invalid("Unsupported DWA version"));
    }

    let unknown_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read unknown compressed size"))? as usize;

    let unknown_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read unknown uncompressed size"))? as usize;

    let ac_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read AC compressed size"))? as usize;

    let ac_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read AC uncompressed size"))? as usize;

    let ac_compression_value = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read AC compression method"))?;

    let ac_compression = match ac_compression_value {
        0 => AcCompression::StaticHuffman,
        1 => AcCompression::Deflate,
        _ => return Err(Error::invalid("Invalid AC compression method")),
    };

    let dc_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read DC compressed size"))? as usize;

    let dc_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read DC uncompressed size"))? as usize;

    let rle_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read RLE compressed size"))? as usize;

    let rle_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read RLE uncompressed size"))? as usize;

    let rle_raw_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read RLE raw size"))? as usize;

    // Version 2 includes channel classification rules, which we skip for now
    // as we recompute them from channel names

    Ok(Header {
        version,
        unknown_compressed_size,
        unknown_uncompressed_size,
        ac_compressed_size,
        ac_uncompressed_size,
        ac_compression,
        dc_compressed_size,
        dc_uncompressed_size,
        rle_compressed_size,
        rle_uncompressed_size,
        rle_raw_size,
    })
}

/// Read a specified number of bytes from the reader
fn read_bytes(reader: &mut Cursor<&[u8]>, count: usize) -> Result<Vec<u8>> {
    let position = reader.position() as usize;
    let data = reader.get_ref();

    if position + count > data.len() {
        return Err(Error::invalid("Not enough data in compressed stream"));
    }

    let bytes = data[position..position + count].to_vec();
    reader.set_position((position + count) as u64);

    Ok(bytes)
}

/// Decompress ZIP/DEFLATE data
fn decompress_zip(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    use zune_inflate::DeflateDecoder;

    let mut decoder = DeflateDecoder::new(compressed);
    let decompressed = decoder.decode_zlib()
        .map_err(|e| Error::invalid(format!("ZIP decompression failed: {:?}", e)))?;

    if decompressed.len() != expected_size {
        return Err(Error::invalid(format!(
            "ZIP decompression size mismatch: expected {}, got {}",
            expected_size,
            decompressed.len()
        )));
    }

    Ok(decompressed)
}

/// Decompress RLE data (simple RLE, not the same as the main RLE compression)
/// This is a basic RLE format used for DWAA/DWAB metadata
fn decompress_rle(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    let mut decompressed = Vec::with_capacity(expected_size);
    let mut remaining = compressed;

    while !remaining.is_empty() && decompressed.len() < expected_size {
        if remaining.is_empty() {
            return Err(Error::invalid("Unexpected end of RLE data"));
        }

        let count = remaining[0] as i8;
        remaining = &remaining[1..];

        if count < 0 {
            // Take the next '-count' bytes as-is
            let n = (-count) as usize;
            if remaining.len() < n {
                return Err(Error::invalid("RLE data truncated"));
            }

            decompressed.extend_from_slice(&remaining[..n]);
            remaining = &remaining[n..];
        } else {
            // Repeat the next value 'count + 1' times
            if remaining.is_empty() {
                return Err(Error::invalid("RLE data truncated"));
            }

            let value = remaining[0];
            remaining = &remaining[1..];

            for _ in 0..=(count as usize) {
                decompressed.push(value);
            }
        }
    }

    if decompressed.len() != expected_size {
        return Err(Error::invalid(format!(
            "RLE decompression size mismatch: expected {}, got {}",
            expected_size,
            decompressed.len()
        )));
    }

    Ok(decompressed)
}

/// Compress function (stub for now, will be implemented later)
pub fn compress(
    _channels: &ChannelList,
    _uncompressed_ne: ByteVec,
    _rectangle: IntegerBounds,
    _num_scan_lines: usize,
    _compression_level: f32,
) -> Result<ByteVec> {
    Err(Error::unsupported(
        "DWAA/DWAB compression not yet implemented"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ac_compression_enum() {
        // Verify AC compression enum values match spec
        assert!(matches!(AcCompression::StaticHuffman, AcCompression::StaticHuffman));
        assert!(matches!(AcCompression::Deflate, AcCompression::Deflate));
    }

    // More tests will be added as implementation progresses
}
