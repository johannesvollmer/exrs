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
/// This parses the deep block's pixel offset table and sample data to extract
/// all samples for a given pixel index.
///
/// **WARNING**: This function assumes all channels are F32. For mixed F16/F32 channels,
/// use `extract_pixel_samples_typed()` instead.
///
/// ## Arguments
///
/// * `block` - The deep block containing the data
/// * `pixel_idx` - Index of the pixel within the block (row-major order)
/// * `channels` - Number of channels per sample (e.g., 4 for RGBA)
///
/// ## Returns
///
/// A vector of sample values, where each sample has `channels` values.
/// Returns empty vec if pixel has no samples.
pub fn extract_pixel_samples(
    block: &UncompressedDeepBlock,
    pixel_idx: usize,
    channels: usize,
) -> Vec<Vec<f32>> {
    if pixel_idx >= block.pixel_offset_table.len() {
        return Vec::new();
    }

    // Get sample range for this pixel
    let start_sample = if pixel_idx == 0 {
        0
    } else {
        block.pixel_offset_table[pixel_idx - 1] as usize
    };
    let end_sample = block.pixel_offset_table[pixel_idx] as usize;
    let sample_count = end_sample - start_sample;

    if sample_count == 0 {
        return Vec::new();
    }

    // Each sample has `channels` float values
    let bytes_per_sample = channels * std::mem::size_of::<f32>();
    let start_byte = start_sample * bytes_per_sample;
    let end_byte = end_sample * bytes_per_sample;

    if end_byte > block.sample_data.len() {
        return Vec::new();
    }

    let sample_bytes = &block.sample_data[start_byte..end_byte];

    // Parse samples
    let mut samples = Vec::new();
    for sample_idx in 0..sample_count {
        let mut sample = Vec::new();
        for chan in 0..channels {
            let offset = (sample_idx * channels + chan) * std::mem::size_of::<f32>();
            if offset + 4 <= sample_bytes.len() {
                let bytes = [
                    sample_bytes[offset],
                    sample_bytes[offset + 1],
                    sample_bytes[offset + 2],
                    sample_bytes[offset + 3],
                ];
                sample.push(f32::from_ne_bytes(bytes));
            }
        }
        if sample.len() == channels {
            samples.push(sample);
        }
    }

    samples
}

/// Extract deep samples with proper type handling for mixed F16/F32 channels.
///
/// **CRITICAL**: OpenEXR deep data uses **channel-by-channel** layout, not interleaved:
/// - Layout: [all samples of chan0][all samples of chan1][all samples of chan2]...
/// - NOT: [sample0 all chans][sample1 all chans]...
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

    if pixel_idx >= block.pixel_offset_table.len() {
        return Vec::new();
    }

    // Get sample range for this pixel
    let start_sample = if pixel_idx == 0 {
        0
    } else {
        block.pixel_offset_table[pixel_idx - 1] as usize
    };
    let end_sample = block.pixel_offset_table[pixel_idx] as usize;
    let sample_count = end_sample - start_sample;

    if sample_count == 0 {
        return Vec::new();
    }

    // Deep data is stored channel-by-channel
    // Calculate byte size for each channel
    let channel_byte_sizes: Vec<usize> = channel_types.iter().map(|t| match t {
        SampleType::F16 => 2,
        SampleType::F32 => 4,
        SampleType::U32 => 4,
    }).collect();

    let bytes_per_sample_all_channels: usize = channel_byte_sizes.iter().sum();

    // Read each channel's samples
    let mut channel_arrays: Vec<Vec<f32>> = Vec::with_capacity(channel_types.len());
    let mut byte_offset = start_sample * bytes_per_sample_all_channels;

    for (ch_idx, &ch_type) in channel_types.iter().enumerate() {
        let mut channel_samples = Vec::with_capacity(sample_count);
        let bytes_per_value = channel_byte_sizes[ch_idx];

        for _ in 0..sample_count {
            if byte_offset + bytes_per_value > block.sample_data.len() {
                break;
            }

            let value = match ch_type {
                SampleType::F16 => {
                    let bytes = [block.sample_data[byte_offset], block.sample_data[byte_offset + 1]];
                    f16::from_ne_bytes(bytes).to_f32()
                }
                SampleType::F32 => {
                    let bytes = [
                        block.sample_data[byte_offset],
                        block.sample_data[byte_offset + 1],
                        block.sample_data[byte_offset + 2],
                        block.sample_data[byte_offset + 3],
                    ];
                    f32::from_ne_bytes(bytes)
                }
                SampleType::U32 => {
                    let bytes = [
                        block.sample_data[byte_offset],
                        block.sample_data[byte_offset + 1],
                        block.sample_data[byte_offset + 2],
                        block.sample_data[byte_offset + 3],
                    ];
                    u32::from_ne_bytes(bytes) as f32
                }
            };

            channel_samples.push(value);
            byte_offset += bytes_per_value;
        }

        channel_arrays.push(channel_samples);
    }

    // Transpose: convert from channel-by-channel to sample-by-sample
    let mut samples = Vec::with_capacity(sample_count);
    for sample_idx in 0..sample_count {
        let mut sample = Vec::with_capacity(channel_types.len());
        for channel_array in &channel_arrays {
            if sample_idx < channel_array.len() {
                sample.push(channel_array[sample_idx]);
            }
        }
        if sample.len() == channel_types.len() {
            samples.push(sample);
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
}
