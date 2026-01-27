//! Deep data compositing operations.
//!
//! This module provides utilities for compositing and manipulating deep data.
//! Operations include:
//! - `flatten()` - Composite deep samples into a flat image
//! - `make_tidy()` - Sort samples by depth and remove overlaps
//! - `composite_pixel()` - Front-to-back compositing for a single pixel
//!
//! These operations are based on the OpenEXR deep compositing specification.
//!
//! ## Example
//!
//! ```no_run
//! use exr::prelude::*;
//! use exr::image::deep::compositing::*;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Composite deep samples to get final color
//! let samples = vec![
//!     DeepSample { depth: 1.0, color: [1.0, 0.0, 0.0], alpha: 0.5 },
//!     DeepSample { depth: 2.0, color: [0.0, 1.0, 0.0], alpha: 0.5 },
//! ];
//!
//! let composited = composite_samples_front_to_back(&samples);
//! println!("Composited color: {:?}", composited);
//! # Ok(())
//! # }
//! ```

/// A single deep sample with depth, color, and alpha.
///
/// Used for compositing operations. The `color` field should be pre-multiplied by alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeepSample {
    /// Depth value (Z coordinate)
    pub depth: f32,

    /// RGB color values (should be pre-multiplied by alpha)
    pub color: [f32; 3],

    /// Alpha value (opacity)
    pub alpha: f32,
}

impl DeepSample {
    /// Create a new deep sample with unpremultiplied color.
    /// This will automatically premultiply the color by alpha.
    pub fn new_unpremultiplied(depth: f32, color: [f32; 3], alpha: f32) -> Self {
        Self {
            depth,
            color: [color[0] * alpha, color[1] * alpha, color[2] * alpha],
            alpha,
        }
    }

    /// Create a new deep sample with already premultiplied color.
    pub fn new_premultiplied(depth: f32, color: [f32; 3], alpha: f32) -> Self {
        Self {
            depth,
            color,
            alpha,
        }
    }
}

/// Composite multiple deep samples using front-to-back compositing.
///
/// This performs the standard deep compositing algorithm:
/// - Samples are expected to be sorted by depth (front to back)
/// - Colors should be premultiplied by alpha
/// - Returns the final composited color and alpha
///
/// ## Algorithm
///
/// For each sample from front to back:
/// ```text
/// output_color += sample_color * (1 - output_alpha)
/// output_alpha += sample_alpha * (1 - output_alpha)
/// ```
///
/// ## Arguments
///
/// * `samples` - Deep samples, should be sorted by depth (front to back)
///
/// ## Returns
///
/// A tuple of `(composited_color, composited_alpha)` where color is premultiplied.
///
/// ## Example
///
/// ```
/// use exr::image::deep::compositing::*;
///
/// let samples = vec![
///     DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5),
///     DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
/// ];
///
/// let (color, alpha) = composite_samples_front_to_back(&samples);
/// // Result will be a blend of red and green with combined alpha
/// ```
pub fn composite_samples_front_to_back(samples: &[DeepSample]) -> ([f32; 3], f32) {
    let mut output_color = [0.0, 0.0, 0.0];
    let mut output_alpha = 0.0;

    for sample in samples {
        let transparency = 1.0 - output_alpha;

        output_color[0] += sample.color[0] * transparency;
        output_color[1] += sample.color[1] * transparency;
        output_color[2] += sample.color[2] * transparency;
        output_alpha += sample.alpha * transparency;

        // Early exit if we've reached full opacity
        if output_alpha >= 0.9999 {
            break;
        }
    }

    (output_color, output_alpha)
}

/// Sort deep samples by depth and remove overlapping samples.
///
/// This operation "tidies" deep data by:
/// 1. Sorting all samples by depth (front to back)
/// 2. Removing samples that are fully occluded by samples in front of them
///
/// This is useful for optimizing deep data before further processing.
///
/// ## Arguments
///
/// * `samples` - Deep samples to tidy (will be modified in place)
///
/// ## Example
///
/// ```
/// use exr::image::deep::compositing::*;
///
/// let mut samples = vec![
///     DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
///     DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 1.0), // Fully opaque
///     DeepSample::new_unpremultiplied(3.0, [0.0, 0.0, 1.0], 0.5), // Will be removed (occluded)
/// ];
///
/// make_tidy(&mut samples);
/// // samples[0] will be the opaque red sample at depth 1.0
/// // samples[1] will be removed because it's behind an opaque sample
/// ```
pub fn make_tidy(samples: &mut Vec<DeepSample>) {
    // Sort by depth (front to back)
    samples.sort_by(|a, b| {
        a.depth
            .partial_cmp(&b.depth)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Remove samples that are fully occluded
    let mut accumulated_alpha = 0.0;
    samples.retain(|sample| {
        if accumulated_alpha >= 0.9999 {
            // Everything behind this is occluded
            false
        } else {
            accumulated_alpha += sample.alpha * (1.0 - accumulated_alpha);
            true
        }
    });
}

/// Flatten deep samples into a single RGBA color.
///
/// This is a convenience function that composites all samples and returns
/// an unpremultiplied RGBA color suitable for display.
///
/// ## Arguments
///
/// * `samples` - Deep samples to flatten
///
/// ## Returns
///
/// An RGBA color with unpremultiplied values `[R, G, B, A]`.
///
/// ## Example
///
/// ```
/// use exr::image::deep::compositing::*;
///
/// let samples = vec![
///     DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5),
///     DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
/// ];
///
/// let rgba = flatten_to_rgba(&samples);
/// println!("Flattened color: {:?}", rgba);
/// ```
pub fn flatten_to_rgba(samples: &[DeepSample]) -> [f32; 4] {
    let (color, alpha) = composite_samples_front_to_back(samples);

    // Unpremultiply the color for output
    let unpremultiplied = if alpha > 0.0001 {
        [color[0] / alpha, color[1] / alpha, color[2] / alpha]
    } else {
        [0.0, 0.0, 0.0]
    };

    [
        unpremultiplied[0],
        unpremultiplied[1],
        unpremultiplied[2],
        alpha,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_sample_compositing() {
        let samples = vec![DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5)];

        let (color, alpha) = composite_samples_front_to_back(&samples);
        assert_eq!(alpha, 0.5);
        assert_eq!(color[0], 0.5); // Premultiplied red
    }

    #[test]
    fn test_two_samples_compositing() {
        let samples = vec![
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5),
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
        ];

        let (color, alpha) = composite_samples_front_to_back(&samples);

        // First sample contributes: R=0.5 (0.5 red * 0.5 alpha)
        // Second sample contributes: G=0.25 (0.5 green * 0.5 alpha * 0.5 transparency)
        // Alpha: 0.5 + 0.5 * 0.5 = 0.75
        assert!((alpha - 0.75).abs() < 0.0001);
        assert!((color[0] - 0.5).abs() < 0.0001);
        assert!((color[1] - 0.25).abs() < 0.0001);
    }

    #[test]
    fn test_make_tidy_sorts() {
        let mut samples = vec![
            DeepSample::new_unpremultiplied(3.0, [0.0, 0.0, 1.0], 0.5),
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5),
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
        ];

        make_tidy(&mut samples);

        assert_eq!(samples[0].depth, 1.0);
        assert_eq!(samples[1].depth, 2.0);
        assert_eq!(samples[2].depth, 3.0);
    }

    #[test]
    fn test_make_tidy_removes_occluded() {
        let mut samples = vec![
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 1.0), // Opaque
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5), // Behind opaque, should be removed
        ];

        make_tidy(&mut samples);

        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].depth, 1.0);
    }

    #[test]
    fn test_flatten_to_rgba() {
        let samples = vec![DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5)];

        let rgba = flatten_to_rgba(&samples);
        assert_eq!(rgba[0], 1.0); // Unpremultiplied red
        assert_eq!(rgba[3], 0.5); // Alpha
    }
}
