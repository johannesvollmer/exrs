//! Data structures for deep images, where each pixel contains any number of
//! samples, for example at different depths.
//!
//! Deep layers use [`AnyDeepChannels`] as their channel data. A deep image with
//! a single layer is a [`DeepImage`](crate::image::DeepImage), and a deep image
//! with multiple layers is a
//! [`DeepLayersImage`](crate::image::DeepLayersImage).
//!
//! Read deep images with `read().deep_data()`, see
//! [`crate::image::read::deep`]. Write deep images with `image.write()`, see
//! [`crate::image::write::deep`].

use std::{convert::TryFrom, ops::Range};

use half::f16;
use smallvec::SmallVec;

use crate::{
    block::{
        samples::{FromNativeSample, Sample},
        BlockIndex, UncompressedDeepBlock,
    },
    error::{Error, Result, UnitResult},
    image::{write::samples::WritableSamples, AnyChannel, FlatSamples},
    io::Data,
    math::Vec2,
    meta::attribute::{ChannelDescription, ChannelList, SampleType},
};

/// The channels of a deep layer.
///
/// Each pixel contains any number of samples, including zero samples.
/// All channels share the same number of samples per pixel.
/// The samples of all pixels are stored in a single flat vector per channel,
/// pixel after pixel, in row-major order. Use [`AnyDeepChannels::sample_range`]
/// to find the samples of a specific pixel inside the flat sample vectors.
///
/// The samples within a pixel may be in any order. Use the
/// `deep_image_state` layer attribute to declare whether the samples are
/// sorted and non-overlapping.
///
/// By convention, deep images contain a `Z` channel, which contains the depth
/// of each sample, and an optional `ZBack` channel, which contains the back
/// depth of volumetric samples. Color channels should be premultiplied by the
/// alpha channel `A`.
#[derive(Clone, PartialEq)]
pub struct AnyDeepChannels {
    resolution: Vec2<usize>,

    /// For each pixel, the index of its first sample, and one more entry
    /// containing the total number of samples.
    sample_offsets: Vec<usize>,

    /// Sorted alphabetically by name. Each channel has one value per sample.
    list: SmallVec<[AnyChannel<FlatSamples>; 4]>,
}

/// Statistics about the distribution of the samples in a deep layer.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeepSampleStatistics {
    /// Total number of pixels in the layer.
    pub pixel_count: usize,

    /// Total number of samples in all pixels.
    pub total_sample_count: usize,

    /// Minimum number of samples in any pixel.
    pub min_samples_per_pixel: usize,

    /// Maximum number of samples in any pixel.
    pub max_samples_per_pixel: usize,

    /// Average number of samples per pixel.
    pub average_samples_per_pixel: f64,

    /// Number of pixels that contain at least one sample.
    pub non_empty_pixel_count: usize,
}

impl AnyDeepChannels {
    /// Create the channels of a deep layer.
    ///
    /// The sample counts contain the number of samples of each pixel, in
    /// row-major order, and must contain exactly `width * height` entries.
    /// Each channel must contain exactly one value for each sample,
    /// with the samples of all pixels concatenated in row-major order.
    /// The channels will be sorted by name.
    /// Each channel must have a sampling rate of `(1, 1)`.
    ///
    /// ```
    /// use exr::{image::deep::AnyDeepChannels, prelude::*};
    ///
    /// // a 2x1 image, the first pixel contains two samples, the second pixel none
    /// let channels = AnyDeepChannels::new(
    ///     (2, 1),
    ///     vec![2, 0],
    ///     vec![
    ///         AnyChannel::new("Z", FlatSamples::F32(vec![1.0, 2.5])),
    ///         AnyChannel::new("A", FlatSamples::F16(vec![f16::ONE, f16::from_f32(0.5)])),
    ///     ],
    /// )?;
    ///
    /// assert_eq!(channels.total_sample_count(), 2);
    /// assert_eq!(channels.sample_range(Vec2(0, 0)), 0..2);
    /// assert_eq!(channels.channels()[0].name, Text::from("A"));
    /// # Ok::<(), exr::error::Error>(())
    /// ```
    pub fn new(
        resolution: impl Into<Vec2<usize>>,
        sample_counts: impl IntoIterator<Item = usize>,
        channels: impl IntoIterator<Item = AnyChannel<FlatSamples>>,
    ) -> Result<Self> {
        let resolution = resolution.into();

        let sample_counts = sample_counts.into_iter();
        let mut sample_offsets = Vec::with_capacity(sample_counts.size_hint().0 + 1);
        sample_offsets.push(0);

        let mut total_sample_count = 0_usize;
        for count in sample_counts {
            total_sample_count = total_sample_count
                .checked_add(count)
                .ok_or_else(|| Error::invalid("deep sample count"))?;

            sample_offsets.push(total_sample_count);
        }

        let mut list: SmallVec<[AnyChannel<FlatSamples>; 4]> = channels.into_iter().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));

        let deep = Self {
            resolution,
            sample_offsets,
            list,
        };

        deep.validate()?;
        Ok(deep)
    }

    /// The width and height of this layer, in pixels.
    #[must_use]
    pub fn resolution(&self) -> Vec2<usize> {
        self.resolution
    }

    /// The number of pixels in this layer.
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        self.resolution.area()
    }

    /// The total number of samples of all pixels.
    /// This is the length of the sample vector of each channel.
    #[must_use]
    pub fn total_sample_count(&self) -> usize {
        self.sample_offsets.last().copied().unwrap_or(0)
    }

    /// The number of samples in the pixel at the specified position.
    /// Panics if the position is outside of the resolution.
    #[must_use]
    pub fn sample_count(&self, position: Vec2<usize>) -> usize {
        self.sample_range(position).len()
    }

    /// The indices of the samples of the pixel at the specified position,
    /// in the sample vector of each channel.
    /// Panics if the position is outside of the resolution.
    #[must_use]
    pub fn sample_range(&self, position: Vec2<usize>) -> Range<usize> {
        assert!(
            position.x() < self.resolution.width() && position.y() < self.resolution.height(),
            "pixel position {:?} is outside of the resolution {:?}",
            position,
            self.resolution
        );

        self.sample_range_by_index(position.flat_index_for_size(self.resolution))
    }

    /// The indices of the samples of the specified pixel, in the sample vector
    /// of each channel. The pixel index is `y * width + x`.
    /// Panics if the index is out of bounds.
    #[must_use]
    pub fn sample_range_by_index(&self, pixel_index: usize) -> Range<usize> {
        self.sample_offsets[pixel_index]..self.sample_offsets[pixel_index + 1]
    }

    /// The number of samples in each pixel, in row-major order.
    #[must_use]
    pub fn sample_counts(&self) -> impl '_ + ExactSizeIterator<Item = usize> {
        self.sample_offsets.windows(2).map(|pair| pair[1] - pair[0])
    }

    /// For each pixel, in row-major order, the index of its first sample,
    /// and one more entry containing the total number of samples.
    #[must_use]
    pub fn sample_offsets(&self) -> &[usize] {
        &self.sample_offsets
    }

    /// The maximum number of samples in a single pixel.
    #[must_use]
    pub fn max_samples_per_pixel(&self) -> usize {
        self.sample_counts().max().unwrap_or(0)
    }

    /// The channels of this layer, sorted by name.
    #[must_use]
    pub fn channels(&self) -> &[AnyChannel<FlatSamples>] {
        &self.list
    }

    /// The channels of this layer, sorted by name.
    /// You may modify the sample values,
    /// but the number of samples in each channel must not change,
    /// and the channels must stay sorted by name.
    pub fn channels_mut(&mut self) -> &mut [AnyChannel<FlatSamples>] {
        &mut self.list
    }

    /// Find the channel with the specified name.
    #[must_use]
    pub fn channel_by_name(&self, name: &str) -> Option<&AnyChannel<FlatSamples>> {
        self.channel_index_by_name(name).map(|index| &self.list[index])
    }

    /// Find the index of the channel with the specified name.
    #[must_use]
    pub fn channel_index_by_name(&self, name: &str) -> Option<usize> {
        self.list.iter().position(|channel| channel.name.eq(name))
    }

    /// The samples of the pixel at the specified position, in the channel
    /// with the specified index. Each sample has the type stored in the
    /// channel. Does not allocate.
    /// Panics if the position is outside of the resolution,
    /// or if the channel index is out of bounds.
    ///
    /// ```
    /// use exr::{image::deep::AnyDeepChannels, prelude::*};
    ///
    /// let channels = AnyDeepChannels::new(
    ///     (1, 1),
    ///     vec![2],
    ///     vec![AnyChannel::new("Z", FlatSamples::F32(vec![1.0, 2.5]))],
    /// )?;
    ///
    /// let depths: Vec<Sample> = channels.pixel_samples(0, Vec2(0, 0)).collect();
    /// assert_eq!(depths, vec![Sample::F32(1.0), Sample::F32(2.5)]);
    /// # Ok::<(), exr::error::Error>(())
    /// ```
    #[must_use]
    pub fn pixel_samples(
        &self,
        channel_index: usize,
        position: Vec2<usize>,
    ) -> impl '_ + ExactSizeIterator<Item = Sample> {
        let samples = &self.list[channel_index].sample_data;
        self.sample_range(position).map(move |index| samples.value_by_flat_index(index))
    }

    /// The samples of the pixel at the specified position, in the channel
    /// with the specified index, converted to the desired type, for example
    /// `f32` or `f16`. Does not allocate.
    /// Panics if the position is outside of the resolution,
    /// or if the channel index is out of bounds.
    ///
    /// ```
    /// use exr::{image::deep::AnyDeepChannels, prelude::*};
    ///
    /// let channels = AnyDeepChannels::new(
    ///     (1, 1),
    ///     vec![2],
    ///     vec![AnyChannel::new("A", FlatSamples::F16(vec![f16::ONE, f16::from_f32(0.5)]))],
    /// )?;
    ///
    /// let alphas: Vec<f32> = channels.pixel_samples_as(0, Vec2(0, 0)).collect();
    /// assert_eq!(alphas, vec![1.0, 0.5]);
    /// # Ok::<(), exr::error::Error>(())
    /// ```
    #[must_use]
    pub fn pixel_samples_as<T: FromNativeSample>(
        &self,
        channel_index: usize,
        position: Vec2<usize>,
    ) -> impl '_ + ExactSizeIterator<Item = T> {
        let samples = &self.list[channel_index].sample_data;
        self.sample_range(position).map(move |index| match samples {
            FlatSamples::F16(values) => T::from_f16(values[index]),
            FlatSamples::F32(values) => T::from_f32(values[index]),
            FlatSamples::U32(values) => T::from_u32(values[index]),
        })
    }

    /// The channel descriptions of this layer, as they would be stored in a
    /// file.
    #[must_use]
    pub fn to_channel_list(&self) -> ChannelList {
        ChannelList::new(
            self.list
                .iter()
                .map(|channel| ChannelDescription {
                    name: channel.name.clone(),
                    sample_type: channel.sample_data.sample_type(),
                    quantize_linearly: channel.quantize_linearly,
                    sampling: channel.sampling,
                })
                .collect(),
        )
    }

    /// Compute statistics about the distribution of the samples.
    #[must_use]
    pub fn statistics(&self) -> DeepSampleStatistics {
        let pixel_count = self.pixel_count();
        let total_sample_count = self.total_sample_count();

        DeepSampleStatistics {
            pixel_count,
            total_sample_count,
            min_samples_per_pixel: self.sample_counts().min().unwrap_or(0),
            max_samples_per_pixel: self.max_samples_per_pixel(),
            non_empty_pixel_count: self.sample_counts().filter(|&count| count > 0).count(),
            average_samples_per_pixel: if pixel_count == 0 {
                0.0
            } else {
                total_sample_count as f64 / pixel_count as f64
            },
        }
    }

    /// Check that the sample counts match the resolution, that each channel
    /// contains one value for each sample, and that channel names are unique.
    pub fn validate(&self) -> UnitResult {
        self.validate_sample_offsets()?;
        self.validate_channels()
    }

    /// Check that the sample counts match the resolution.
    /// Takes time proportional to the number of pixels.
    fn validate_sample_offsets(&self) -> UnitResult {
        let Vec2(width, height) = self.resolution;
        let offset_count = width.checked_mul(height).and_then(|count| count.checked_add(1));

        if offset_count != Some(self.sample_offsets.len()) {
            return Err(Error::invalid(format!(
                "deep layer of resolution {}x{} requires one sample count per pixel, but has {}",
                width,
                height,
                self.sample_offsets.len().saturating_sub(1)
            )));
        }

        if self.sample_offsets.first() != Some(&0)
            || self.sample_offsets.windows(2).any(|pair| pair[1] < pair[0])
        {
            return Err(Error::invalid("deep sample offsets"));
        }

        Ok(())
    }

    /// Check that each channel contains one value for each sample, is not
    /// subsampled, and that channel names are unique. The channels must be
    /// sorted by name, which is checked when validating the header.
    /// Unlike the header validation, this always rejects duplicate names,
    /// as the channels of this layer are always written to a file.
    pub(crate) fn validate_channels(&self) -> UnitResult {
        for channel in &self.list {
            if channel.sample_data.len() != self.total_sample_count() {
                return Err(Error::invalid(format!(
                    "deep channel `{}` contains {} values, but the layer contains {} samples",
                    channel.name,
                    channel.sample_data.len(),
                    self.total_sample_count()
                )));
            }

            if channel.sampling != Vec2(1, 1) {
                return Err(Error::invalid(format!(
                    "deep channel `{}` must not be subsampled",
                    channel.name
                )));
            }
        }

        // the channels are sorted by name, so duplicates are neighbours
        if self.list.windows(2).any(|pair| pair[0].name == pair[1].name) {
            return Err(Error::invalid("duplicate deep channel name"));
        }

        Ok(())
    }

    /// Extract a deep block from this layer.
    /// The channels must have the same order as in the header.
    pub(crate) fn extract_block(&self, index: BlockIndex) -> Result<UncompressedDeepBlock> {
        let Vec2(block_x, block_y) = index.pixel_position;
        let Vec2(block_width, block_height) = index.pixel_size;
        let width = self.resolution.width();

        let bytes_per_sample: usize = self
            .list
            .iter()
            .map(|channel| channel.sample_data.sample_type().bytes_per_sample())
            .sum();

        let mut pixel_offset_table = Vec::with_capacity(index.pixel_size.area());
        let mut block_sample_count = 0_usize;

        for row in 0..block_height {
            let first_pixel = (block_y + row) * width + block_x;

            for pixel in first_pixel..first_pixel + block_width {
                block_sample_count += self.sample_range_by_index(pixel).len();
                pixel_offset_table.push(
                    u32::try_from(block_sample_count)
                        .map_err(|_| Error::invalid("too many deep samples in a single block"))?,
                );
            }
        }

        let mut sample_data = Vec::with_capacity(block_sample_count * bytes_per_sample);

        for row in 0..block_height {
            let first_pixel = (block_y + row) * width + block_x;
            let samples =
                self.sample_offsets[first_pixel]..self.sample_offsets[first_pixel + block_width];

            for channel in &self.list {
                write_samples_to_ne_bytes(&channel.sample_data, samples.clone(), &mut sample_data)?;
            }
        }

        Ok(UncompressedDeepBlock {
            index,
            pixel_offset_table,
            sample_data,
        })
    }
}

impl std::fmt::Debug for AnyDeepChannels {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AnyDeepChannels")
            .field("resolution", &self.resolution)
            .field("total_sample_count", &self.total_sample_count())
            .field("list", &self.list)
            .finish_non_exhaustive()
    }
}

/// Collects the decompressed blocks of the largest resolution level of a deep
/// layer, and then assembles them into the channels of that layer.
///
/// The sample count of each pixel is stored as soon as a block is inserted.
/// The samples themselves are kept inside their blocks until all blocks are
/// known, as the position of the samples in the layer depends on the sample
/// counts of all previous pixels.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DeepBlocksCollector {
    resolution: Vec2<usize>,
    channels: ChannelList,

    /// The number of samples of each pixel, in row-major order.
    sample_counts: Vec<usize>,

    /// The blocks that have been inserted, in any order.
    blocks: Vec<UncompressedDeepBlock>,

    /// Whether a block at a position has already been inserted.
    /// Contains one entry per block, in row-major block order.
    is_block_inserted: Vec<bool>,
    block_size: Vec2<usize>,
}

impl DeepBlocksCollector {
    /// Prepare collecting the blocks of a layer with the specified resolution
    /// and the specified maximum block size. The channels must have the same
    /// order as in the blocks.
    pub fn new(
        resolution: Vec2<usize>,
        block_size: Vec2<usize>,
        channels: ChannelList,
    ) -> Result<Self> {
        let pixel_count = resolution
            .width()
            .checked_mul(resolution.height())
            .ok_or_else(|| Error::invalid("deep layer size"))?;

        let block_count = resolution
            .width()
            .div_ceil(block_size.width().max(1))
            .checked_mul(resolution.height().div_ceil(block_size.height().max(1)))
            .ok_or_else(|| Error::invalid("deep layer size"))?;

        Ok(Self {
            resolution,
            channels,
            sample_counts: try_allocate_zeroed(pixel_count)?,
            blocks: Vec::new(),
            is_block_inserted: try_allocate_zeroed(block_count)?,
            block_size,
        })
    }

    /// Validate the block and store its sample counts.
    /// If a block with the same position was already inserted,
    /// which can happen in a corrupt file, the new block is ignored.
    pub fn insert_block(&mut self, block: UncompressedDeepBlock) -> UnitResult {
        block.validate(&self.channels)?;

        let Vec2(block_x, block_y) = block.index.pixel_position;
        let block_size = block.index.pixel_size;

        if block_x + block_size.width() > self.resolution.width()
            || block_y + block_size.height() > self.resolution.height()
            || block_x % self.block_size.width().max(1) != 0
            || block_y % self.block_size.height().max(1) != 0
        {
            return Err(Error::invalid("deep block position"));
        }

        let blocks_per_row = self.resolution.width().div_ceil(self.block_size.width().max(1));
        let block_index = (block_y / self.block_size.height().max(1)) * blocks_per_row
            + block_x / self.block_size.width().max(1);

        if std::mem::replace(&mut self.is_block_inserted[block_index], true) {
            return Ok(());
        }

        for row in 0..block_size.height() {
            let target_start = (block_y + row) * self.resolution.width() + block_x;
            let targets = &mut self.sample_counts[target_start..target_start + block_size.width()];

            for (column, target) in targets.iter_mut().enumerate() {
                *target = block.sample_count(row * block_size.width() + column);
            }
        }

        self.blocks.push(block);
        Ok(())
    }

    /// Assemble the channels of the layer from all inserted blocks.
    /// Pixels that are not contained in any block contain no samples.
    /// Cannot fail, as the blocks have been validated when inserting them.
    pub fn into_channels(self) -> AnyDeepChannels {
        let Self {
            resolution,
            channels,
            sample_counts,
            blocks,
            ..
        } = self;

        // the sample counts of the same size have already been allocated
        let mut sample_offsets = Vec::with_capacity(sample_counts.len() + 1);
        sample_offsets.push(0);

        let mut total_sample_count = 0_usize;
        for count in sample_counts {
            // cannot overflow, as all samples are already in memory
            total_sample_count += count;
            sample_offsets.push(total_sample_count);
        }

        let mut list: SmallVec<[AnyChannel<FlatSamples>; 4]> = channels
            .list
            .iter()
            .map(|description| AnyChannel {
                name: description.name.clone(),
                quantize_linearly: description.quantize_linearly,
                sampling: description.sampling,
                sample_data: match description.sample_type {
                    SampleType::F16 => FlatSamples::F16(vec![f16::ZERO; total_sample_count]),
                    SampleType::F32 => FlatSamples::F32(vec![0.0; total_sample_count]),
                    SampleType::U32 => FlatSamples::U32(vec![0; total_sample_count]),
                },
            })
            .collect();

        for block in blocks {
            let Vec2(block_x, block_y) = block.index.pixel_position;
            let block_width = block.index.pixel_size.width();
            let mut remaining_bytes = block.sample_data.as_slice();

            for row in 0..block.index.pixel_size.height() {
                let first_pixel = (block_y + row) * resolution.width() + block_x;
                let targets =
                    sample_offsets[first_pixel]..sample_offsets[first_pixel + block_width];

                // the samples of one channel of a row in the block are also
                // contiguous in the layer
                for channel in &mut list {
                    read_samples_from_ne_bytes(
                        &mut channel.sample_data,
                        targets.clone(),
                        &mut remaining_bytes,
                    )
                    .expect("deep block validation bug");
                }
            }

            debug_assert!(remaining_bytes.is_empty(), "not all deep samples were read");
        }

        let deep = AnyDeepChannels {
            resolution,
            sample_offsets,
            list,
        };

        debug_assert!(deep.validate().is_ok(), "deep block validation bug");
        deep
    }
}

/// Allocate a vector of zeroes, returning an error instead of aborting
/// if the size, which may be read from a file, is too large.
fn try_allocate_zeroed<T: Default + Clone>(len: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(len).map_err(|_| Error::invalid("deep layer too large for memory"))?;
    values.resize(len, T::default());
    Ok(values)
}

/// Overwrite the specified samples with the native-endian values from the
/// bytes, and advance the bytes.
fn read_samples_from_ne_bytes(
    samples: &mut FlatSamples,
    targets: Range<usize>,
    bytes: &mut &[u8],
) -> UnitResult {
    match samples {
        FlatSamples::F16(values) => f16::read_slice_ne(bytes, &mut values[targets]),
        FlatSamples::F32(values) => f32::read_slice_ne(bytes, &mut values[targets]),
        FlatSamples::U32(values) => u32::read_slice_ne(bytes, &mut values[targets]),
    }
}

/// Append the specified samples to the bytes, in native-endian format.
fn write_samples_to_ne_bytes(
    samples: &FlatSamples,
    sources: Range<usize>,
    bytes: &mut Vec<u8>,
) -> UnitResult {
    match samples {
        FlatSamples::F16(values) => f16::write_slice_ne(bytes, &values[sources]),
        FlatSamples::F32(values) => f32::write_slice_ne(bytes, &values[sources]),
        FlatSamples::U32(values) => u32::write_slice_ne(bytes, &values[sources]),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::meta::attribute::Text;

    fn test_channels() -> AnyDeepChannels {
        AnyDeepChannels::new(
            (3, 2),
            vec![1, 0, 2, 3, 1, 0],
            vec![
                AnyChannel::new("Z", FlatSamples::F32(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0])),
                AnyChannel::new("A", FlatSamples::F16(vec![f16::ONE; 7])),
                AnyChannel::new("id", FlatSamples::U32(vec![1, 2, 3, 4, 5, 6, 7])),
            ],
        )
        .unwrap()
    }

    #[test]
    fn sample_access() {
        let deep = test_channels();

        assert_eq!(deep.pixel_count(), 6);
        assert_eq!(deep.total_sample_count(), 7);
        assert_eq!(deep.max_samples_per_pixel(), 3);
        assert_eq!(deep.sample_counts().collect::<Vec<_>>(), vec![1, 0, 2, 3, 1, 0]);
        assert_eq!(deep.sample_range(Vec2(0, 0)), 0..1);
        assert_eq!(deep.sample_range(Vec2(1, 0)), 1..1);
        assert_eq!(deep.sample_range(Vec2(2, 0)), 1..3);
        assert_eq!(deep.sample_range(Vec2(0, 1)), 3..6);
        assert_eq!(deep.sample_count(Vec2(1, 1)), 1);

        let names: Vec<Text> = deep.channels().iter().map(|channel| channel.name.clone()).collect();
        assert_eq!(names, vec![Text::from("A"), Text::from("Z"), Text::from("id")]);
        assert_eq!(deep.channel_index_by_name("Z"), Some(1));
        assert!(deep.channel_by_name("R").is_none());

        let statistics = deep.statistics();
        assert_eq!(statistics.non_empty_pixel_count, 4);
        assert_eq!(statistics.min_samples_per_pixel, 0);
        assert_eq!(statistics.max_samples_per_pixel, 3);
    }

    #[test]
    fn reject_invalid_channels() {
        let missing_counts = AnyDeepChannels::new(
            (2, 2),
            vec![1, 1, 1],
            vec![AnyChannel::new("Z", FlatSamples::F32(vec![0.0; 3]))],
        );
        assert!(missing_counts.is_err());

        let missing_samples = AnyDeepChannels::new(
            (2, 1),
            vec![1, 2],
            vec![AnyChannel::new("Z", FlatSamples::F32(vec![0.0; 2]))],
        );
        assert!(missing_samples.is_err());

        let duplicate_names = AnyDeepChannels::new(
            (1, 1),
            vec![1],
            vec![
                AnyChannel::new("Z", FlatSamples::F32(vec![0.0])),
                AnyChannel::new("Z", FlatSamples::F16(vec![f16::ZERO])),
            ],
        );
        assert!(duplicate_names.is_err());
    }

    #[test]
    fn blocks_round_trip() {
        let deep = test_channels();
        let channels = deep.to_channel_list();

        let block_indices = [(Vec2(0, 0), Vec2(2, 2)), (Vec2(2, 0), Vec2(1, 2))];

        let blocks: Vec<UncompressedDeepBlock> = block_indices
            .iter()
            .map(|&(pixel_position, pixel_size)| {
                deep.extract_block(BlockIndex {
                    layer: 0,
                    pixel_position,
                    pixel_size,
                    level: Vec2(0, 0),
                })
                .unwrap()
            })
            .collect();

        for block in &blocks {
            block.validate(&channels).unwrap();
        }

        assert_eq!(blocks[0].pixel_offset_table, vec![1, 1, 4, 5]);
        assert_eq!(blocks[1].pixel_offset_table, vec![2, 2]);

        let mut collector =
            DeepBlocksCollector::new(deep.resolution(), Vec2(2, 2), channels).unwrap();

        // insert the blocks in reverse order, and one block twice
        collector.insert_block(blocks[1].clone()).unwrap();
        collector.insert_block(blocks[0].clone()).unwrap();
        collector.insert_block(blocks[1].clone()).unwrap();

        assert_eq!(collector.into_channels(), deep);
    }
}
