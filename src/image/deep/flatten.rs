//! Deep-to-flat conversion utilities.
//!
//! Composites multiple deep images into a single flat RGBA image,
//! handling spatial alignment via data windows.

use crate::block::UncompressedDeepBlock;
use crate::image::deep::compositing::*;
use crate::image::deep::merge::*;
use crate::math::Vec2;
use crate::meta::attribute::IntegerBounds;
use std::collections::HashMap;

/// Represents a flat RGBA pixel
#[derive(Debug, Clone, Copy)]
pub struct FlatPixel {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for FlatPixel {
    fn default() -> Self {
        Self {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        }
    }
}

/// A deep image source with its spatial bounds
pub struct DeepImageSource {
    /// The blocks containing deep data
    pub blocks: Vec<UncompressedDeepBlock>,
    /// Data window (spatial extent) for this image
    pub data_window: IntegerBounds,
    /// Number of channels per sample (usually 5: A, B, G, R, Z)
    pub num_channels: usize,
}

/// Compute the union of multiple data windows
pub fn union_of_windows(windows: &[IntegerBounds]) -> IntegerBounds {
    if windows.is_empty() {
        return IntegerBounds {
            position: Vec2(0, 0),
            size: Vec2(0, 0),
        };
    }

    let mut min_x = windows[0].position.x();
    let mut min_y = windows[0].position.y();
    let mut max_x = windows[0].position.x() + windows[0].size.x() as i32;
    let mut max_y = windows[0].position.y() + windows[0].size.y() as i32;

    for window in &windows[1..] {
        min_x = min_x.min(window.position.x());
        min_y = min_y.min(window.position.y());
        max_x = max_x.max(window.position.x() + window.size.x() as i32);
        max_y = max_y.max(window.position.y() + window.size.y() as i32);
    }

    IntegerBounds {
        position: Vec2(min_x, min_y),
        size: Vec2((max_x - min_x) as usize, (max_y - min_y) as usize),
    }
}

/// Composite multiple deep images into a flat RGBA image
///
/// This handles spatial alignment by:
/// 1. Computing the union of all data windows
/// 2. For each pixel in the union, collecting samples from all sources
/// 3. Compositing using front-to-back Over operator
///
/// ## Arguments
///
/// * `sources` - Vector of deep image sources with their data windows
///
/// ## Returns
///
/// A tuple of (flat pixels, composite data window)
pub fn composite_deep_to_flat(
    sources: &[DeepImageSource],
) -> (Vec<FlatPixel>, IntegerBounds) {
    // Compute union of all data windows
    let windows: Vec<_> = sources.iter().map(|s| s.data_window).collect();
    let union = union_of_windows(&windows);

    let width = union.size.x();
    let height = union.size.y();
    let mut flat_pixels = vec![FlatPixel::default(); width * height];

    // Process each scanline in the union
    for y in 0..height {
        let global_y = union.position.y() + y as i32;

        for x in 0..width {
            let global_x = union.position.x() + x as i32;
            let mut all_samples = Vec::new();

            // Collect samples from all sources that overlap this pixel
            for source in sources {
                // Check if this pixel is within this source's data window
                let local_x = global_x - source.data_window.position.x();
                let local_y = global_y - source.data_window.position.y();

                if local_x >= 0
                    && local_y >= 0
                    && (local_x as usize) < source.data_window.size.x()
                    && (local_y as usize) < source.data_window.size.y()
                {
                    // Find the block for this scanline
                    if let Some(block) = source
                        .blocks
                        .iter()
                        .find(|b| b.index.pixel_position.y() == local_y as usize)
                    {
                        // Extract samples for this pixel within the block
                        let pixel_idx = local_x as usize;
                        let raw_samples =
                            extract_pixel_samples(block, pixel_idx, source.num_channels);

                        // Convert to DeepSample format (assuming A, B, G, R, Z channel order)
                        for sample in raw_samples {
                            if sample.len() >= 5 {
                                let alpha = sample[0];
                                let b = sample[1];
                                let g = sample[2];
                                let r = sample[3];
                                let depth = sample[4];

                                all_samples.push(DeepSample::new_unpremultiplied(
                                    depth,
                                    [r, g, b],
                                    alpha,
                                ));
                            }
                        }
                    }
                }
            }

            // Sort and composite
            make_tidy(&mut all_samples);
            let (color, alpha) = composite_samples_front_to_back(&all_samples);

            // Unpremultiply for output
            let flat_idx = y * width + x;
            flat_pixels[flat_idx] = if alpha > 0.0001 {
                FlatPixel {
                    r: color[0] / alpha,
                    g: color[1] / alpha,
                    b: color[2] / alpha,
                    a: alpha,
                }
            } else {
                FlatPixel::default()
            };
        }
    }

    (flat_pixels, union)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_union_of_windows() {
        let windows = vec![
            IntegerBounds {
                position: Vec2(0, 0),
                size: Vec2(100, 100),
            },
            IntegerBounds {
                position: Vec2(50, 50),
                size: Vec2(100, 100),
            },
        ];

        let union = union_of_windows(&windows);
        assert_eq!(union.position, Vec2(0, 0));
        assert_eq!(union.size, Vec2(150, 150));
    }

    #[test]
    fn test_union_with_negative_offsets() {
        let windows = vec![
            IntegerBounds {
                position: Vec2(-50, -50),
                size: Vec2(100, 100),
            },
            IntegerBounds {
                position: Vec2(0, 0),
                size: Vec2(100, 100),
            },
        ];

        let union = union_of_windows(&windows);
        assert_eq!(union.position, Vec2(-50, -50));
        assert_eq!(union.size, Vec2(150, 150));
    }
}
