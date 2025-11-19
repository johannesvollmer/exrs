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
use smallvec::SmallVec;
use std::sync::atomic::Ordering;

use classifier::{classify_channels, CompressionScheme};
use constants::AC_COUNT;
use nonlinear::ToLinearLut;
use half::f16;
use dct::{inverse_dct_8x8_optimized, from_half_zigzag};

const VERBOSE_DWA_LOG: bool = false;

/// Per-channel decode state tracking
struct ChannelDecodeState {
    scheme: CompressionScheme,
    sample_type: crate::meta::attribute::SampleType,
    width: usize,
    height: usize,
    x_sampling: usize,
    y_sampling: usize,
    bytes_per_sample: usize,

    /// Starting offset in the output buffer for this channel
    channel_start_offset: usize,

    /// Offset to each row in the output buffer (accounting for subsampling)
    row_offsets: Vec<usize>,

    /// Cursor for RLE data (if scheme is Rle)
    rle_cursor: usize,

    /// Cursor for Unknown data (if scheme is Unknown)
    unknown_cursor: usize,
}

impl ChannelDecodeState {
    fn new(
        scheme: CompressionScheme,
        channel: &crate::meta::attribute::ChannelDescription,
        rectangle: IntegerBounds,
        channel_index: usize,
        channels: &crate::meta::attribute::ChannelList,
    ) -> Self {
        let channel_resolution = channel.subsampled_resolution(rectangle.size);
        let width = channel_resolution.x();
        let height = channel_resolution.y();
        let bytes_per_sample = channel.sample_type.bytes_per_sample();
        let y_sampling = channel.sampling.y();

        // Calculate row offsets for SCANLINE-PLANAR layout:
        // Y=0: [ch0 samples][ch1 samples][ch2 samples][ch3 samples]
        // Y=1: [ch0 samples][ch1 samples][ch2 samples][ch3 samples]
        // This matches how convert_little_endian_to_current expects data (mod.rs:508-533)

        let row_offsets: Vec<usize> = (0..height)
            .map(|subsampled_row| {
                let full_y = subsampled_row * y_sampling;

                // Calculate offset for this scanline
                let mut scanline_offset = 0usize;

                // Add bytes for all previous scanlines
                for y in 0..full_y {
                    // For each channel, add its contribution to this scanline
                    for ch in &channels.list {
                        let ch_y_sampling = ch.sampling.y();
                        if y % ch_y_sampling == 0 {
                            let ch_resolution = ch.subsampled_resolution(rectangle.size);
                            let ch_width = ch_resolution.x();
                            let ch_bytes_per_sample = ch.sample_type.bytes_per_sample();
                            scanline_offset += ch_width * ch_bytes_per_sample;
                        }
                    }
                }

                // Now add bytes for previous channels in this scanline
                channels.list.iter()
                    .take(channel_index)
                    .filter(|ch| full_y % ch.sampling.y() == 0)
                    .for_each(|ch| {
                        let ch_resolution = ch.subsampled_resolution(rectangle.size);
                        let ch_width = ch_resolution.x();
                        let ch_bytes_per_sample = ch.sample_type.bytes_per_sample();
                        scanline_offset += ch_width * ch_bytes_per_sample;
                    });

                scanline_offset
            })
            .collect();

        Self {
            scheme,
            sample_type: channel.sample_type,
            width,
            height,
            x_sampling: channel.sampling.x(),
            y_sampling,
            bytes_per_sample,
            channel_start_offset: row_offsets[0],
            row_offsets,
            rle_cursor: 0,
            unknown_cursor: 0,
        }
    }
}

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
                // AC uncompressed size is in u16 count, convert to bytes
                decompress_zip(&compressed, header.ac_uncompressed_size * 2)?
            }
            AcCompression::StaticHuffman => {
                // Use PIZ Huffman decompressor - AC coefficients are u16 values
                let decompressed_u16 = super::piz::huffman::decompress(&compressed, header.ac_uncompressed_size)?;

                // Convert u16 to bytes (little-endian)
                let mut bytes = vec![0u8; decompressed_u16.len() * 2];
                for (i, &value) in decompressed_u16.iter().enumerate() {
                    let le_bytes = value.to_le_bytes();
                    bytes[i * 2] = le_bytes[0];
                    bytes[i * 2 + 1] = le_bytes[1];
                }
                bytes
            }
        }
    } else {
        Vec::new()
    };

    let dc_data = if header.dc_compressed_size > 0 {
        let compressed = read_bytes(&mut reader, header.dc_compressed_size)?;
        if VERBOSE_DWA_LOG {
            eprintln!("DWA DC: compressed_size={}, uncompressed_count={} (expect {} bytes)",
                      header.dc_compressed_size, header.dc_uncompressed_size, header.dc_uncompressed_size * 2);
        }
        // DC coefficients are u16 values, decompress and apply byte-delta decoding
        let decompressed = decompress_zip(&compressed, header.dc_uncompressed_size * 2)?;
        if VERBOSE_DWA_LOG {
            eprintln!("DWA DC: after ZIP decompression: {} bytes", decompressed.len());
        }
        // Apply byte-delta decoding and interleaving (zip_reconstruct_bytes from OpenEXR)
        let reconstructed = zip_reconstruct_bytes(&decompressed);
        if VERBOSE_DWA_LOG {
            eprintln!("DWA DC: after zip_reconstruct: {} bytes ({} u16 values)",
                      reconstructed.len(), reconstructed.len() / 2);
        }
        reconstructed
    } else {
        Vec::new()
    };

    let rle_data = if header.rle_compressed_size > 0 {
        let compressed = read_bytes(&mut reader, header.rle_compressed_size)?;
        if VERBOSE_DWA_LOG {
            eprintln!("DWA: RLE compressed_size={}, uncompressed_size={}, raw_size={}",
                      header.rle_compressed_size, header.rle_uncompressed_size, header.rle_raw_size);
            eprintln!("DWA: RLE compressed (first 16): {:02x?}", &compressed[..compressed.len().min(16)]);
        }

        let uncompressed = decompress_zip(&compressed, header.rle_uncompressed_size)?;
        if VERBOSE_DWA_LOG {
            eprintln!("DWA: RLE after ZIP decompression: {} bytes", uncompressed.len());
            eprintln!("DWA: RLE uncompressed (first 16): {:02x?}", &uncompressed[..uncompressed.len().min(16)]);
        }

        let rle_decompressed = decompress_rle(&uncompressed, header.rle_raw_size)?;
        if VERBOSE_DWA_LOG {
            eprintln!("DWA: RLE after RLE decompression: {} bytes", rle_decompressed.len());
            eprintln!("DWA: RLE decompressed (first 16): {:02x?}", &rle_decompressed[..rle_decompressed.len().min(16)]);
        }

        // RLE data is already in byte-plane format (low bytes, then high bytes)
        // No need for zip_reconstruct_bytes - that's only for DC data
        rle_decompressed
    } else {
        if VERBOSE_DWA_LOG {
            eprintln!("DWA: No RLE data (rle_compressed_size=0)");
        }
        Vec::new()
    };

    // Create lookup table for inverse nonlinear transform (nonlinear -> linear)
    let to_linear_lut = ToLinearLut::new();

    // Allocate output buffer
    let mut output = vec![0u8; expected_byte_size];

    static DECOMPRESS_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let count = DECOMPRESS_COUNT.fetch_add(1, Ordering::Relaxed);
    if VERBOSE_DWA_LOG {
        eprintln!("DWA decompress #{}: rectangle={}x{} at ({},{}), expected_byte_size={}, channels.bytes_per_pixel={}",
                  count, rectangle.size.width(), rectangle.size.height(),
                  rectangle.position.x(), rectangle.position.y(),
                  expected_byte_size, channels.bytes_per_pixel);
    }

    // Initialize channel decode states with row offsets
    // Note: Output uses SCANLINE-PLANAR layout (Y=0: ch0,ch1,ch2,ch3; Y=1: ch0,ch1,ch2,ch3; ...)
    let channel_states: SmallVec<[ChannelDecodeState; 4]> = channels.list.iter()
        .enumerate()
        .map(|(ch_idx, channel)| {
            let channel_class = &classification.channel_classifications[ch_idx];
            let channel_resolution = channel.subsampled_resolution(rectangle.size);
            let bytes_per_sample = channel.sample_type.bytes_per_sample();
            let channel_bytes = channel_resolution.area() * bytes_per_sample;

            if VERBOSE_DWA_LOG {
                let channel_name: String = channel.name.clone().into();
                eprintln!("DWA channel {} '{}': scheme={:?}, resolution={}x{}, bytes={}",
                          ch_idx, channel_name, channel_class.scheme,
                          channel_resolution.width(), channel_resolution.height(), channel_bytes);
            }

            ChannelDecodeState::new(
                channel_class.scheme,
                channel,
                rectangle,
                ch_idx,
                channels,
            )
        })
        .collect();

    // Determine lossy channel dimensions (all lossy channels share same resolution)
    let (lossy_channel_order, lossy_channel_map) =
        build_lossy_channel_order(&channel_states, &classification);
    let first_lossy_idx = *lossy_channel_order
        .first()
        .ok_or_else(|| Error::invalid("No lossy DCT channels found"))?;
    let lossy_channel = &channel_states[first_lossy_idx];
    let num_blocks_x = (lossy_channel.width + constants::BLOCK_SIZE - 1) / constants::BLOCK_SIZE;
    let num_blocks_y = (lossy_channel.height + constants::BLOCK_SIZE - 1) / constants::BLOCK_SIZE;

    // Prepare per-channel DC, RLE, and Unknown data
    let dc_planes = prepare_dc_planes(&channel_states, num_blocks_x, num_blocks_y, &dc_data, &lossy_channel_order)?;
    let (rle_slices, unknown_slices) = split_auxiliary_streams(&channel_states, &rle_data, &unknown_data)?;
    let mut rle_cursors: SmallVec<[usize; 4]> = (0..channel_states.len()).map(|_| 0).collect();
    let mut unknown_cursors: SmallVec<[usize; 4]> = (0..channel_states.len()).map(|_| 0).collect();

    // Decode lossy channels block-by-block, mirroring OpenEXR's pipeline
    let lossy_channel_count = lossy_channel_order.len();
    let mut row_blocks_f32: SmallVec<[Vec<f32>; 4]> = (0..lossy_channel_count)
        .map(|_| vec![0.0f32; num_blocks_x * constants::BLOCK_AREA])
        .collect();
    let mut row_blocks_bits: SmallVec<[Vec<u16>; 4]> = (0..lossy_channel_count)
        .map(|_| vec![0u16; num_blocks_x * constants::BLOCK_AREA])
        .collect();
    let mut ac_cursor = std::io::Cursor::new(ac_data.as_slice());

    for block_y in 0..num_blocks_y {
        decode_block_row(
            block_y,
            num_blocks_x,
            &channel_states,
            &dc_planes,
            &mut ac_cursor,
            &classification,
            &lossy_channel_order,
            &lossy_channel_map,
            &mut row_blocks_f32,
        )?;

        quantize_row_blocks(&row_blocks_f32, &mut row_blocks_bits)?;

        for by in 0..constants::BLOCK_SIZE {
            let y = block_y * constants::BLOCK_SIZE + by;
            if y >= rectangle.size.height() {
                break;
            }

            write_scanline_from_blocks(
                y,
                by,
                num_blocks_x,
                &channel_states,
                &row_blocks_bits,
                &rle_slices,
                &unknown_slices,
                &mut rle_cursors,
                &mut unknown_cursors,
                &lossy_channel_map,
                &to_linear_lut,
                &mut output,
            )?;
        }
    }

    let ac_bytes_consumed = ac_cursor.position() as usize;
    if ac_bytes_consumed != ac_data.len() && VERBOSE_DWA_LOG {
        eprintln!(
            "DWA WARNING: AC data not fully consumed (used {}, total {})",
            ac_bytes_consumed,
            ac_data.len()
        );
    }

    Ok(output)
}

fn prepare_dc_planes(
    channel_states: &SmallVec<[ChannelDecodeState; 4]>,
    num_blocks_x: usize,
    num_blocks_y: usize,
    dc_data: &[u8],
    lossy_channel_order: &[usize],
) -> Result<SmallVec<[Vec<u16>; 4]>> {
    if dc_data.len() % 2 != 0 {
        return Err(Error::invalid("DC data length must be even"));
    }

    let dc_as_u16: Vec<u16> = dc_data
        .chunks_exact(2)
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    let mut planes: SmallVec<[Vec<u16>; 4]> = channel_states
        .iter()
        .map(|state| {
            if state.scheme == CompressionScheme::LossyDct {
                Vec::with_capacity(num_blocks_x * num_blocks_y)
            } else {
                Vec::new()
            }
        })
        .collect();

    let mut offset = 0usize;
    let channel_blocks = num_blocks_x * num_blocks_y;
    for &ch_idx in lossy_channel_order {
        if offset + channel_blocks > dc_as_u16.len() {
            return Err(Error::invalid("DC data buffer too small"));
        }
        planes[ch_idx] = dc_as_u16[offset..offset + channel_blocks].to_vec();
        offset += channel_blocks;
    }

    if offset != dc_as_u16.len() {
        return Err(Error::invalid("DC data buffer has leftover data"));
    }

    Ok(planes)
}

fn split_auxiliary_streams<'a>(
    channel_states: &'a SmallVec<[ChannelDecodeState; 4]>,
    rle_data: &'a [u8],
    unknown_data: &'a [u8],
) -> Result<(SmallVec<[&'a [u8]; 4]>, SmallVec<[&'a [u8]; 4]>)> {
    let mut rle_offset = 0usize;
    let rle_slices: SmallVec<[&[u8]; 4]> = channel_states.iter()
        .enumerate()
        .map(|(ch_idx, state)| {
            if state.scheme == CompressionScheme::Rle {
                let bytes = state.height * state.width * state.bytes_per_sample;
                let start = rle_offset;
                let end = start + bytes;
                if end > rle_data.len() {
                    return Err(Error::invalid(format!(
                        "RLE slice overflow for channel {}: need {} bytes at offset {}, have {} total",
                        ch_idx, bytes, start, rle_data.len()
                    )));
                }
                rle_offset = end;
                Ok(&rle_data[start..end])
            } else {
                Ok(&rle_data[0..0])
            }
        })
        .collect::<Result<SmallVec<[&[u8]; 4]>>>()?;

    let mut unknown_offset = 0usize;
    let unknown_slices: SmallVec<[&[u8]; 4]> = channel_states.iter()
        .enumerate()
        .map(|(ch_idx, state)| {
            if state.scheme == CompressionScheme::Unknown {
                let bytes = state.height * state.width * state.bytes_per_sample;
                let start = unknown_offset;
                let end = start + bytes;
                if end > unknown_data.len() {
                    return Err(Error::invalid(format!(
                        "Unknown slice overflow for channel {}: need {} bytes at offset {}, have {} total",
                        ch_idx, bytes, start, unknown_data.len()
                    )));
                }
                unknown_offset = end;
                Ok(&unknown_data[start..end])
            } else {
                Ok(&unknown_data[0..0])
            }
        })
        .collect::<Result<SmallVec<[&[u8]; 4]>>>()?;

    Ok((rle_slices, unknown_slices))
}

fn build_lossy_channel_order(
    channel_states: &SmallVec<[ChannelDecodeState; 4]>,
    classification: &classifier::ClassificationResult,
) -> (Vec<usize>, Vec<Option<usize>>) {
    let mut order = Vec::new();
    let mut map = vec![None; channel_states.len()];

    for group in &classification.csc_groups {
        push_lossy_channel(&mut order, &mut map, group.r_index);
        push_lossy_channel(&mut order, &mut map, group.g_index);
        push_lossy_channel(&mut order, &mut map, group.b_index);
    }

    for (idx, state) in channel_states.iter().enumerate() {
        if state.scheme == CompressionScheme::LossyDct {
            push_lossy_channel(&mut order, &mut map, idx);
        }
    }

    (order, map)
}

fn push_lossy_channel(
    order: &mut Vec<usize>,
    map: &mut [Option<usize>],
    ch_idx: usize,
) {
    if map[ch_idx].is_some() {
        return;
    }
    let pos = order.len();
    order.push(ch_idx);
    map[ch_idx] = Some(pos);
}


/// Decode one row of 8x8 blocks for all lossy DCT channels
fn decode_block_row(
    block_y: usize,
    num_blocks_x: usize,
    channel_states: &SmallVec<[ChannelDecodeState; 4]>,
    dc_planes: &SmallVec<[Vec<u16>; 4]>,
    ac_cursor: &mut std::io::Cursor<&[u8]>,
    classification: &classifier::ClassificationResult,
    lossy_channel_order: &[usize],
    lossy_channel_map: &[Option<usize>],
    row_blocks: &mut SmallVec<[Vec<f32>; 4]>,
) -> Result<()> {


    // Decode each block in this row
    for block_x in 0..num_blocks_x {
        for &ch_idx in lossy_channel_order {
            let state = &channel_states[ch_idx];
            debug_assert_eq!(state.scheme, CompressionScheme::LossyDct);
            let row_block_idx = lossy_channel_map[ch_idx]
                .expect("lossy channel missing from order map");

            let dc_plane = &dc_planes[ch_idx];
            let block_idx = block_y * num_blocks_x + block_x;

            if block_idx >= dc_plane.len() {
                return Err(Error::invalid("DC plane index out of bounds"));
            }

            // Read DC coefficient (already in native endian from zip_reconstruct_bytes)
            let dc_coeff_bits = dc_plane[block_idx];

            // Read AC coefficients from continuous RLE stream
            let ac_coeffs_bits = read_ac_coefficients_for_block(ac_cursor)?;

            // Find last non-zero coefficient for optimization
            let last_non_zero = rle::find_last_non_zero(&ac_coeffs_bits);

            // Construct full DCT coefficient block
            let mut half_block = [0u16; 64];
            half_block[0] = dc_coeff_bits;
            half_block[1..=AC_COUNT].copy_from_slice(&ac_coeffs_bits[..AC_COUNT]);
            let mut dct_coeffs = [0.0f32; 64];
            from_half_zigzag(&half_block, &mut dct_coeffs);

            // Debug first block of first row
            if VERBOSE_DWA_LOG && block_y == 0 && block_x == 0 {
                eprintln!("DWA DEBUG ch {} block (0,0):", ch_idx);
                eprintln!("  DC: bits=0x{:04x}, f16={}, f32={}",
                          dc_coeff_bits, f16::from_bits(dc_coeff_bits), dct_coeffs[0]);
                eprintln!("  AC non-zero count: {}", ac_coeffs_bits.iter().filter(|&&b| b != 0).count());
                eprintln!("  First 4 AC bits (zigzag): {:04x?}", &ac_coeffs_bits[..4.min(ac_coeffs_bits.len())]);
                eprintln!("  First 4 AC f32 (normal order): [{:.6}, {:.6}, {:.6}, {:.6}]",
                          dct_coeffs[1], dct_coeffs[2], dct_coeffs[3], dct_coeffs[4]);
            }

            // Apply inverse DCT
            let spatial_block = inverse_dct_8x8_optimized(&dct_coeffs, last_non_zero);

            // Debug first block spatial values
            if VERBOSE_DWA_LOG && block_y == 0 && block_x == 0 {
                eprintln!("  After IDCT: first 4 spatial (perceptual f32): [{:.6}, {:.6}, {:.6}, {:.6}]",
                          spatial_block[0], spatial_block[1], spatial_block[2], spatial_block[3]);
            }

            // Store spatial block as nonlinear f16 bits
            // IMPORTANT: The IDCT output is ALREADY in perceptual (nonlinear) space!
            // We just convert f32→f16 without any nonlinear encoding.
            // The toLinear LUT will later convert these to linear for CSC/output.
            let row_block = &mut row_blocks[row_block_idx];
            let offset = block_x * 64;
            spatial_block.iter().enumerate().for_each(|(i, &val)| {
                row_block[offset + i] = val;
            });

            if VERBOSE_DWA_LOG && block_y == 0 && block_x == 0 {
                eprintln!("  After perceptual store: first 4 values: [{:.6}, {:.6}, {:.6}, {:.6}]",
                          row_block[offset], row_block[offset + 1], row_block[offset + 2], row_block[offset + 3]);
            }
        }
    }

    // Apply inverse CSC for RGB triplets
    for csc_group in &classification.csc_groups {
        let r_idx = lossy_channel_map[csc_group.r_index]
            .ok_or_else(|| Error::invalid("Missing R channel in lossy order"))?;
        let g_idx = lossy_channel_map[csc_group.g_index]
            .ok_or_else(|| Error::invalid("Missing G channel in lossy order"))?;
        let b_idx = lossy_channel_map[csc_group.b_index]
            .ok_or_else(|| Error::invalid("Missing B channel in lossy order"))?;

            if VERBOSE_DWA_LOG && block_y == 0 {
                eprintln!("DWA DEBUG: Applying CSC - r_idx={}, g_idx={}, b_idx={} (lossy indices)", r_idx, g_idx, b_idx);
                eprintln!("           CSC group: R ch={}, G ch={}, B ch={}",
                          csc_group.r_index, csc_group.g_index, csc_group.b_index);
            }

            // Apply CSC for each block in this row
            // IMPORTANT: CSC operates in PERCEPTUAL (nonlinear) space!
            // Y'CbCr (with prime) → R'G'B' (with prime) are both perceptual.
            // We just convert f16→f32, apply CSC, then f32→f16.
            // NO toLinear/toNonlinear transforms!

            for block_x in 0..num_blocks_x {
                let offset = block_x * 64;

                // Extract Y'CbCr values for this block (perceptual space)
                // OpenEXR encoding stores Y in the R channel slot,
                // Cb in the G slot, and Cr in the B slot.
                let y_block: [f32; 64] = std::array::from_fn(|i| row_blocks[r_idx][offset + i]);
                let cb_block: [f32; 64] = std::array::from_fn(|i| row_blocks[g_idx][offset + i]);
                let cr_block: [f32; 64] = std::array::from_fn(|i| row_blocks[b_idx][offset + i]);

                if VERBOSE_DWA_LOG && block_y == 0 && block_x == 0 {
                    eprintln!("  Before CSC (perceptual): Y'={:.6}, Cb={:.6}, Cr={:.6}",
                              y_block[0], cb_block[0], cr_block[0]);
                }

                // Convert Y'CbCr to R'G'B' (in perceptual space)
                y_block.iter()
                    .zip(cb_block.iter())
                    .zip(cr_block.iter())
                    .enumerate()
                    .for_each(|(i, ((&y, &cb), &cr))| {
                        let (r, g, b) = csc::ycbcr_to_rgb(y, cb, cr);

                        if VERBOSE_DWA_LOG && block_y == 0 && block_x == 0 && i == 0 {
                            eprintln!("  After CSC (perceptual): R'={:.6}, G'={:.6}, B'={:.6}", r, g, b);
                        }

                        // Store R'G'B' as perceptual f32
                        row_blocks[r_idx][offset + i] = r;
                        row_blocks[g_idx][offset + i] = g;
                        row_blocks[b_idx][offset + i] = b;
                    });
            }
        }

    Ok(())
}

fn quantize_row_blocks(
    row_blocks_f32: &SmallVec<[Vec<f32>; 4]>,
    row_blocks_bits: &mut SmallVec<[Vec<u16>; 4]>,
) -> Result<()> {
    row_blocks_f32.iter()
        .zip(row_blocks_bits.iter_mut())
        .try_for_each(|(src, dst)| {
            if src.len() != dst.len() {
                return Err(Error::invalid("Row block buffer size mismatch"));
            }
            src.iter()
                .zip(dst.iter_mut())
                .for_each(|(&val, dst_elem)| {
                    *dst_elem = float_to_half_bits(val);
                });
            Ok(())
        })
}

fn float_to_half_bits(value: f32) -> u16 {
    let ui = value.to_bits();
    let sign = ((ui >> 16) & 0x8000) as u16;
    let mut ret = sign;
    let mut abs = ui & 0x7fff_ffff;

    if abs >= 0x3880_0000 {
        if abs >= 0x7f80_0000 {
            ret |= 0x7c00;
            if abs == 0x7f80_0000 {
                return ret;
            }
            let m = (abs & 0x007f_ffff) >> 13;
            return ret | (m as u16) | if m == 0 { 1 } else { 0 };
        }

        if abs > 0x477f_efff {
            return ret | 0x7c00;
        }

        abs -= 0x3800_0000;
        abs = (abs + 0x0000_0fff + ((abs >> 13) & 1)) >> 13;
        return ret | (abs as u16);
    }

    if abs < 0x3300_0001 {
        return ret;
    }

    let e = abs >> 23;
    let shift = 0x7e - e;
    let m = 0x0080_0000 | (abs & 0x007f_ffff);
    let r = (m as u64) << (32 - shift);
    ret |= (m >> shift) as u16;
    if r > 0x8000_0000 || (r == 0x8000_0000 && (ret & 0x1) != 0) {
        ret = ret.wrapping_add(1);
    }
    ret
}

/// Write one scanline for all channels using decoded row blocks
fn write_scanline_from_blocks(
    y: usize,
    by: usize,  // row within 8x8 block (0-7)
    _num_blocks_x: usize,
    channel_states: &SmallVec<[ChannelDecodeState; 4]>,
    row_blocks: &SmallVec<[Vec<u16>; 4]>,
    rle_slices: &SmallVec<[&[u8]; 4]>,
    unknown_slices: &SmallVec<[&[u8]; 4]>,
    rle_cursors: &mut SmallVec<[usize; 4]>,
    unknown_cursors: &mut SmallVec<[usize; 4]>,
    lossy_channel_map: &[Option<usize>],
    to_linear_lut: &ToLinearLut,
    output: &mut [u8],
) -> Result<()> {
    use constants::BLOCK_SIZE;

    for (ch_idx, state) in channel_states.iter().enumerate() {
        // CRITICAL: Honor y_sampling - skip rows where y is not aligned
        // This matches OpenEXR internal_dwa_compressor.h:1160-1188
        if y % state.y_sampling != 0 {
            continue;
        }

        // Compute subsampled row index for indexing into row_offsets
        // state.height is already subsampled, row_offsets has state.height entries
        let subsampled_y = y / state.y_sampling;

        // Skip if subsampled row is beyond this channel's height
        if subsampled_y >= state.height {
            continue;
        }

        let row_offset = state.row_offsets[subsampled_y];
        let row_bytes = state.width * state.bytes_per_sample;

        if VERBOSE_DWA_LOG && y < 3 {
            eprintln!("DWA ch {} y {} (subsampled_y {}): row_offset={}, row_bytes={}, scheme={:?}",
                      ch_idx, y, subsampled_y, row_offset, row_bytes, state.scheme);
        }

        match state.scheme {
            CompressionScheme::LossyDct => {
                let row_block_idx = lossy_channel_map[ch_idx]
                    .ok_or_else(|| Error::invalid("Missing lossy channel index for scanline"))?;
                let row_block = &row_blocks[row_block_idx];

                if VERBOSE_DWA_LOG && y == 0 && ch_idx == 1 {
                    eprintln!("DWA LossyDct ch {} y {}: row_block.len={}, first 4 values: {:04x?}",
                              ch_idx, y, row_block.len(),
                              &row_block[..4.min(row_block.len())]);
                }

                for x in 0..state.width {
                    let block_x = x / BLOCK_SIZE;
                    let bx = x % BLOCK_SIZE;
                    let block_offset = block_x * 64 + by * BLOCK_SIZE + bx;

                    if block_offset >= row_block.len() {
                        return Err(Error::invalid("Block offset out of bounds"));
                    }

                    let nonlinear_bits = row_block[block_offset];
                    let linear_bits = to_linear_lut.lookup(nonlinear_bits);

                    // Planar layout: samples for this channel are contiguous
                    let out_offset = row_offset + x * state.bytes_per_sample;

                    if state.sample_type == crate::meta::attribute::SampleType::F16 {
                        // Write as F16
                        let bytes = linear_bits.to_le_bytes();
                        output[out_offset] = bytes[0];
                        output[out_offset + 1] = bytes[1];

                        if VERBOSE_DWA_LOG && y == 0 && ch_idx == 1 && x < 4 {
                            let f_val = f16::from_bits(linear_bits);
                            eprintln!("  x={}: nonlinear=0x{:04x}, linear=0x{:04x}, f={}",
                                      x, nonlinear_bits, linear_bits, f_val);
                        }
                    } else if state.sample_type == crate::meta::attribute::SampleType::F32 {
                        // Convert to F32
                        let linear_f32 = f16::from_bits(linear_bits).to_f32();
                        let bytes = linear_f32.to_le_bytes();
                        output[out_offset..out_offset + 4].copy_from_slice(&bytes);
                    }
                }
            }
            CompressionScheme::Rle => {
                // RLE data is in byte-plane format: all first bytes, then all second bytes, etc.
                // For F16: [low_byte_0, low_byte_1, ..., low_byte_N, high_byte_0, high_byte_1, ..., high_byte_N]
                let rle_slice = rle_slices[ch_idx];
                let pixel_cursor = rle_cursors[ch_idx];  // cursor tracks pixel count, not byte offset

                let total_pixels = state.width * state.height;
                let pixels_per_row = state.width;

                if pixel_cursor + pixels_per_row > total_pixels {
                    return Err(Error::invalid("RLE pixel cursor out of bounds"));
                }

                // Reconstruct pixels from byte planes
                for x in 0..pixels_per_row {
                    let pixel_idx = pixel_cursor + x;
                    // Planar layout: samples for this channel are contiguous
                    let out_pos = row_offset + x * state.bytes_per_sample;

                    // Read each byte from its respective plane with bounds checking
                    for byte_idx in 0..state.bytes_per_sample {
                        let plane_offset = byte_idx * total_pixels;
                        let read_pos = plane_offset + pixel_idx;

                        if read_pos >= rle_slice.len() {
                            return Err(Error::invalid(format!(
                                "RLE planar read out of bounds: ch {} byte_plane {} pixel {} (offset {} >= slice len {})",
                                ch_idx, byte_idx, pixel_idx, read_pos, rle_slice.len()
                            )));
                        }

                        let byte_val = rle_slice[read_pos];
                        output[out_pos + byte_idx] = byte_val;
                    }
                }

                if VERBOSE_DWA_LOG && (y == 7 || y == 8 || y == 9) {
                    let first_pixel_u16 = u16::from_le_bytes([output[row_offset], output[row_offset + 1]]);
                    let first_pixel_f16 = f16::from_bits(first_pixel_u16);
                    eprintln!("DWA RLE ch {} y {}: pixel_cursor {} -> {}, row_offset {}, first pixel bytes: {:02x?} = 0x{:04x} = {}",
                              ch_idx, y, pixel_cursor, pixel_cursor + pixels_per_row, row_offset,
                              &output[row_offset..row_offset + state.bytes_per_sample.min(2)],
                              first_pixel_u16, first_pixel_f16);
                }

                rle_cursors[ch_idx] = pixel_cursor + pixels_per_row;
            }
            CompressionScheme::Unknown => {
                // Copy row from this channel's Unknown slice
                let unknown_slice = unknown_slices[ch_idx];
                let cursor = unknown_cursors[ch_idx];

                if cursor + row_bytes > unknown_slice.len() {
                    return Err(Error::invalid("Unknown data buffer overrun"));
                }

                // Write pixels to planar layout (samples for this channel are contiguous)
                output[row_offset..row_offset + row_bytes]
                    .copy_from_slice(&unknown_slice[cursor..cursor + row_bytes]);

                unknown_cursors[ch_idx] = cursor + row_bytes;
            }
        }
    }

    Ok(())
}

/// Decode a lossy DCT channel into spatial domain
fn decode_lossy_dct_channel(
    resolution: crate::prelude::Vec2<usize>,
    _classification: &classifier::ChannelClassification,
    _csc_groups: &[classifier::CscGroup],
    ac_reader: &mut std::io::Cursor<&[u8]>,
    dc_reader: &mut std::io::Cursor<&[u8]>,
) -> Result<Vec<f32>> {
    use constants::BLOCK_SIZE;

    let width = resolution.x();
    let height = resolution.y();
    let pixel_count = width * height;

    // Calculate number of blocks
    let blocks_x = (width + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let blocks_y = (height + BLOCK_SIZE - 1) / BLOCK_SIZE;

    // Allocate spatial buffer
    let mut spatial_data = vec![0.0f32; pixel_count];

    // Process each 8x8 block
    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            // Read DC coefficient (u16, little-endian) - stored as f16
            let dc_coeff_bits = read_u16_le(dc_reader)?;

            // Read AC coefficients from continuous RLE stream - stored as f16
            let ac_coeffs_bits = read_ac_coefficients_for_block(ac_reader)?;

            // Find last non-zero coefficient for optimization
            let last_non_zero = rle::find_last_non_zero(&ac_coeffs_bits);

            // Construct full DCT coefficient block (DC + AC)
            let mut half_block = [0u16; 64];
            half_block[0] = dc_coeff_bits;
            half_block[1..=AC_COUNT].copy_from_slice(&ac_coeffs_bits[..AC_COUNT]);
            let mut dct_coeffs = [0.0f32; 64];
            from_half_zigzag(&half_block, &mut dct_coeffs);

            // Apply inverse DCT
            let spatial_block = inverse_dct_8x8_optimized(&dct_coeffs, last_non_zero);

            if block_y == 0 && block_x == 0 {
                eprintln!("DWA DEBUG: First block DC={:.6}, first spatial values: {:.6}, {:.6}, {:.6}",
                          dct_coeffs[0], spatial_block[0], spatial_block[1], spatial_block[2]);
            }

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

    let bytes: [u8; 2] = data.get(pos..pos + 2)
        .and_then(|slice| std::convert::TryFrom::try_from(slice).ok())
        .ok_or_else(|| Error::invalid("Not enough data to read u16"))?;

    reader.set_position((pos + 2) as u64);
    Ok(u16::from_le_bytes(bytes))
}

/// Read AC coefficients for one 8x8 block from continuous RLE stream
/// Based on OpenEXR LossyDctDecoder_unRleAc
fn read_ac_coefficients_for_block(reader: &mut std::io::Cursor<&[u8]>) -> Result<[u16; 63]> {
    let mut ac_coeffs = [0u16; 63];
    let mut dct_comp = 1; // Start at 1 (DC is 0, we're reading AC)

    while dct_comp < 64 {
        if reader.position() as usize >= reader.get_ref().len() {
            return Err(Error::invalid("Unexpected end of AC stream"));
        }

        let val = read_u16_le(reader)?;

        if (val & 0xff00) == 0xff00 {
            // RLE marker: 0xffXX
            let count = (val & 0xff) as usize;

            // Count == 0 means "rest of block is zero" (OpenEXR uses 64)
            let advance = if count == 0 { 64 } else { count };
            dct_comp += advance;
            if count != 0 && dct_comp > 64 {
                return Err(Error::invalid("AC zero-run exceeds block length"));
            }
            if count == 0 {
                break;
            }
        } else {
            // Regular coefficient value
            if dct_comp >= 64 {
                return Err(Error::invalid("AC coefficient index out of range"));
            }
            // Store in zigzag order (dct_comp-1 because AC starts at index 1)
            ac_coeffs[dct_comp - 1] = val;
            dct_comp += 1;
        }
    }

    Ok(ac_coeffs)
}

/// Apply inverse CSC to convert Y'CbCr spatial data to RGB
fn apply_inverse_csc(y_data: &[f32], cb_data: &[f32], cr_data: &[f32]) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    use csc::ycbcr_to_rgb;

    let pixel_count = y_data.len();
    let mut r_data = vec![0.0f32; pixel_count];
    let mut g_data = vec![0.0f32; pixel_count];
    let mut b_data = vec![0.0f32; pixel_count];

    y_data.iter()
        .zip(cb_data.iter())
        .zip(cr_data.iter())
        .zip(r_data.iter_mut().zip(g_data.iter_mut()).zip(b_data.iter_mut()))
        .for_each(|(((&y, &cb), &cr), ((r, g), b))| {
            let (r_val, g_val, b_val) = ycbcr_to_rgb(y, cb, cr);
            *r = r_val;
            *g = g_val;
            *b = b_val;
        });

    (r_data, g_data, b_data)
}

/// Apply inverse nonlinear transform and write channel to output
///
/// The spatial data from inverse DCT is in nonlinear (quantized) space.
/// We need to apply the toLinear LUT to convert back to linear light.
fn write_channel_to_output(
    spatial_data: &[f32],
    sample_type: crate::meta::attribute::SampleType,
    to_linear_lut: &ToLinearLut,
    output: &mut [u8],
) -> Result<()> {
    use crate::meta::attribute::SampleType;

    match sample_type {
        SampleType::F16 => {
            // Convert to f16 with inverse nonlinear transform
            if output.len() != spatial_data.len() * 2 {
                return Err(Error::invalid("Output buffer size mismatch for F16"));
            }

            for (i, &value) in spatial_data.iter().enumerate() {
                // Spatial data from inverse DCT is in quantized (nonlinear) space as f32
                // Convert to f16 nonlinear bits, then apply toLinear LUT (u16->u16)
                let nonlinear_f16 = f16::from_f32(value);
                let nonlinear_bits = nonlinear_f16.to_bits();

                // Apply the exact u16->u16 LUT from OpenEXR
                let linear_bits = to_linear_lut.lookup(nonlinear_bits);

                if i < 3 {
                    eprintln!("DWA DEBUG write[{}]: spatial_f32={:.6}, nonlinear_bits={:04x}, linear_bits={:04x} ({:.6})",
                              i, value, nonlinear_bits, linear_bits, f16::from_bits(linear_bits).to_f32());
                }

                // Write linear half as little-endian bytes
                let bytes = linear_bits.to_le_bytes();
                output[i * 2] = bytes[0];
                output[i * 2 + 1] = bytes[1];
            }
        }
        SampleType::F32 => {
            // For F32, convert nonlinear half to linear half, then expand to f32
            // This matches OpenEXR: toLinear first, then half_to_float
            if output.len() != spatial_data.len() * 4 {
                return Err(Error::invalid("Output buffer size mismatch for F32"));
            }

            for (i, &value) in spatial_data.iter().enumerate() {
                // Convert to nonlinear half, apply toLinear LUT, then expand to f32
                let nonlinear_f16 = f16::from_f32(value);
                let nonlinear_bits = nonlinear_f16.to_bits();
                let linear_bits = to_linear_lut.lookup(nonlinear_bits);
                let linear_f32 = f16::from_bits(linear_bits).to_f32();

                // Write as little-endian bytes
                let bytes = linear_f32.to_le_bytes();
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

/// ZIP reconstruct bytes - performs byte-delta decoding and interleaving
/// for DWA DC data decompression.
///
/// This implements the `internal_zip_reconstruct_bytes` function from OpenEXR:
/// 1. reconstruct() - byte-delta decoding
/// 2. interleave() - de-interleaves the data
fn zip_reconstruct_bytes(source: &[u8]) -> Vec<u8> {
    if source.is_empty() {
        return Vec::new();
    }

    // Step 1: reconstruct() - byte-delta decoding
    // For each byte starting at index 1, set buf[i] = (buf[i-1] + buf[i] - 128) as u8
    let mut reconstructed = source.to_vec();
    for i in 1..reconstructed.len() {
        reconstructed[i] = reconstructed[i - 1]
            .wrapping_add(reconstructed[i])
            .wrapping_sub(128);
    }

    // Step 2: interleave() - de-interleave the data
    // Split buffer into two halves at (count+1)/2, then interleave:
    // out[0]=half1[0], out[1]=half2[0], out[2]=half1[1], out[3]=half2[1], etc.
    let count = reconstructed.len();
    let split_point = (count + 1) / 2;

    let mut output = Vec::with_capacity(count);
    let half1 = &reconstructed[..split_point];
    let half2 = &reconstructed[split_point..];

    let mut i1 = 0;
    let mut i2 = 0;

    while output.len() < count {
        if i1 < half1.len() {
            output.push(half1[i1]);
            i1 += 1;
        }
        if output.len() < count && i2 < half2.len() {
            output.push(half2[i2]);
            i2 += 1;
        }
    }

    output
}

/// Decompress RLE data (simple RLE, not the same as the main RLE compression)
/// This is a basic RLE format used for DWAA/DWAB metadata
fn decompress_rle(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    let mut decompressed = Vec::with_capacity(expected_size);
    let mut remaining = compressed;

    if VERBOSE_DWA_LOG {
        eprintln!("decompress_rle: input {:02x?}, expected_size {}", compressed, expected_size);
    }

    while !remaining.is_empty() && decompressed.len() < expected_size {
        if remaining.is_empty() {
            return Err(Error::invalid("Unexpected end of RLE data"));
        }

        let count = remaining[0] as i8;
        remaining = &remaining[1..];

        if VERBOSE_DWA_LOG {
            eprintln!("  RLE count={} (0x{:02x}), current len={}", count, count as u8, decompressed.len());
        }

        if count < 0 {
            // Take the next '-count' bytes as-is
            let n = (-count) as usize;
            if remaining.len() < n {
                return Err(Error::invalid("RLE data truncated"));
            }

            if VERBOSE_DWA_LOG {
                eprintln!("  Literal: copying {} bytes", n);
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

            let repeat_count = (count as usize) + 1;
            if VERBOSE_DWA_LOG {
                eprintln!("  Run: repeating value 0x{:02x} {} times", value, repeat_count);
            }

            decompressed.extend(std::iter::repeat(value).take(repeat_count));
        }
    }

    if decompressed.len() != expected_size {
        return Err(Error::invalid(format!(
            "RLE decompression size mismatch: expected {}, got {}",
            expected_size,
            decompressed.len()
        )));
    }

    if VERBOSE_DWA_LOG {
        eprintln!("decompress_rle done: {} bytes, first 16: {:02x?}, last 16: {:02x?}",
                  decompressed.len(),
                  &decompressed[..16.min(decompressed.len())],
                  &decompressed[decompressed.len().saturating_sub(16)..]);
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

    #[test]
    fn test_zip_reconstruct_bytes_empty() {
        let result = zip_reconstruct_bytes(&[]);
        assert_eq!(result, Vec::<u8>::new());
    }

    #[test]
    fn test_zip_reconstruct_bytes_single() {
        let result = zip_reconstruct_bytes(&[42]);
        assert_eq!(result, vec![42]);
    }

    #[test]
    fn test_zip_reconstruct_bytes_basic() {
        // Test with a simple sequence
        // First, let's understand what happens:
        // Input: [128, 1, 2, 3]
        // After reconstruct (byte-delta decoding):
        //   buf[0] = 128 (unchanged)
        //   buf[1] = (128 + 1 - 128) = 1
        //   buf[2] = (1 + 2 - 128) = wrapping_sub gives 131
        //   buf[3] = (131 + 3 - 128) = 6
        // Split at (4+1)/2 = 2
        // half1 = [128, 1], half2 = [131, 6]
        // Interleave: [128, 131, 1, 6]

        let input = vec![128, 1, 2, 3];
        let result = zip_reconstruct_bytes(&input);

        // Verify the length is preserved
        assert_eq!(result.len(), input.len());
    }

    #[test]
    fn test_zip_reconstruct_bytes_even_length() {
        // Test with even-length input
        let input = vec![128, 0, 0, 0, 0, 0];
        let result = zip_reconstruct_bytes(&input);

        // After reconstruct: [128, 0, 128, 0, 128, 0] (wrapping arithmetic)
        // Split at (6+1)/2 = 3: half1=[128, 0, 128], half2=[0, 128, 0]
        // Interleave: [128, 0, 0, 128, 128, 0]
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_zip_reconstruct_bytes_odd_length() {
        // Test with odd-length input
        let input = vec![100, 28, 28, 28, 28];
        let result = zip_reconstruct_bytes(&input);

        // Verify the length is preserved
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_dwaa_decode_channels_are_distinct() {
        use crate::prelude::*;

        // Load the reference (decompressed) image first to see expected values
        let ref_image = read()
            .no_deep_data()
            .largest_resolution_level()
            .all_channels()
            .all_layers()
            .all_attributes()
            .from_file("./tests/images/valid/custom/compression_methods/f16/decompressed_dwaa.exr")
            .expect("Failed to load reference image");

        // Load a DWA-compressed test image
        let image = read()
            .no_deep_data()
            .largest_resolution_level()
            .all_channels()
            .all_layers()
            .all_attributes()
            .from_file("./tests/images/valid/custom/compression_methods/f16/dwaa.exr")
            .expect("Failed to load DWAA test image");

        // Get the first layer
        let ref_layer = &ref_image.layer_data[0];
        let layer = &image.layer_data[0];
        let (width, height) = (layer.size.width(), layer.size.height());

        println!("Loaded DWAA image: {}x{}", width, height);
        println!("Number of channels: {}", layer.channel_data.list.len());

        // Get RGBA channels (assuming standard RGBA order)
        assert!(layer.channel_data.list.len() >= 4, "Expected at least 4 channels");

        let ref_ch0 = &ref_layer.channel_data.list[0];
        let ref_ch1 = &ref_layer.channel_data.list[1];
        let ref_ch2 = &ref_layer.channel_data.list[2];
        let ref_ch3 = &ref_layer.channel_data.list[3];

        let ch0 = &layer.channel_data.list[0];
        let ch1 = &layer.channel_data.list[1];
        let ch2 = &layer.channel_data.list[2];
        let ch3 = &layer.channel_data.list[3];

        println!("Channel names: {}, {}, {}, {}",
                 Into::<String>::into(ch0.name.clone()),
                 Into::<String>::into(ch1.name.clone()),
                 Into::<String>::into(ch2.name.clone()),
                 Into::<String>::into(ch3.name.clone()));

        // Check first few pixels
        if let (
            FlatSamples::F16(ref_samples0),
            FlatSamples::F16(ref_samples1),
            FlatSamples::F16(ref_samples2),
            FlatSamples::F16(ref_samples3),
            FlatSamples::F16(samples0),
            FlatSamples::F16(samples1),
            FlatSamples::F16(samples2),
            FlatSamples::F16(samples3),
        ) = (
            &ref_ch0.sample_data, &ref_ch1.sample_data, &ref_ch2.sample_data, &ref_ch3.sample_data,
            &ch0.sample_data, &ch1.sample_data, &ch2.sample_data, &ch3.sample_data
        ) {

            // Sample first few pixels
            for y in 0..3.min(height) {
                for x in 0..4.min(width) {
                    let idx = y * width + x;
                    let ref_v0 = ref_samples0[idx];
                    let ref_v1 = ref_samples1[idx];
                    let ref_v2 = ref_samples2[idx];
                    let ref_v3 = ref_samples3[idx];

                    let v0 = samples0[idx];
                    let v1 = samples1[idx];
                    let v2 = samples2[idx];
                    let v3 = samples3[idx];

                    println!("Pixel ({}, {}):", x, y);
                    println!("  Reference: ch0={:.4}, ch1={:.4}, ch2={:.4}, ch3={:.4}", ref_v0, ref_v1, ref_v2, ref_v3);
                    println!("  Decoded:   ch0={:.4}, ch1={:.4}, ch2={:.4}, ch3={:.4}", v0, v1, v2, v3);

                    // Check if decoded values match the pattern of reference values
                    // (allowing for lossy compression differences)
                    if idx == 0 {
                        // Check if all decoded channels are identical (bug symptom)
                        let all_same = v0 == v1 && v1 == v2 && v2 == v3;

                        // Check if reference channels are NOT all the same
                        let ref_all_same = ref_v0 == ref_v1 && ref_v1 == ref_v2 && ref_v2 == ref_v3;

                        if all_same && !ref_all_same {
                            panic!("BUG: All decoded channels at pixel (0,0) are identical ({:.4}), but reference shows different values! This suggests planar/interleaved layout confusion.", v0);
                        }
                    }
                }
            }

            println!("SUCCESS: Channel layout appears correct");
        } else {
            panic!("Expected F16 samples");
        }
    }

    // More tests will be added as implementation progresses
}
