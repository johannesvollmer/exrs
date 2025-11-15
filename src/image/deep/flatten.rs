//! Deep-to-flat conversion utilities.
//!
//! Composites multiple deep images into a single flat RGBA image,
//! handling spatial alignment via data windows.

use crate::block::UncompressedDeepBlock;
use crate::image::deep::merge::extract_pixel_samples_typed;
use std::cmp::Ordering;
use crate::math::Vec2;
use crate::meta::attribute::IntegerBounds;
use smallvec::SmallVec;

/// Represents a flat RGBA pixel
#[derive(Debug, Clone, Copy)]
pub struct FlatPixel {
    /// Premultiplied red component.
    pub r: f32,
    /// Premultiplied green component.
    pub g: f32,
    /// Premultiplied blue component.
    pub b: f32,
    /// Accumulated alpha component.
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
#[derive(Debug)]
pub struct DeepImageSource {
    /// The blocks containing deep data
    pub blocks: Vec<UncompressedDeepBlock>,
    /// Data window (spatial extent) for this image
    pub data_window: IntegerBounds,
    /// Identifier used for debugging/logging.
    pub label: String,
    /// Channel names in the order stored in the deep block
    pub channel_names: Vec<String>,
    /// Channel types for proper data extraction
    pub channel_types: Vec<crate::meta::attribute::SampleType>,
}

/// Compute the union of multiple data windows
pub fn union_of_windows(windows: &[IntegerBounds]) -> IntegerBounds {
    if windows.is_empty() {
        return IntegerBounds {
            position: Vec2(0, 0),
            size: Vec2(0, 0),
        };
    }

    let (min_x, min_y, max_x, max_y) = windows[1..].iter().fold(
        (
            windows[0].position.x(),
            windows[0].position.y(),
            windows[0].position.x() + windows[0].size.x() as i32,
            windows[0].position.y() + windows[0].size.y() as i32,
        ),
        |(min_x, min_y, max_x, max_y), window| {
            (
                min_x.min(window.position.x()),
                min_y.min(window.position.y()),
                max_x.max(window.position.x() + window.size.x() as i32),
                max_y.max(window.position.y() + window.size.y() as i32),
            )
        },
    );

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
    target_window: Option<IntegerBounds>,
) -> (Vec<FlatPixel>, IntegerBounds) {
    #[derive(Clone, Copy, Default)]
    struct ChannelLookup {
        z: Option<usize>,
        z_back: Option<usize>,
        alpha: Option<usize>,
        r: Option<usize>,
        g: Option<usize>,
        b: Option<usize>,
    }

    impl ChannelLookup {
        fn from_names(names: &[String]) -> Self {
            let mut lookup = names.iter().enumerate().fold(
                ChannelLookup::default(),
                |mut acc, (idx, name)| {
                    match name.as_str() {
                        "Z" => acc.z = Some(idx),
                        "ZBack" => acc.z_back = Some(idx),
                        "A" => acc.alpha = Some(idx),
                        "R" => acc.r = Some(idx),
                        "G" => acc.g = Some(idx),
                        "B" => acc.b = Some(idx),
                        _ => {}
                    }
                    acc
                },
            );

            if lookup.z_back.is_none() {
                lookup.z_back = lookup.z;
            }

            lookup
        }

        fn is_usable(&self) -> bool {
            self.z.is_some() && self.alpha.is_some()
        }
    }

    struct SampleData {
        z: f32,
        z_back: f32,
        alpha: f32,
        color: [f32; 3],
    }

    struct GatheredSamples {
        samples: SmallVec<[SampleData; 8]>,
        contributing_sources: usize,
    }

    impl GatheredSamples {
        fn new() -> Self {
            Self {
                samples: SmallVec::new(),
                contributing_sources: 0,
            }
        }

        fn push_sample(&mut self, sample: &[f32], lookup: &ChannelLookup) {
            let depth_idx = match lookup.z {
                Some(idx) => idx,
                None => return,
            };
            let alpha_idx = match lookup.alpha {
                Some(idx) => idx,
                None => return,
            };

            let depth = *sample.get(depth_idx).unwrap_or(&0.0);
            let depth_back = lookup
                .z_back
                .and_then(|idx| sample.get(idx))
                .copied()
                .unwrap_or(depth);
            let alpha = *sample.get(alpha_idx).unwrap_or(&0.0);

            let get_channel = |idx: Option<usize>| -> f32 {
                idx.and_then(|i| sample.get(i)).copied().unwrap_or(0.0)
            };

            self.samples.push(SampleData {
                z: depth,
                z_back: depth_back,
                alpha,
                color: [
                    get_channel(lookup.r),
                    get_channel(lookup.g),
                    get_channel(lookup.b),
                ],
            });
        }

        fn total_samples(&self) -> usize {
            self.samples.len()
        }
    }

    fn composite_samples(samples: &GatheredSamples) -> FlatPixel {
        let total = samples.total_samples();
        if total == 0 {
            return FlatPixel::default();
        }

        let mut outputs = [0.0f32; 4];

        let mut sort_order: SmallVec<[usize; 8]> = (0..total).collect();
        let sort_required = samples.contributing_sources > 1;
        if sort_required {
            sort_order.sort_by(|&a, &b| {
                samples.samples[a].z.partial_cmp(&samples.samples[b].z)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| {
                        samples.samples[a].z_back.partial_cmp(&samples.samples[b].z_back)
                            .unwrap_or(Ordering::Equal)
                    })
                    .then_with(|| a.cmp(&b))
            });
        }

        let mut accumulate = |sample: &SampleData| -> bool {
            let accumulated_alpha = outputs[3];
            if accumulated_alpha >= 1.0 {
                return true;
            }

            let transparency = 1.0 - accumulated_alpha;
            outputs[0] += transparency * sample.color[0];
            outputs[1] += transparency * sample.color[1];
            outputs[2] += transparency * sample.color[2];
            outputs[3] += transparency * sample.alpha;
            false
        };

        if sort_required {
            for &idx in sort_order.iter() {
                if accumulate(&samples.samples[idx]) {
                    break;
                }
            }
        } else {
            for sample in samples.samples.iter() {
                if accumulate(sample) {
                    break;
                }
            }
        }

        FlatPixel {
            r: outputs[0],
            g: outputs[1],
            b: outputs[2],
            a: outputs[3],
        }
    }

    // Compute union of all data windows
    let windows: Vec<_> = sources.iter().map(|s| s.data_window).collect();
    let union = if let Some(window) = target_window {
        window
    } else {
        union_of_windows(&windows)
    };

    let width = union.size.x();
    let height = union.size.y();
    let mut flat_pixels = vec![FlatPixel::default(); width * height];

    let channel_maps: Vec<ChannelLookup> = sources
        .iter()
        .map(|src| ChannelLookup::from_names(&src.channel_names))
        .collect();

    let debug_pixel = std::env::var("DEBUG_PIXEL")
        .ok()
        .and_then(|value| {
            let mut parts = value.split(',');
            let x = parts.next()?.trim().parse::<i32>().ok()?;
            let y = parts.next()?.trim().parse::<i32>().ok()?;
            Some((x, y))
        });

    let process_pixel = |global_x: i32, global_y: i32| -> FlatPixel {
        let mut gathered = GatheredSamples::new();
        let is_debug_pixel = debug_pixel
            .map(|(dx, dy)| dx == global_x && dy == global_y)
            .unwrap_or(false);

        sources
            .iter()
            .enumerate()
            .filter_map(|(source_idx, source)| {
                let channel_lookup = &channel_maps[source_idx];
                if !channel_lookup.is_usable() {
                    return None;
                }

                let local_x = global_x - source.data_window.position.x();
                let local_y = global_y - source.data_window.position.y();

                if local_x < 0
                    || local_y < 0
                    || (local_x as usize) >= source.data_window.size.x()
                    || (local_y as usize) >= source.data_window.size.y()
                {
                    return None;
                }

                let block = source.blocks.iter().find(|b| {
                    let block_x_start = b.index.pixel_position.x() as i32;
                    let block_y_start = b.index.pixel_position.y() as i32;
                    let block_x_end = block_x_start + b.index.pixel_size.x() as i32;
                    let block_y_end = block_y_start + b.index.pixel_size.y() as i32;

                    local_x >= block_x_start
                        && local_x < block_x_end
                        && local_y >= block_y_start
                        && local_y < block_y_end
                })?;

                let block_x_offset = (local_x - block.index.pixel_position.x() as i32) as usize;
                let block_y_offset = (local_y - block.index.pixel_position.y() as i32) as usize;
                let block_width = block.index.pixel_size.x();
                let pixel_idx = block_y_offset * block_width + block_x_offset;

                Some((
                    source_idx,
                    channel_lookup,
                    block,
                    pixel_idx,
                    &source.channel_types,
                    source.label.as_str(),
                ))
            })
            .for_each(
                |(source_idx, channel_lookup, block, pixel_idx, channel_types, label)| {
                    let raw_samples =
                        extract_pixel_samples_typed(block, pixel_idx, channel_types);

                    if raw_samples.is_empty() {
                        return;
                    }

                    gathered.contributing_sources += 1;
                    if is_debug_pixel {
                        println!(
                            "[debug pixel] source {} ({}) has {} samples",
                            source_idx,
                            label,
                            raw_samples.len()
                        );
                    }

                    raw_samples.into_iter().for_each(|sample| {
                        if is_debug_pixel {
                            println!(
                                "[debug pixel] source {} ({}) sample: A={:.6}, R={:.6}, G={:.6}, B={:.6}, Z={:.3}",
                                source_idx,
                                label,
                                channel_lookup
                                    .alpha
                                    .and_then(|idx| sample.get(idx))
                                    .copied()
                                    .unwrap_or(0.0),
                                channel_lookup
                                    .r
                                    .and_then(|idx| sample.get(idx))
                                    .copied()
                                    .unwrap_or(0.0),
                                channel_lookup
                                    .g
                                    .and_then(|idx| sample.get(idx))
                                    .copied()
                                    .unwrap_or(0.0),
                                channel_lookup
                                    .b
                                    .and_then(|idx| sample.get(idx))
                                    .copied()
                                    .unwrap_or(0.0),
                                channel_lookup
                                    .z
                                    .and_then(|idx| sample.get(idx))
                                    .copied()
                                    .unwrap_or(0.0)
                            );
                        }
                        gathered.push_sample(&sample, channel_lookup);
                    });
                },
            );

        if gathered.total_samples() == 0 {
            return FlatPixel::default();
        }

        let result = composite_samples(&gathered);
        if is_debug_pixel {
            println!(
                "[debug pixel] composite result: R={:.6} G={:.6} B={:.6} A={:.6}",
                result.r, result.g, result.b, result.a
            );
        }
        result
    };

    #[cfg(feature = "rayon")]
    {
        use rayon::prelude::*;
        flat_pixels
            .par_chunks_mut(width)
            .enumerate()
            .for_each(|(row_idx, row)| {
                let global_y = union.position.y() + row_idx as i32;
                row.iter_mut().enumerate().for_each(|(col_idx, pixel)| {
                    let global_x = union.position.x() + col_idx as i32;
                    *pixel = process_pixel(global_x, global_y);
                });
            });
    }

    #[cfg(not(feature = "rayon"))]
    {
        flat_pixels
            .chunks_mut(width)
            .enumerate()
            .for_each(|(row_idx, row)| {
                let global_y = union.position.y() + row_idx as i32;
                row.iter_mut().enumerate().for_each(|(col_idx, pixel)| {
                    let global_x = union.position.x() + col_idx as i32;
                    *pixel = process_pixel(global_x, global_y);
                });
            });
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
