//! Deep sample storage for images with multiple samples per pixel.
//!
//! This module provides data structures for storing and accessing deep samples,
//! where each pixel can have a variable number of samples at different depths.

#[cfg(feature = "deep-data")]
use crate::error::{Error, Result, UnitResult};
#[cfg(feature = "deep-data")]
use crate::image::FlatSamples;
#[cfg(feature = "deep-data")]
use crate::math::Vec2;
#[cfg(feature = "deep-data")]
use std::convert::TryFrom;

/// Storage for deep samples with variable sample counts per pixel.
///
/// Deep images store multiple samples per pixel at different depths. This type
/// efficiently stores the samples in a flat array with a separate array tracking
/// how many samples each pixel has.
///
/// # Memory Layout
///
/// The samples are stored in a flat array, with pixel sample arrays concatenated
/// together. The `sample_counts` array stores how many samples each pixel has.
///
/// ```text
/// Pixel (0,0): 3 samples  -> indices 0..3
/// Pixel (1,0): 0 samples  -> (empty)
/// Pixel (2,0): 5 samples  -> indices 3..8
/// Pixel (3,0): 2 samples  -> indices 8..10
/// ...
/// ```
///
/// # Examples
///
/// ```
/// # #[cfg(feature = "deep-data")]
/// # {
/// use exr::image::deep_samples::DeepSamples;
/// use exr::image::FlatSamples;
/// use exr::math::Vec2;
///
/// // Create storage for a 10x10 image
/// let resolution = Vec2(10, 10);
/// let sample_counts = vec![0, 3, 1, 0, 2, /* ... */]; // Per-pixel counts
/// let samples = FlatSamples::F32(vec![/* sample data */]);
///
/// let deep_samples = DeepSamples::new(resolution, sample_counts, samples);
/// # }
/// ```
#[cfg(feature = "deep-data")]
#[derive(Debug, Clone, PartialEq)]
pub struct DeepSamples {
    /// Number of samples for each pixel (row-major: width × height).
    /// The length of this vector is always `resolution.0 * resolution.1`.
    sample_counts: Vec<u32>,

    /// Actual sample data (concatenated arrays, indexed by cumulative sample_counts).
    /// The total number of samples is the sum of all sample_counts.
    samples: FlatSamples,

    /// Image dimensions (width, height) for indexing.
    resolution: Vec2<usize>,

    /// Cached cumulative offsets for O(1) pixel access.
    /// cumulative_offsets[pixel_index] = sum of all sample counts before this pixel.
    /// Length is `sample_counts.len() + 1`, with the last element being total sample count.
    cumulative_offsets: Vec<usize>,
}

#[cfg(feature = "deep-data")]
impl DeepSamples {
    /// Creates new deep sample storage from sample counts and sample data.
    ///
    /// # Arguments
    ///
    /// * `resolution` - Image dimensions (width, height)
    /// * `sample_counts` - Number of samples for each pixel (must have length width × height)
    /// * `samples` - Flat array of all samples (total length must equal sum of sample_counts)
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `sample_counts` length doesn't match `resolution.0 * resolution.1`
    /// - Total samples in `samples` doesn't match sum of `sample_counts`
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "deep-data")]
    /// # {
    /// use exr::image::deep_samples::DeepSamples;
    /// use exr::image::FlatSamples;
    /// use exr::math::Vec2;
    ///
    /// let resolution = Vec2(2, 2); // 2×2 image
    /// let sample_counts = vec![1, 0, 2, 1]; // 4 pixels with varying sample counts
    /// let samples = FlatSamples::F32(vec![1.0, 2.0, 3.0, 4.0]); // 4 total samples
    ///
    /// let deep_samples = DeepSamples::new(resolution, sample_counts, samples)?;
    /// # Ok::<(), exr::error::Error>(())
    /// # }
    /// ```
    pub fn new(
        resolution: Vec2<usize>,
        sample_counts: Vec<u32>,
        samples: FlatSamples,
    ) -> Result<Self> {
        let pixel_count = resolution.0 * resolution.1;

        // Validate sample counts array size
        if sample_counts.len() != pixel_count {
            return Err(Error::invalid(format!(
                "sample_counts length {} doesn't match resolution {}×{} = {}",
                sample_counts.len(),
                resolution.0,
                resolution.1,
                pixel_count
            )));
        }

        // Calculate total samples and validate against samples array
        let total_samples: u64 = sample_counts.iter().map(|&c| c as u64).sum();
        let total_samples = usize::try_from(total_samples).map_err(|_| {
            Error::invalid(format!("total sample count {} exceeds usize::MAX", total_samples))
        })?;

        let samples_len = samples.len();
        if samples_len != total_samples {
            return Err(Error::invalid(format!(
                "samples length {} doesn't match sum of sample_counts {}",
                samples_len, total_samples
            )));
        }

        // Build cumulative offsets for O(1) access
        let mut cumulative_offsets = Vec::with_capacity(pixel_count + 1);
        cumulative_offsets.push(0);

        let mut offset = 0_usize;
        for &count in &sample_counts {
            offset += count as usize;
            cumulative_offsets.push(offset);
        }

        Ok(Self {
            sample_counts,
            samples,
            resolution,
            cumulative_offsets,
        })
    }

    /// Creates empty deep sample storage (all pixels have 0 samples).
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "deep-data")]
    /// # {
    /// use exr::image::deep_samples::DeepSamples;
    /// use exr::meta::attribute::SampleType;
    /// use exr::math::Vec2;
    ///
    /// let resolution = Vec2(100, 100);
    /// let empty = DeepSamples::empty(resolution, SampleType::F32);
    ///
    /// assert_eq!(empty.get_sample_count(0, 0), 0);
    /// # }
    /// ```
    pub fn empty(resolution: Vec2<usize>, sample_type: crate::meta::attribute::SampleType) -> Self {
        let pixel_count = resolution.0 * resolution.1;
        let sample_counts = vec![0_u32; pixel_count];

        let samples = match sample_type {
            crate::meta::attribute::SampleType::F16 => FlatSamples::F16(Vec::new()),
            crate::meta::attribute::SampleType::F32 => FlatSamples::F32(Vec::new()),
            crate::meta::attribute::SampleType::U32 => FlatSamples::U32(Vec::new()),
        };

        let cumulative_offsets = vec![0_usize; pixel_count + 1];

        Self {
            sample_counts,
            samples,
            resolution,
            cumulative_offsets,
        }
    }

    /// Returns the image resolution (width, height).
    #[inline]
    #[must_use]
    pub fn resolution(&self) -> Vec2<usize> {
        self.resolution
    }

    /// Returns the width of the image.
    #[inline]
    #[must_use]
    pub fn width(&self) -> usize {
        self.resolution.0
    }

    /// Returns the height of the image.
    #[inline]
    #[must_use]
    pub fn height(&self) -> usize {
        self.resolution.1
    }

    /// Returns the total number of pixels in the image.
    #[inline]
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        self.sample_counts.len()
    }

    /// Returns the total number of samples across all pixels.
    #[inline]
    #[must_use]
    pub fn total_sample_count(&self) -> usize {
        *self.cumulative_offsets.last().unwrap_or(&0)
    }

    /// Returns the number of samples for the pixel at (x, y).
    ///
    /// # Panics
    ///
    /// Panics if the pixel coordinates are out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "deep-data")]
    /// # {
    /// # use exr::image::deep_samples::DeepSamples;
    /// # use exr::image::FlatSamples;
    /// # use exr::math::Vec2;
    /// # let resolution = Vec2(2, 2);
    /// # let sample_counts = vec![1, 0, 2, 1];
    /// # let samples = FlatSamples::F32(vec![1.0, 2.0, 3.0, 4.0]);
    /// # let deep_samples = DeepSamples::new(resolution, sample_counts, samples).unwrap();
    /// assert_eq!(deep_samples.get_sample_count(0, 0), 1);
    /// assert_eq!(deep_samples.get_sample_count(1, 0), 0);
    /// assert_eq!(deep_samples.get_sample_count(0, 1), 2);
    /// # }
    /// ```
    #[inline]
    #[must_use]
    pub fn get_sample_count(&self, x: usize, y: usize) -> u32 {
        let index = self.pixel_index(x, y);
        self.sample_counts[index]
    }

    /// Returns a reference to the underlying flat samples.
    #[inline]
    #[must_use]
    pub fn samples(&self) -> &FlatSamples {
        &self.samples
    }

    /// Returns a mutable reference to the underlying flat samples.
    #[inline]
    #[must_use]
    pub fn samples_mut(&mut self) -> &mut FlatSamples {
        &mut self.samples
    }

    /// Returns a reference to the sample counts array.
    #[inline]
    #[must_use]
    pub fn sample_counts(&self) -> &[u32] {
        &self.sample_counts
    }

    /// Converts pixel coordinates to a flat pixel index.
    #[inline]
    fn pixel_index(&self, x: usize, y: usize) -> usize {
        assert!(x < self.resolution.0, "x coordinate {} out of bounds (width: {})", x, self.resolution.0);
        assert!(y < self.resolution.1, "y coordinate {} out of bounds (height: {})", y, self.resolution.1);
        y * self.resolution.0 + x
    }

    /// Returns the range of sample indices for the pixel at (x, y).
    ///
    /// The returned range can be used to index into the flat samples array.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[cfg(feature = "deep-data")]
    /// # {
    /// # use exr::image::deep_samples::DeepSamples;
    /// # use exr::image::FlatSamples;
    /// # use exr::math::Vec2;
    /// # let resolution = Vec2(2, 2);
    /// # let sample_counts = vec![1, 0, 2, 1];
    /// # let samples = FlatSamples::F32(vec![1.0, 2.0, 3.0, 4.0]);
    /// # let deep_samples = DeepSamples::new(resolution, sample_counts, samples).unwrap();
    /// let range = deep_samples.sample_range(0, 0);
    /// assert_eq!(range, 0..1); // First pixel has 1 sample at index 0
    ///
    /// let range = deep_samples.sample_range(0, 1);
    /// assert_eq!(range, 1..3); // Third pixel has 2 samples at indices 1-2
    /// # }
    /// ```
    #[inline]
    #[must_use]
    pub fn sample_range(&self, x: usize, y: usize) -> std::ops::Range<usize> {
        let pixel_index = self.pixel_index(x, y);
        let start = self.cumulative_offsets[pixel_index];
        let end = self.cumulative_offsets[pixel_index + 1];
        start..end
    }

    /// Returns statistics about the deep samples.
    ///
    /// Useful for debugging and understanding the sample distribution.
    #[must_use]
    pub fn statistics(&self) -> DeepSampleStatistics {
        let mut max_samples = 0_u32;
        let mut min_samples = u32::MAX;
        let mut pixels_with_samples = 0_usize;

        for &count in &self.sample_counts {
            max_samples = max_samples.max(count);
            min_samples = min_samples.min(count);
            if count > 0 {
                pixels_with_samples += 1;
            }
        }

        if min_samples == u32::MAX {
            min_samples = 0;
        }

        let average_samples = if self.pixel_count() > 0 {
            self.total_sample_count() as f64 / self.pixel_count() as f64
        } else {
            0.0
        };

        DeepSampleStatistics {
            pixel_count: self.pixel_count(),
            total_samples: self.total_sample_count(),
            min_samples_per_pixel: min_samples,
            max_samples_per_pixel: max_samples,
            average_samples_per_pixel: average_samples,
            pixels_with_samples,
        }
    }

    /// Validates internal consistency.
    ///
    /// Checks that cumulative offsets match sample counts and that
    /// total samples match the samples array length.
    pub fn validate(&self) -> UnitResult {
        // Check sample counts length
        let expected_pixel_count = self.resolution.0 * self.resolution.1;
        if self.sample_counts.len() != expected_pixel_count {
            return Err(Error::invalid(format!(
                "sample_counts length {} doesn't match expected pixel count {}",
                self.sample_counts.len(),
                expected_pixel_count
            )));
        }

        // Check cumulative offsets length
        if self.cumulative_offsets.len() != self.sample_counts.len() + 1 {
            return Err(Error::invalid(format!(
                "cumulative_offsets length {} doesn't match sample_counts.len() + 1 = {}",
                self.cumulative_offsets.len(),
                self.sample_counts.len() + 1
            )));
        }

        // Verify cumulative offsets are correct
        let mut expected_offset = 0_usize;
        for (i, &count) in self.sample_counts.iter().enumerate() {
            if self.cumulative_offsets[i] != expected_offset {
                return Err(Error::invalid(format!(
                    "cumulative_offsets[{}] = {} but expected {}",
                    i, self.cumulative_offsets[i], expected_offset
                )));
            }
            expected_offset += count as usize;
        }

        // Check total samples
        if self.cumulative_offsets[self.sample_counts.len()] != expected_offset {
            return Err(Error::invalid(format!(
                "final cumulative_offset {} doesn't match expected total {}",
                self.cumulative_offsets[self.sample_counts.len()],
                expected_offset
            )));
        }

        // Check samples array length
        if self.samples.len() != expected_offset {
            return Err(Error::invalid(format!(
                "samples length {} doesn't match total sample count {}",
                self.samples.len(),
                expected_offset
            )));
        }

        Ok(())
    }
}

/// Statistics about deep sample distribution.
#[cfg(feature = "deep-data")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeepSampleStatistics {
    /// Total number of pixels in the image.
    pub pixel_count: usize,

    /// Total number of samples across all pixels.
    pub total_samples: usize,

    /// Minimum samples in any single pixel.
    pub min_samples_per_pixel: u32,

    /// Maximum samples in any single pixel.
    pub max_samples_per_pixel: u32,

    /// Average samples per pixel.
    pub average_samples_per_pixel: f64,

    /// Number of pixels that have at least one sample.
    pub pixels_with_samples: usize,
}

#[cfg(all(test, feature = "deep-data"))]
mod tests {
    use super::*;

    #[test]
    fn test_empty_deep_samples() {
        let resolution = Vec2(10, 10);
        let empty = DeepSamples::empty(resolution, crate::meta::attribute::SampleType::F32);

        assert_eq!(empty.width(), 10);
        assert_eq!(empty.height(), 10);
        assert_eq!(empty.pixel_count(), 100);
        assert_eq!(empty.total_sample_count(), 0);
        assert_eq!(empty.get_sample_count(0, 0), 0);
        assert_eq!(empty.get_sample_count(9, 9), 0);

        assert!(empty.validate().is_ok());
    }

    #[test]
    fn test_new_deep_samples() {
        let resolution = Vec2(2, 2);
        let sample_counts = vec![1, 0, 2, 1];
        let samples = FlatSamples::F32(vec![1.0, 2.0, 3.0, 4.0]);

        let deep = DeepSamples::new(resolution, sample_counts, samples).unwrap();

        assert_eq!(deep.pixel_count(), 4);
        assert_eq!(deep.total_sample_count(), 4);
        assert_eq!(deep.get_sample_count(0, 0), 1);
        assert_eq!(deep.get_sample_count(1, 0), 0);
        assert_eq!(deep.get_sample_count(0, 1), 2);
        assert_eq!(deep.get_sample_count(1, 1), 1);

        assert!(deep.validate().is_ok());
    }

    #[test]
    fn test_sample_ranges() {
        let resolution = Vec2(2, 2);
        let sample_counts = vec![1, 0, 2, 1];
        let samples = FlatSamples::F32(vec![1.0, 2.0, 3.0, 4.0]);

        let deep = DeepSamples::new(resolution, sample_counts, samples).unwrap();

        assert_eq!(deep.sample_range(0, 0), 0..1);
        assert_eq!(deep.sample_range(1, 0), 1..1); // Empty range
        assert_eq!(deep.sample_range(0, 1), 1..3);
        assert_eq!(deep.sample_range(1, 1), 3..4);
    }

    #[test]
    fn test_statistics() {
        let resolution = Vec2(3, 2);
        let sample_counts = vec![0, 5, 2, 0, 10, 1];
        let total_samples = 0 + 5 + 2 + 0 + 10 + 1;
        let samples = FlatSamples::F32(vec![0.0; total_samples]);

        let deep = DeepSamples::new(resolution, sample_counts, samples).unwrap();
        let stats = deep.statistics();

        assert_eq!(stats.pixel_count, 6);
        assert_eq!(stats.total_samples, 18);
        assert_eq!(stats.min_samples_per_pixel, 0);
        assert_eq!(stats.max_samples_per_pixel, 10);
        assert_eq!(stats.pixels_with_samples, 4);
        assert!((stats.average_samples_per_pixel - 3.0).abs() < 0.001);
    }

    #[test]
    fn test_validation_errors() {
        let resolution = Vec2(2, 2);

        // Wrong sample_counts length
        let sample_counts = vec![1, 0, 2]; // Should be 4
        let samples = FlatSamples::F32(vec![1.0, 2.0, 3.0]);
        assert!(DeepSamples::new(resolution, sample_counts, samples).is_err());

        // Wrong samples length
        let sample_counts = vec![1, 0, 2, 1];
        let samples = FlatSamples::F32(vec![1.0, 2.0]); // Should be 4
        assert!(DeepSamples::new(resolution, sample_counts, samples).is_err());
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn test_out_of_bounds_access() {
        let resolution = Vec2(2, 2);
        let sample_counts = vec![1, 0, 2, 1];
        let samples = FlatSamples::F32(vec![1.0, 2.0, 3.0, 4.0]);
        let deep = DeepSamples::new(resolution, sample_counts, samples).unwrap();

        let _ = deep.get_sample_count(2, 0); // x out of bounds
    }
}
