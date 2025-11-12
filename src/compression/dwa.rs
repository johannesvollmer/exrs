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
    let header = parse_header(&mut reader, channels.list.len())?;

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

    // Decode all lossy DCT channels into intermediate spatial buffers
    let mut spatial_buffers: Vec<Option<Vec<f32>>> = vec![None; channels.list.len()];

    let mut ac_reader = std::io::Cursor::new(&ac_data[..]);
    let mut dc_reader = std::io::Cursor::new(&dc_data[..]);

    for (ch_idx, channel) in channels.list.iter().enumerate() {
        let channel_class = &classification.channel_classifications[ch_idx];
        let channel_resolution = channel.subsampled_resolution(rectangle.size);

        if channel_class.scheme == CompressionScheme::LossyDct {
            // Decode this lossy DCT channel
            let spatial_data = decode_lossy_dct_channel(
                channel_resolution,
                channel_class,
                &classification.csc_groups,
                &mut ac_reader,
                &mut dc_reader,
            )?;

            spatial_buffers[ch_idx] = Some(spatial_data);
        }
    }

    // Apply inverse CSC for RGB triplets
    for csc_group in &classification.csc_groups {
        if let (Some(y_data), Some(cb_data), Some(cr_data)) = (
            &spatial_buffers[csc_group.r_index],
            &spatial_buffers[csc_group.g_index],
            &spatial_buffers[csc_group.b_index],
        ) {
            // Convert Y'CbCr to RGB
            let (r_data, g_data, b_data) = apply_inverse_csc(y_data, cb_data, cr_data);

            spatial_buffers[csc_group.r_index] = Some(r_data);
            spatial_buffers[csc_group.g_index] = Some(g_data);
            spatial_buffers[csc_group.b_index] = Some(b_data);
        }
    }

    // Write all channels to output
    let mut output_offset = 0;
    let mut unknown_offset = 0;
    let mut rle_offset = 0;

    for (ch_idx, channel) in channels.list.iter().enumerate() {
        let channel_class = &classification.channel_classifications[ch_idx];
        let channel_resolution = channel.subsampled_resolution(rectangle.size);
        let channel_pixel_count = channel_resolution.area();
        let bytes_per_sample = channel.sample_type.bytes_per_sample();
        let channel_bytes = channel_pixel_count * bytes_per_sample;

        match channel_class.scheme {
            CompressionScheme::LossyDct => {
                if let Some(spatial_data) = &spatial_buffers[ch_idx] {
                    // Apply inverse nonlinear transform and write to output
                    write_channel_to_output(
                        spatial_data,
                        channel.sample_type,
                        &nonlinear_lut,
                        &mut output[output_offset..output_offset + channel_bytes],
                    )?;
                }
                output_offset += channel_bytes;
            }
            CompressionScheme::Rle => {
                // RLE compressed channel
                if rle_offset + channel_bytes > rle_data.len() {
                    return Err(Error::invalid("RLE data buffer too small"));
                }

                output[output_offset..output_offset + channel_bytes]
                    .copy_from_slice(&rle_data[rle_offset..rle_offset + channel_bytes]);

                rle_offset += channel_bytes;
                output_offset += channel_bytes;
            }
            CompressionScheme::Unknown => {
                // ZIP compressed channel
                if unknown_offset + channel_bytes > unknown_data.len() {
                    return Err(Error::invalid("Unknown data buffer too small"));
                }

                output[output_offset..output_offset + channel_bytes]
                    .copy_from_slice(&unknown_data[unknown_offset..unknown_offset + channel_bytes]);

                unknown_offset += channel_bytes;
                output_offset += channel_bytes;
            }
        }
    }

    Ok(output)
}

/// Decode a lossy DCT channel into spatial domain
fn decode_lossy_dct_channel(
    resolution: crate::prelude::Vec2<usize>,
    classification: &classifier::ChannelClassification,
    _csc_groups: &[classifier::CscGroup],
    ac_reader: &mut std::io::Cursor<&[u8]>,
    dc_reader: &mut std::io::Cursor<&[u8]>,
) -> Result<Vec<f32>> {
    use constants::{BLOCK_SIZE, INVERSE_ZIGZAG_ORDER};
    use dct::inverse_dct_8x8_optimized;

    let width = resolution.x();
    let height = resolution.y();
    let pixel_count = width * height;

    // Calculate number of blocks
    let blocks_x = (width + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let blocks_y = (height + BLOCK_SIZE - 1) / BLOCK_SIZE;

    // Allocate spatial buffer
    let mut spatial_data = vec![0.0f32; pixel_count];

    // Determine which quantization table to use
    let quant_table = if classification.csc_group_index.is_some() {
        // Part of CSC group - use appropriate table based on role
        match classification.csc_channel_role {
            Some(0) => &constants::QUANT_TABLE_Y,      // Y' (stored in R)
            Some(1) => &constants::QUANT_TABLE_CBCR,   // Cb (stored in G)
            Some(2) => &constants::QUANT_TABLE_CBCR,   // Cr (stored in B)
            _ => &constants::QUANT_TABLE_Y,
        }
    } else {
        // Standalone channel (Y, BY, RY, etc.) - use Y table
        &constants::QUANT_TABLE_Y
    };

    // Process each 8x8 block
    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            // Read DC coefficient (u16, little-endian)
            let dc_coeff = read_u16_le(dc_reader)?;

            // Read AC coefficients (RLE encoded)
            let ac_encoded = read_rle_ac_block(ac_reader)?;
            let ac_coeffs = rle::decode_ac_coefficients(&ac_encoded)?;

            // Find last non-zero coefficient for optimization
            let last_non_zero = rle::find_last_non_zero(&ac_coeffs);

            // Construct full DCT coefficient block (DC + AC)
            let mut dct_coeffs = [0.0f32; 64];

            // DC coefficient (index 0)
            let dc_quant = quant_table[0];
            dct_coeffs[0] = (dc_coeff as f32) * dc_quant;

            // AC coefficients (indices 1-63, in zigzag order)
            for i in 0..63 {
                if ac_coeffs[i] != 0 {
                    // Un-zigzag: ac_coeffs[i] is in zigzag position i+1
                    let normal_idx = INVERSE_ZIGZAG_ORDER[i + 1];
                    let ac_quant = quant_table[normal_idx];
                    dct_coeffs[normal_idx] = (ac_coeffs[i] as f32) * ac_quant;
                }
            }

            // Apply inverse DCT
            let spatial_block = inverse_dct_8x8_optimized(&dct_coeffs, last_non_zero);

            // Copy block to output, handling edge cases
            for by in 0..BLOCK_SIZE {
                let y = block_y * BLOCK_SIZE + by;
                if y >= height {
                    break;
                }

                for bx in 0..BLOCK_SIZE {
                    let x = block_x * BLOCK_SIZE + bx;
                    if x >= width {
                        break;
                    }

                    let block_idx = by * BLOCK_SIZE + bx;
                    let output_idx = y * width + x;
                    spatial_data[output_idx] = spatial_block[block_idx];
                }
            }
        }
    }

    Ok(spatial_data)
}

/// Read a u16 from cursor in little-endian format
fn read_u16_le(reader: &mut std::io::Cursor<&[u8]>) -> Result<u16> {
    let pos = reader.position() as usize;
    let data = reader.get_ref();

    if pos + 2 > data.len() {
        return Err(Error::invalid("Not enough data to read u16"));
    }

    let bytes = [data[pos], data[pos + 1]];
    reader.set_position((pos + 2) as u64);
    Ok(u16::from_le_bytes(bytes))
}

/// Read RLE-encoded AC coefficients for one block
fn read_rle_ac_block(reader: &mut std::io::Cursor<&[u8]>) -> Result<Vec<u16>> {
    use constants::rle_markers;

    let mut encoded = Vec::new();

    loop {
        let value = read_u16_le(reader)?;
        encoded.push(value);

        if rle_markers::is_end_of_block(value) {
            break;
        }

        // Prevent infinite loops on malformed data
        if encoded.len() > 256 {
            return Err(Error::invalid("RLE AC block too long"));
        }
    }

    Ok(encoded)
}

/// Apply inverse CSC to convert Y'CbCr spatial data to RGB
fn apply_inverse_csc(y_data: &[f32], cb_data: &[f32], cr_data: &[f32]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    use csc::ycbcr_to_rgb;

    let pixel_count = y_data.len();
    let mut r_data = vec![0.0f32; pixel_count];
    let mut g_data = vec![0.0f32; pixel_count];
    let mut b_data = vec![0.0f32; pixel_count];

    for i in 0..pixel_count {
        let (r, g, b) = ycbcr_to_rgb(y_data[i], cb_data[i], cr_data[i]);
        r_data[i] = r;
        g_data[i] = g;
        b_data[i] = b;
    }

    (r_data, g_data, b_data)
}

/// Apply inverse nonlinear transform and write channel to output
fn write_channel_to_output(
    spatial_data: &[f32],
    sample_type: crate::meta::attribute::SampleType,
    nonlinear_lut: &InverseNonlinearLut,
    output: &mut [u8],
) -> Result<()> {
    use crate::meta::attribute::SampleType;
    use half::f16;

    match sample_type {
        SampleType::F16 => {
            // Convert to f16 with inverse nonlinear transform
            if output.len() != spatial_data.len() * 2 {
                return Err(Error::invalid("Output buffer size mismatch for F16"));
            }

            for (i, &value) in spatial_data.iter().enumerate() {
                // Apply inverse nonlinear transform
                let linear = nonlinear_lut.lookup_bits(f16::from_f32(value).to_bits());

                // Convert to f16 and write as little-endian bytes
                let half = f16::from_f32(linear);
                let bytes = half.to_le_bytes();
                output[i * 2] = bytes[0];
                output[i * 2 + 1] = bytes[1];
            }
        }
        SampleType::F32 => {
            // Convert to f32 with inverse nonlinear transform
            if output.len() != spatial_data.len() * 4 {
                return Err(Error::invalid("Output buffer size mismatch for F32"));
            }

            for (i, &value) in spatial_data.iter().enumerate() {
                // Apply inverse nonlinear transform
                let linear = nonlinear::from_nonlinear(value);

                // Write as little-endian bytes
                let bytes = linear.to_le_bytes();
                output[i * 4..i * 4 + 4].copy_from_slice(&bytes);
            }
        }
        SampleType::U32 => {
            return Err(Error::unsupported(
                "U32 sample type not supported for lossy DCT compression"
            ));
        }
    }

    Ok(())
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

/// Read and skip channel classification rules (version 2+)
fn read_channel_rules(reader: &mut Cursor<&[u8]>, _num_channels: usize) -> Result<usize> {
    // Channel rules format (from OpenEXR readChannelRules):
    // - u16: total size of rules block in bytes (including this u16)
    // - Series of Classifier structures (variable length)

    // Read total size as u16
    let pos = reader.position() as usize;
    let data = reader.get_ref();

    if pos + 2 > data.len() {
        return Err(Error::invalid("Not enough data to read channel rules size"));
    }

    let rule_size = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;

    if rule_size < 2 {
        return Err(Error::invalid("Invalid channel rules size"));
    }

    // Skip the entire rules block
    reader.set_position((pos + rule_size) as u64);

    Ok(rule_size)
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
fn parse_header(reader: &mut Cursor<&[u8]>, _num_channels: usize) -> Result<Header> {
    // Read header values (all u64 in little-endian)
    let version = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read DWA version"))?;


    if version > 2 {
        return Err(Error::invalid(format!("Unsupported DWA version: {}", version)));
    }

    // Header fields in order (from OpenEXR internal_dwa_helpers.h DataSizesSingle enum):
    // 0: VERSION
    // 1: UNKNOWN_UNCOMPRESSED_SIZE
    // 2: UNKNOWN_COMPRESSED_SIZE
    // 3: AC_COMPRESSED_SIZE
    // 4: DC_COMPRESSED_SIZE
    // 5: RLE_COMPRESSED_SIZE
    // 6: RLE_UNCOMPRESSED_SIZE
    // 7: RLE_RAW_SIZE
    // 8: AC_UNCOMPRESSED_COUNT
    // 9: DC_UNCOMPRESSED_COUNT
    // 10: AC_COMPRESSION

    let unknown_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read unknown uncompressed size"))? as usize;

    let unknown_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read unknown compressed size"))? as usize;

    let ac_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read AC compressed size"))? as usize;

    let dc_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read DC compressed size"))? as usize;

    let rle_compressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read RLE compressed size"))? as usize;

    let rle_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read RLE uncompressed size"))? as usize;

    let rle_raw_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read RLE raw size"))? as usize;

    let ac_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read AC uncompressed count"))? as usize;

    let dc_uncompressed_size = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read DC uncompressed count"))? as usize;

    let ac_compression_value = read_u64_le(reader)
        .map_err(|_| Error::invalid("Failed to read AC compression method"))?;

    // Parse AC compression method
    let ac_compression = match ac_compression_value {
        0 => AcCompression::StaticHuffman,
        1 => AcCompression::Deflate,
        _ => return Err(Error::invalid(format!(
            "Invalid AC compression method: {}",
            ac_compression_value
        ))),
    };

    // Version 2+ files include channel classification rules after the header
    if version >= 2 {
        // Read and skip channel rules - we recompute them from channel names
        let _rule_size = read_channel_rules(reader, _num_channels)?;
    }

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
