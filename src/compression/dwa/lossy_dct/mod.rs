//! Lossy DCT codec: shared AC/DC streams for each LOSSY_DCT channel group.
//!
//! RGB groups go through modified-709 CSC (`color_space_conversion`) and DCT
//! in batches. Decode prefers fused x86 SIMD (`x86/`) before the strip-tile
//! fallback.

use std::convert::TryInto;

use half::f16;

use super::{ChannelInfo, CompressorScheme, color_space_conversion, discrete_cosine_transform};
use crate::{
    error::{Error, Result},
    meta::attribute::SampleType,
};

mod ac_rle;
mod half_float_quantizer;
mod quantization;
mod transfer_curve;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

use ac_rle::{rle_ac, un_rle_ac};
use quantization::{QuantTables, from_half_zigzag, quantize_coefficients_to_zigzag};
use transfer_curve::{to_linear_table, to_nonlinear_table};

pub(super) fn encode_lossy_channels(
    infos: &[ChannelInfo],
    csc_groups: &[[usize; 3]],
    channel_bytes: &[Vec<u8>],
    quant_base_error: f32,
) -> Result<(Vec<u16>, Vec<u16>)> {
    // Lossy chunks use shared AC/DC streams. CSC triplets consume the streams
    // first, then standalone LOSSY_DCT channels continue from the same cursors.
    let mut ac = Vec::new();
    let mut dc = Vec::new();
    let mut grouped = vec![false; infos.len()];

    for &group in csc_groups {
        let info = &infos[group[0]];
        let components = group
            .iter()
            .map(|&channel| channel_half_samples(&channel_bytes[channel], &infos[channel], true))
            .collect::<Result<Vec<_>>>()?;

        encode_lossy_dct_group(
            &components,
            info.width,
            info.height,
            quant_base_error,
            &mut ac,
            &mut dc,
        )?;

        for &channel in &group {
            grouped[channel] = true;
        }
    }

    for (index, info) in infos.iter().enumerate() {
        if grouped[index] || info.scheme != CompressorScheme::LossyDct {
            continue;
        }

        let apply_nonlinear = !info.quantize_linearly;
        let samples = channel_half_samples(&channel_bytes[index], info, apply_nonlinear)?;
        encode_lossy_dct_group(
            &[samples],
            info.width,
            info.height,
            quant_base_error,
            &mut ac,
            &mut dc,
        )?;
    }

    Ok((ac, dc))
}

fn channel_half_samples(
    bytes: &[u8],
    info: &ChannelInfo,
    apply_nonlinear: bool,
) -> Result<Vec<u16>> {
    // OpenEXR stores lossy input as half precision internally before DCT.
    // F32 channels are clamped to the finite half range and demoted here.
    let mut samples = Vec::with_capacity(info.width * info.height);

    match info.sample_type {
        SampleType::F16 => {
            let chunks = bytes.chunks_exact(2);
            if !chunks.remainder().is_empty() {
                return Err(Error::invalid("DWA f16 channel data size"));
            }
            samples.extend(chunks.map(|pair| u16::from_le_bytes([pair[0], pair[1]])));
        }
        SampleType::F32 => {
            let chunks = bytes.chunks_exact(4);
            if !chunks.remainder().is_empty() {
                return Err(Error::invalid("DWA f32 channel data size"));
            }
            samples.extend(chunks.map(|quad| {
                let mut value = f32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]);
                value = value.clamp(-65504.0, 65504.0);
                f16::from_f32(value).to_bits()
            }));
        }
        SampleType::U32 => {
            return Err(Error::unsupported("DWA lossy DCT compression of u32 channels"));
        }
    }

    if samples.len() != info.width * info.height {
        return Err(Error::invalid("DWA lossy channel data size mismatch"));
    }

    if apply_nonlinear {
        let to_nonlinear = to_nonlinear_table();
        for sample in &mut samples {
            *sample = to_nonlinear[*sample as usize];
        }
    }

    Ok(samples)
}

fn encode_lossy_dct_group(
    components: &[Vec<u16>],
    width: usize,
    height: usize,
    quant_base_error: f32,
    ac: &mut Vec<u16>,
    dc: &mut Vec<u16>,
) -> Result<()> {
    if width == 0 || height == 0 {
        return Ok(());
    }

    let component_count = components.len();
    if component_count != 1 && component_count != 3 {
        return Err(Error::invalid("invalid DWA lossy component count"));
    }

    for component in components {
        if component.len() != width * height {
            return Err(Error::invalid("DWA lossy component size mismatch"));
        }
    }

    // Mirror the source block edges so partial 8x8 blocks behave the same way
    // as the reference encoder.
    let quant_tables = QuantTables::new(quant_base_error);
    let blocks_x = (width + 7) / 8;
    let blocks_y = (height + 7) / 8;
    let block_count = blocks_x * blocks_y;
    let mut group_dc: Vec<Vec<u16>> =
        (0..component_count).map(|_| Vec::with_capacity(block_count)).collect();

    let mut row_blocks: Vec<[[f32; 64]; 3]> = vec![[[0.0; 64]; 3]; blocks_x];

    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            for component_index in 0..component_count {
                let block = &mut row_blocks[block_x][component_index];
                for y in 0..8 {
                    let src_y = mirror_index(block_y * 8 + y, height);
                    for x in 0..8 {
                        let src_x = mirror_index(block_x * 8 + x, width);
                        let bits = components[component_index][src_y * width + src_x];
                        block[y * 8 + x] = f16::from_bits(bits).to_f32();
                    }
                }
            }
        }

        if component_count == 3 {
            // CSC is performed in nonlinear space for the RGB triplet, batched
            // across the whole row at once (same shape as the DCT batch below).
            color_space_conversion::csc709_forward_8x8_batch(row_blocks.iter_mut());
        }

        discrete_cosine_transform::dct_forward_8x8_batch(
            row_blocks.iter_mut().flat_map(|blocks| blocks[..component_count].iter_mut()),
        );

        for block_x in 0..blocks_x {
            for component_index in 0..component_count {
                let block = &mut row_blocks[block_x][component_index];
                let (tolerances, half_tolerances) = if component_index == 0 {
                    (&quant_tables.y, &quant_tables.half_y)
                } else {
                    (&quant_tables.cbcr, &quant_tables.half_cbcr)
                };

                let half_zig = quantize_coefficients_to_zigzag(block, tolerances, half_tolerances);
                group_dc[component_index].push(half_zig[0]);
                rle_ac(&half_zig, ac);
            }
        }
    }

    for component_dc in group_dc {
        dc.extend(component_dc);
    }

    Ok(())
}

fn mirror_index(index: usize, length: usize) -> usize {
    // The C encoder mirrors out-of-bounds coordinates back into the image
    // rather than clamping them. This keeps edge blocks symmetrical.
    debug_assert_ne!(length, 0);
    let mut value = index as isize;
    let length = length as isize;

    if value >= length {
        value = length - (value - (length - 1));
    }
    if value < 0 {
        value = length - 1;
    }

    value as usize
}

/// One of the chunk-global u16 streams (AC or DC). All channel groups of a
/// chunk consume the same stream, so the cursor carries across groups.
pub(super) struct PackedStream<'v> {
    values: &'v [u16],
    cursor: usize,
}

impl<'v> PackedStream<'v> {
    fn new(values: &'v [u16]) -> Self {
        Self {
            values,
            cursor: 0,
        }
    }

    fn next(&mut self) -> Option<u16> {
        let value = self.values.get(self.cursor).copied();
        self.cursor += 1;
        value
    }

    /// Value at "offset" past the cursor, without consuming (the DC stream
    /// is indexed planar per group and advanced once at group end).
    fn peek_at(&self, offset: usize) -> Option<u16> {
        self.values.get(self.cursor + offset).copied()
    }

    fn advance(&mut self, count: usize) {
        self.cursor += count;
    }

    /// Values left between the cursor and the end of the stream.
    fn remaining(&self) -> usize {
        self.values.len().saturating_sub(self.cursor)
    }

    /// The next `len` values as a plain slice, without consuming them.
    fn peek_slice(&self, len: usize) -> &'v [u16] {
        &self.values[self.cursor..self.cursor + len]
    }
}

/// Where one channel's decoded lossy DCT output should land in the final
/// scanline-interleaved output buffer: its sample type (for serialization)
/// and, per local row, the byte offset `compute_row_offsets` assigned it.
/// Writing straight here instead of into an intermediate per-channel buffer
/// avoids a second full-image copy pass (the equivalent of OpenEXR C++'s
/// `LossyDctDecoder_execute` writing directly into its output rows).
pub(super) struct ScanlineTarget<'a> {
    pub(super) sample_type: SampleType,
    pub(super) row_offsets: &'a [usize],
}

/// Decode all LOSSY_DCT channels directly into `out`: first every CSC group,
/// then the standalone channels, both in channel order - the order in which
/// the encoder appended them to the shared AC/DC streams.
pub(super) fn decode_lossy_channels(
    infos: &[ChannelInfo],
    csc_groups: &[[usize; 3]],
    ac_packed: &[u16],
    dc_packed: &[u16],
    row_offsets: &[Vec<usize>],
    out: &mut [u8],
) -> Result<()> {
    // Decode CSC triplets first, then standalone lossy channels. The shared
    // AC/DC cursors advance in the same order the encoder wrote them.
    let mut ac = PackedStream::new(ac_packed);
    let mut dc = PackedStream::new(dc_packed);

    let mut grouped = vec![false; infos.len()];

    for &group in csc_groups {
        // all three channels have identical sampling, hence identical size
        let info = &infos[group[0]];
        let mut targets: [ScanlineTarget<'_>; 3] = std::array::from_fn(|i| {
            let channel = group[i];
            ScanlineTarget {
                sample_type: infos[channel].sample_type,
                row_offsets: &row_offsets[channel],
            }
        });

        decode_lossy_dct_group(
            &mut ac,
            &mut dc,
            info.width,
            info.height,
            Some(to_linear_table()),
            &mut targets,
            out,
        )?;

        for &channel in &group {
            grouped[channel] = true;
        }
    }

    for (index, info) in infos.iter().enumerate() {
        if grouped[index] || info.scheme != CompressorScheme::LossyDct {
            continue;
        }
        let mut targets = [ScanlineTarget {
            sample_type: info.sample_type,
            row_offsets: &row_offsets[index],
        }];
        let to_linear = (!info.quantize_linearly).then(to_linear_table);
        decode_lossy_dct_group(
            &mut ac,
            &mut dc,
            info.width,
            info.height,
            to_linear,
            &mut targets,
            out,
        )?;
    }

    Ok(())
}

/// Decode one standalone channel (targets.len() == 1) or one CSC'd R/G/B
/// triplet (targets.len() == 3): per 8x8 block and component, read the
/// DC value, un-RLE the AC values, inverse-DCT, and write straight into the
/// final output buffer at each target's precomputed row offsets.
fn decode_lossy_dct_group(
    ac: &mut PackedStream<'_>,
    dc: &mut PackedStream<'_>,
    width: usize,
    height: usize,
    to_linear: Option<&[u16; 65536]>,
    targets: &mut [ScanlineTarget<'_>],
    out: &mut [u8],
) -> Result<()> {
    // Prefer the AVX-512 (V4) fused path when available: same fused shape as
    // the AVX2 path below, but the DCT/CSC middle step processes 2 spatial
    // blocks at once through 512-bit registers (see
    // `try_decode_group_fused_avx512`).
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if let Some(result) =
        x86::try_decode_group_fused_avx512(ac, dc, width, height, to_linear, targets, out)
    {
        return result;
    }

    // Otherwise, prefer the fused per-block path on AVX2+F16C hosts: one
    // spatial 8x8 (or RGB triplet) finishes unRLE->iDCT->CSC->write while its
    // ~1 KiB working set is still L1-hot, matching OpenEXR's LossyDctDecoder
    // shape more closely than the strip-tiled multi-pass fallback below.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if let Some(result) =
        x86::try_decode_group_fused(ac, dc, width, height, to_linear, targets, out)
    {
        return result;
    }

    let components = targets.len();
    let blocks_x = (width + 7) / 8;
    let blocks_y = (height + 7) / 8;
    let block_count = blocks_x * blocks_y;

    // Buffer a strip of block-rows at a time and reuse it across strips,
    // the same streaming shape the encoder already uses per row, widened to
    // amortize the batch dispatch's fixed per-call cost. Buffering the whole
    // group at once, always 3-wide, can be tens of megabytes for a wide DWAB
    // chunk (256 scanlines), which relies on a big-enough L3 to avoid
    // spilling to main memory and, under parallel decode, several chunks'
    // worth of that footprint compete for the same shared L3 at once. A
    // strip of a few block-rows keeps each thread's working set to a few MB
    // (regardless of image width) while still batching thousands of blocks
    // per dispatch.
    const STRIP_BLOCK_ROWS: usize = 1;

    // The strip is additionally tiled along x.
    //
    // 32 blocks is the widest tile whose buffers still fit a 48 KiB L1d: the
    // tile is 32 * 3 * 256 = 24 KiB, plus the ~12 KiB of output rows the last
    // pass writes for it. Measured against no x tiling, it removes ~17% of the
    // decode's L1 misses and ~31% of its L2 misses; the wall-clock effect is
    // under 1%, because the out-of-order engine already overlapped most of
    // those misses, but it is reproducible and never a regression.
    //
    // Requires `STRIP_BLOCK_ROWS == 1`: the AC stream is a sequential bitstream
    // written in block-row-major order, so an x tile may only be the innermost
    // block loop of a single row, never span several buffered rows.
    const _: () = assert!(STRIP_BLOCK_ROWS == 1);
    const STRIP_BLOCK_COLS: usize = 32;

    let tile_capacity = STRIP_BLOCK_COLS.min(blocks_x.max(1));
    let strip_capacity = tile_capacity * STRIP_BLOCK_ROWS.min(blocks_y.max(1));
    let mut row_blocks: Vec<[f32; 64]> = vec![[0.0f32; 64]; strip_capacity * components];
    let mut needs_inverse_dct: Vec<bool> = vec![false; strip_capacity * components];

    for strip_start in (0..blocks_y).step_by(STRIP_BLOCK_ROWS) {
        let strip_rows = STRIP_BLOCK_ROWS.min(blocks_y - strip_start);

        for tile_start in (0..blocks_x).step_by(STRIP_BLOCK_COLS) {
            let tile_cols = STRIP_BLOCK_COLS.min(blocks_x - tile_start);
            let strip_blocks = tile_cols * strip_rows;

            for row_in_strip in 0..strip_rows {
                let block_y = strip_start + row_in_strip;

                for tile_x in 0..tile_cols {
                    let block_x = tile_start + tile_x;
                    let block_index = block_y * blocks_x + block_x;

                    for component in 0..components {
                        let mut zig_block = [0u16; 64];

                        // the DC stream is planar: all of component 0's blocks,
                        // then all of component 1's, ... (indexed against the whole
                        // group's block_count, even though only one strip is buffered)
                        zig_block[0] = dc
                            .peek_at(component * block_count + block_index)
                            .ok_or_else(|| Error::invalid("truncated DWA DC data"))?;

                        let last_non_zero = un_rle_ac(ac, &mut zig_block)?;

                        let slot = (row_in_strip * tile_cols + tile_x) * components + component;
                        let dct_block = &mut row_blocks[slot];
                        if last_non_zero == 0 {
                            // DC-only block: all AC coefficients are zero, so the
                            // inverse DCT can fill the whole block from one value.
                            dct_block[0] = f16::from_bits(zig_block[0]).to_f32();
                            discrete_cosine_transform::dct_inverse_8x8_dc_only(dct_block);
                            needs_inverse_dct[slot] = false;
                        } else {
                            from_half_zigzag(&zig_block, dct_block);
                            needs_inverse_dct[slot] = true;
                        }
                    }
                }
            }

            let strip_slots = strip_blocks * components;
            discrete_cosine_transform::dct_inverse_8x8_batch(
                row_blocks[..strip_slots]
                    .iter_mut()
                    .zip(needs_inverse_dct[..strip_slots].iter())
                    .filter_map(|(block, &needed)| needed.then_some(block)),
            );

            if components == 3 {
                // Batched across the whole strip at once (same shape as the DCT batch
                // above). A `[f32; 64]` triplet is layout-identical to `[[f32; 64]; 3]`,
                // so this reinterprets 3-block chunks of the flat buffer without a copy.
                color_space_conversion::csc709_inverse_8x8_batch(
                    row_blocks[..strip_slots]
                        .chunks_exact_mut(3)
                        .map(|triplet| triplet.try_into().unwrap()),
                );
            }

            for row_in_strip in 0..strip_rows {
                let block_y = strip_start + row_in_strip;
                let y_count = 8.min(height - block_y * 8);

                for tile_x in 0..tile_cols {
                    let block_x = tile_start + tile_x;
                    let base = (row_in_strip * tile_cols + tile_x) * components;
                    let x_count = 8.min(width - block_x * 8);

                    // Convert nonlinear DCT output back to linear half values, crop
                    // the edges to the actual image extent, and serialize straight
                    // into the final scanline buffer at this target's row offsets
                    // (no intermediate per-channel buffer + later copy, mirroring
                    // OpenEXR C++'s LossyDctDecoder_execute writing directly into
                    // its output rows). `to_linear` and the sample type are the
                    // same for the whole call, so match them once per
                    // block/component here instead of once per pixel.
                    for (component, target) in targets.iter_mut().enumerate() {
                        let block = &row_blocks[base + component];
                        let bytes_per_sample = target.sample_type.bytes_per_sample();

                        macro_rules! write_row {
                            ($linearize:expr) => {
                                for dy in 0..y_count {
                                    let y = block_y * 8 + dy;
                                    let row = &block[dy * 8..dy * 8 + x_count];
                                    let offset =
                                        target.row_offsets[y] + block_x * 8 * bytes_per_sample;
                                    let out_row = &mut out[offset..][..x_count * bytes_per_sample];

                                    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                                    {
                                        let handled = match target.sample_type {
                                            SampleType::F16 => {
                                                x86::try_write_row_f16(row, to_linear, out_row)
                                                    || x86::try_write_row_f16_sse2(
                                                        row, to_linear, out_row,
                                                    )
                                            }
                                            SampleType::F32 => {
                                                x86::try_write_row_f32(row, to_linear, out_row)
                                                    || x86::try_write_row_f32_sse2(
                                                        row, to_linear, out_row,
                                                    )
                                            }
                                            SampleType::U32 => false,
                                        };
                                        if handled {
                                            continue;
                                        }
                                    }
                                    match target.sample_type {
                                        SampleType::F16 => {
                                            for (chunk, &value) in
                                                out_row.chunks_exact_mut(2).zip(row)
                                            {
                                                let linear: f16 = $linearize(value);
                                                chunk.copy_from_slice(
                                                    &linear.to_bits().to_le_bytes(),
                                                );
                                            }
                                        }
                                        SampleType::F32 => {
                                            for (chunk, &value) in
                                                out_row.chunks_exact_mut(4).zip(row)
                                            {
                                                let linear: f16 = $linearize(value);
                                                chunk.copy_from_slice(
                                                    &linear.to_f32().to_le_bytes(),
                                                );
                                            }
                                        }
                                        // rejected before decoding
                                        SampleType::U32 => {
                                            return Err(Error::unsupported(
                                                "DWA lossy DCT compression of u32 channels",
                                            ));
                                        }
                                    }
                                }
                            };
                        }

                        match to_linear {
                            Some(table) => write_row!(|value: f32| -> f16 {
                                let nonlinear = f16::from_f32(value);
                                f16::from_bits(table[nonlinear.to_bits() as usize])
                            }),
                            None => write_row!(|value: f32| -> f16 { f16::from_f32(value) }),
                        }
                    }
                }
            }
        }
    }

    dc.advance(components * block_count);
    Ok(())
}
