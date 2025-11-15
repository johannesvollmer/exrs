//! Merging multiple deep images into a single composited result.
//!
//! This module provides utilities for combining multiple deep data sources
//! (e.g., from multiple layers or separate images) into a single composited image.

use crate::block::UncompressedDeepBlock;
use crate::image::deep::compositing::{composite_samples_front_to_back, DeepSample, make_tidy};
use std::collections::HashMap;

/// A pixel's worth of deep samples from potentially multiple sources.
#[derive(Debug, Clone)]
pub struct MergedPixelSamples {
    /// All samples for this pixel, from all sources
    pub samples: Vec<DeepSample>,
}

impl MergedPixelSamples {
    /// Create a new empty pixel
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
        }
    }

    /// Add samples from a source
    pub fn add_samples(&mut self, samples: &[DeepSample]) {
        self.samples.extend_from_slice(samples);
    }

    /// Sort samples by depth and remove occluded samples
    pub fn tidy(&mut self) {
        make_tidy(&mut self.samples);
    }

    /// Composite all samples to get final color
    pub fn composite(&self) -> ([f32; 3], f32) {
        composite_samples_front_to_back(&self.samples)
    }
}

impl Default for MergedPixelSamples {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract deep samples from a block for a specific pixel.
///
/// Deep sample data inside an EXR block is stored **channel-major**:
/// for each channel, for each pixel (scanline order), for each sample: value.
/// This function translates that packed representation into per-sample values.
///
/// **WARNING**: This helper assumes all channels are `f32`. For mixed channel
/// types, use [`extract_pixel_samples_typed()`] instead.
pub fn extract_pixel_samples(
    block: &UncompressedDeepBlock,
    pixel_idx: usize,
    channels: usize,
) -> Vec<Vec<f32>> {
    use crate::meta::attribute::SampleType;

    if channels == 0 {
        return Vec::new();
    }

    let channel_types = vec![SampleType::F32; channels];
    extract_pixel_samples_typed(block, pixel_idx, &channel_types)
}

/// Extract deep samples with proper type handling for mixed F16/F32 channels.
///
/// Deep sample data is stored channel-major inside a block:
/// for each channel, for each pixel (in scanline order), for each sample: value.
/// The pixel offset table still references cumulative sample counts, so all
/// channels share the same sample indices; only the byte layout differs.
///
/// ## Arguments
///
/// * `block` - The deep block containing the data
/// * `pixel_idx` - Index of the pixel within the block (row-major order)
/// * `channel_types` - Slice of SampleType for each channel
///
/// ## Returns
///
/// A vector of sample values, where each sample has values from all channels.
/// Returns empty vec if pixel has no samples.
pub fn extract_pixel_samples_typed(
    block: &UncompressedDeepBlock,
    pixel_idx: usize,
    channel_types: &[crate::meta::attribute::SampleType],
) -> Vec<Vec<f32>> {
    use crate::meta::attribute::SampleType;
    use half::f16;

    if channel_types.is_empty()
        || block.pixel_offset_table.is_empty()
        || pixel_idx >= block.pixel_offset_table.len()
    {
        return Vec::new();
    }

    let total_samples = match block.pixel_offset_table.last() {
        Some(&count) if count > 0 => count as usize,
        _ => return Vec::new(),
    };

    // Pre-compute where each channel's data begins inside the packed sample buffer.
    // Deep data stores all samples for channel0 first, then channel1, etc.
    #[derive(Copy, Clone)]
    struct ChannelInfo {
        sample_type: SampleType,
        offset: usize,
        value_size: usize,
    }

    let mut channel_infos = Vec::with_capacity(channel_types.len());
    let mut channel_data_offset = 0usize;
    for &sample_type in channel_types {
        let value_size = sample_type.bytes_per_sample();
        let channel_bytes = match total_samples.checked_mul(value_size) {
            Some(size) => size,
            None => return Vec::new(),
        };

        channel_infos.push(ChannelInfo {
            sample_type,
            offset: channel_data_offset,
            value_size,
        });

        channel_data_offset = match channel_data_offset.checked_add(channel_bytes) {
            Some(new_offset) => new_offset,
            None => return Vec::new(),
        };
    }

    if channel_data_offset > block.sample_data.len() {
        return Vec::new();
    }

    // Determine the sample range for the requested pixel.
    let start_sample_i32 = if pixel_idx == 0 {
        0
    } else {
        block.pixel_offset_table[pixel_idx - 1]
    };
    let end_sample_i32 = block.pixel_offset_table[pixel_idx];

    if end_sample_i32 <= start_sample_i32 || start_sample_i32 < 0 || end_sample_i32 < 0 {
        return Vec::new();
    }

    let start_sample = start_sample_i32 as usize;
    let end_sample = end_sample_i32 as usize;

    if end_sample > total_samples {
        return Vec::new();
    }

    let sample_count = end_sample - start_sample;
    let mut samples = Vec::with_capacity(sample_count);

    for relative_sample in 0..sample_count {
        let absolute_sample = start_sample + relative_sample;
        let mut sample_values = Vec::with_capacity(channel_infos.len());

        for info in &channel_infos {
            let byte_index = match absolute_sample
                .checked_mul(info.value_size)
                .and_then(|sample_offset| info.offset.checked_add(sample_offset))
            {
                Some(idx) => idx,
                None => return Vec::new(),
            };

            let end_index = match byte_index.checked_add(info.value_size) {
                Some(idx) => idx,
                None => return Vec::new(),
            };

            if end_index > block.sample_data.len() {
                return Vec::new();
            }

            let value = match info.sample_type {
                SampleType::F16 => {
                    let bytes = [block.sample_data[byte_index], block.sample_data[byte_index + 1]];
                    f16::from_ne_bytes(bytes).to_f32()
                }
                SampleType::F32 => {
                    let bytes = [
                        block.sample_data[byte_index],
                        block.sample_data[byte_index + 1],
                        block.sample_data[byte_index + 2],
                        block.sample_data[byte_index + 3],
                    ];
                    f32::from_ne_bytes(bytes)
                }
                SampleType::U32 => {
                    let bytes = [
                        block.sample_data[byte_index],
                        block.sample_data[byte_index + 1],
                        block.sample_data[byte_index + 2],
                        block.sample_data[byte_index + 3],
                    ];
                    u32::from_ne_bytes(bytes) as f32
                }
            };

            sample_values.push(value);
        }

        if sample_values.len() == channel_infos.len() {
            samples.push(sample_values);
        }
    }

    samples
}

/// Convert raw sample data to DeepSample for compositing.
///
/// Assumes the sample data contains [R, G, B, A, Z] channels in that order.
pub fn samples_to_deep_samples(samples: Vec<Vec<f32>>) -> Vec<DeepSample> {
    samples
        .into_iter()
        .filter_map(|sample| {
            if sample.len() >= 5 {
                // [R, G, B, A, Z]
                let r = sample[0];
                let g = sample[1];
                let b = sample[2];
                let alpha = sample[3];
                let depth = sample[4];

                Some(DeepSample::new_unpremultiplied(depth, [r, g, b], alpha))
            } else {
                None
            }
        })
        .collect()
}

/// Merge multiple deep blocks covering the same spatial region.
///
/// This function takes multiple deep blocks (potentially from different layers
/// or images) that cover the same pixels and merges them into a single composited
/// result per pixel.
///
/// ## Arguments
///
/// * `blocks` - Vec of deep blocks to merge
/// * `channels` - Number of channels per sample (e.g., 5 for RGBA + Z)
///
/// ## Returns
///
/// A map from pixel index to merged samples for that pixel.
pub fn merge_deep_blocks(
    blocks: &[UncompressedDeepBlock],
    channels: usize,
) -> HashMap<usize, MergedPixelSamples> {
    let mut merged: HashMap<usize, MergedPixelSamples> = HashMap::new();

    for block in blocks {
        let num_pixels = block.pixel_offset_table.len();

        for pixel_idx in 0..num_pixels {
            let samples = extract_pixel_samples(block, pixel_idx, channels);

            if !samples.is_empty() {
                let deep_samples = samples_to_deep_samples(samples);

                merged
                    .entry(pixel_idx)
                    .or_insert_with(MergedPixelSamples::new)
                    .add_samples(&deep_samples);
            }
        }
    }

    // Tidy each pixel's samples
    for pixel_samples in merged.values_mut() {
        pixel_samples.tidy();
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BlockIndex;
    use crate::math::Vec2;
    use crate::meta::attribute::SampleType;
    use half::f16;

    #[test]
    fn test_merged_pixel_samples() {
        let mut pixel = MergedPixelSamples::new();

        pixel.add_samples(&[
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5),
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
        ]);

        assert_eq!(pixel.samples.len(), 2);

        pixel.tidy();
        assert_eq!(pixel.samples[0].depth, 1.0);
        assert_eq!(pixel.samples[1].depth, 2.0);

        let (_color, alpha) = pixel.composite();
        assert!(alpha > 0.0);
        assert!(alpha <= 1.0);
    }

    #[test]
    fn test_extract_pixel_samples() {
        // Create a simple test block
        let block = UncompressedDeepBlock {
            index: BlockIndex {
                layer: 0,
                pixel_position: Vec2(0, 0),
                pixel_size: Vec2(2, 2),
                level: Vec2(0, 0),
            },
            pixel_offset_table: vec![2, 4, 6, 8], // Each pixel has 2 samples
            sample_data: {
                // 8 samples * 1 channel * 4 bytes = 32 bytes
                let mut data = Vec::new();
                for i in 0..8 {
                    data.extend_from_slice(&(i as f32).to_ne_bytes());
                }
                data
            },
        };

        let samples = extract_pixel_samples(&block, 0, 1);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0][0], 0.0);
        assert_eq!(samples[1][0], 1.0);

        let samples = extract_pixel_samples(&block, 1, 1);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0][0], 2.0);
        assert_eq!(samples[1][0], 3.0);
    }

    #[test]
    fn test_extract_pixel_samples_typed_channel_major() {
        // Two pixels, each with one sample, two channels with different types.
        let block = UncompressedDeepBlock {
            index: BlockIndex {
                layer: 0,
                pixel_position: Vec2(0, 0),
                pixel_size: Vec2(2, 1),
                level: Vec2(0, 0),
            },
            pixel_offset_table: vec![1, 2],
            sample_data: {
                let mut data = Vec::new();
                // Channel 0 (F16) values for both samples
                data.extend_from_slice(&f16::from_f32(0.5).to_ne_bytes());
                data.extend_from_slice(&f16::from_f32(0.25).to_ne_bytes());
                // Channel 1 (F32) values for both samples
                data.extend_from_slice(&1.25f32.to_ne_bytes());
                data.extend_from_slice(&2.5f32.to_ne_bytes());
                data
            },
        };

        let channel_types = vec![SampleType::F16, SampleType::F32];

        let pixel0 = extract_pixel_samples_typed(&block, 0, &channel_types);
        assert_eq!(pixel0.len(), 1);
        assert!((pixel0[0][0] - 0.5).abs() < 1e-6);
        assert!((pixel0[0][1] - 1.25).abs() < 1e-6);

        let pixel1 = extract_pixel_samples_typed(&block, 1, &channel_types);
        assert_eq!(pixel1.len(), 1);
        assert!((pixel1[0][0] - 0.25).abs() < 1e-6);
        assert!((pixel1[0][1] - 2.5).abs() < 1e-6);
    }
}
