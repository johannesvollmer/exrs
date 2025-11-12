//! Extract lines from a block of pixel bytes.

use crate::block::BlockIndex;
use crate::error::{Result, UnitResult};
use crate::math::*;
use crate::meta::attribute::ChannelList;
use smallvec::SmallVec;
use std::io::Cursor;
use std::ops::Range;

/// A single line of pixels.
/// Use [LineRef] or [LineRefMut] for easier type names.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct LineSlice<T> {
    // TODO also store enum SampleType, as it would always be matched in every place it is used
    /// Where this line is located inside the image.
    pub location: LineIndex,

    /// The raw bytes of the pixel line, either `&[u8]` or `&mut [u8]`.
    /// Must be re-interpreted as slice of f16, f32, or u32,
    /// according to the channel data type.
    pub value: T,
}

/// An reference to a single line of pixels.
/// May go across the whole image or just a tile section of it.
///
/// This line contains an immutable slice that all samples will be read from.
pub type LineRef<'s> = LineSlice<&'s [u8]>;

/// A reference to a single mutable line of pixels.
/// May go across the whole image or just a tile section of it.
///
/// This line contains a mutable slice that all samples will be written to.
pub type LineRefMut<'s> = LineSlice<&'s mut [u8]>;

/// Specifies where a row of pixels lies inside an image.
/// This is a globally unique identifier which includes
/// the layer, channel index, and pixel location.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash)]
pub struct LineIndex {
    /// Index of the layer.
    pub layer: usize,

    /// The channel index of the layer.
    pub channel: usize,

    /// Index of the mip or rip level in the image.
    pub level: Vec2<usize>,

    /// Position of the most left pixel of the row.
    pub position: Vec2<usize>,

    /// The width of the line; the number of samples in this row,
    /// that is, the number of f16, f32, or u32 values.
    pub sample_count: usize,
}

impl LineIndex {
    /// Iterates the lines of this block index in interleaved fashion:
    /// For each line in this block, this iterator steps once through each channel that has samples on that line.
    /// This is how lines are stored in a pixel data block.
    ///
    /// Properly handles channel subsampling: channels with ySampling > 1 may not have samples
    /// on every scanline, and channels with xSampling > 1 have fewer samples per line.
    ///
    /// Does not check whether `self.layer_index`, `self.level`, `self.size` and `self.position` are valid indices.
    // TODO be sure this cannot produce incorrect data, as this is not further checked but only handled with panics
    #[inline]
    #[must_use]
    pub fn lines_in_block(
        block: BlockIndex,
        channels: &ChannelList,
    ) -> impl Iterator<Item = (Range<usize>, LineIndex)> {
        use crate::math::num_samples;

        struct LineIter {
            channels: SmallVec<[ChannelInfo; 8]>,
            layer: usize,
            level: Vec2<usize>,
            x_min: i32,
            x_max: i32,
            y: i32,
            end_y: i32,
            byte: usize,
            channel: usize,
        }

        #[derive(Clone, Copy)]
        struct ChannelInfo {
            x_sampling: usize,
            y_sampling: usize,
            bytes_per_sample: usize,
        }

        impl Iterator for LineIter {
            type Item = (Range<usize>, LineIndex);

            fn next(&mut self) -> Option<Self::Item> {
                loop {
                    // If we've processed all Y coordinates, we're done
                    if self.y >= self.end_y {
                        return None;
                    }

                    // Find the next channel that has samples at the current Y coordinate
                    while self.channel < self.channels.len() {
                        let channel_info = self.channels[self.channel];

                        // Check if this channel has samples at this Y coordinate
                        use crate::math::mod_p;
                        if mod_p(self.y, channel_info.y_sampling) == 0 {
                            // Calculate the number of samples in this scanline for this channel
                            let sample_count =
                                num_samples(channel_info.x_sampling, self.x_min, self.x_max);

                            let byte_len = sample_count * channel_info.bytes_per_sample;

                            let return_value = (
                                (self.byte..self.byte + byte_len),
                                LineIndex {
                                    channel: self.channel,
                                    layer: self.layer,
                                    level: self.level,
                                    position: Vec2(self.x_min as usize, self.y as usize),
                                    sample_count,
                                },
                            );

                            // Increment indices
                            self.byte += byte_len;
                            self.channel += 1;

                            return Some(return_value);
                        }

                        // This channel doesn't have samples at this Y, try next channel
                        self.channel += 1;
                    }

                    // We've processed all channels for this Y coordinate, move to next Y
                    self.channel = 0;
                    self.y += 1;
                }
            }
        }

        let channel_infos: SmallVec<[ChannelInfo; 8]> = channels
            .list
            .iter()
            .map(|channel| ChannelInfo {
                x_sampling: channel.sampling.x(),
                y_sampling: channel.sampling.y(),
                bytes_per_sample: channel.sample_type.bytes_per_sample(),
            })
            .collect();

        let x_min = block.pixel_position.x() as i32;
        let x_max = x_min + block.pixel_size.width() as i32 - 1;

        LineIter {
            channels: channel_infos,
            layer: block.layer,
            level: block.level,
            x_min,
            x_max,
            y: block.pixel_position.y() as i32,
            end_y: (block.pixel_position.y() + block.pixel_size.height()) as i32,
            byte: 0,
            channel: 0,
        }
    }
}

impl<'s> LineRefMut<'s> {
    /// Writes the samples (f16, f32, u32 values) into this line value reference.
    /// Use `write_samples` if there is no slice available.
    #[inline]
    #[must_use]
    pub fn write_samples_from_slice<T: crate::io::Data>(self, slice: &[T]) -> UnitResult {
        debug_assert_eq!(
            slice.len(),
            self.location.sample_count,
            "slice size does not match the line width"
        );
        debug_assert_eq!(
            self.value.len(),
            self.location.sample_count * T::BYTE_SIZE,
            "sample type size does not match line byte size"
        );

        T::write_slice_ne(&mut Cursor::new(self.value), slice)
    }

    /// Iterate over all samples in this line, from left to right.
    /// The supplied `get_line` function returns the sample value
    /// for a given sample index within the line,
    /// which starts at zero for each individual line.
    /// Use `write_samples_from_slice` if you already have a slice of samples.
    #[inline]
    #[must_use]
    pub fn write_samples<T: crate::io::Data>(
        self,
        mut get_sample: impl FnMut(usize) -> T,
    ) -> UnitResult {
        debug_assert_eq!(
            self.value.len(),
            self.location.sample_count * T::BYTE_SIZE,
            "sample type size does not match line byte size"
        );

        let mut write = Cursor::new(self.value);

        for index in 0..self.location.sample_count {
            T::write_ne(get_sample(index), &mut write)?;
        }

        Ok(())
    }
}

impl LineRef<'_> {
    /// Read the samples (f16, f32, u32 values) from this line value reference.
    /// Use `read_samples` if there is not slice available.
    pub fn read_samples_into_slice<T: crate::io::Data>(self, slice: &mut [T]) -> UnitResult {
        debug_assert_eq!(
            slice.len(),
            self.location.sample_count,
            "slice size does not match the line width"
        );
        debug_assert_eq!(
            self.value.len(),
            self.location.sample_count * T::BYTE_SIZE,
            "sample type size does not match line byte size"
        );

        T::read_slice_ne(&mut Cursor::new(self.value), slice)
    }

    /// Iterate over all samples in this line, from left to right.
    /// Use `read_sample_into_slice` if you already have a slice of samples.
    pub fn read_samples<T: crate::io::Data>(&self) -> impl Iterator<Item = Result<T>> + '_ {
        debug_assert_eq!(
            self.value.len(),
            self.location.sample_count * T::BYTE_SIZE,
            "sample type size does not match line byte size"
        );

        let mut read = self.value; // FIXME deep data
        (0..self.location.sample_count).map(move |_| T::read_ne(&mut read))
    }
}
