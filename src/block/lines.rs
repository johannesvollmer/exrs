//! Extract lines from a block of pixel bytes.

use crate::math::*;
use std::io::{Cursor};
use crate::error::{Result, UnitResult, usize_to_i32};
use std::ops::Range;
use crate::block::{BlockIndex};
use crate::meta::attribute::ChannelList;


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
/// This is a unique identifier within one image.
/// Itincludes the layer, channel index, and pixel location.
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

    /// The width of the line; the number of
    /// samples in this row if there was no subsampling.
    pub full_sample_count: usize,

}

#[derive(Clone, Eq, PartialEq, Debug, Hash)]
enum LineSampleBytes {
    Skipped,

    Sampled {
        // pub sub_samples: LineSampleCount,
        bytes_in_block: Range<usize>,

        /// The subsampled width of the line; the true number of samples in this row,
        /// that is, the number of f16, f32, or u32 values, but may be less than expected because of subsampling.
        sub_sample_count: usize,
    }
}

impl LineIndex {

    /*pub fn for_lines_in_block(block: Block, channels: &ChannelList) {

    }*/

    /// Iterates the lines of this block index in interleaved fashion:
    /// For each line in this block, this iterator steps once through each channel.
    /// This is how lines are stored in a pixel data block.
    /// Respects subsampling.
    ///
    /// Does not check whether `self.layer_index`, `self.level`, `self.size` and `self.position` are valid indices.__
    // TODO be sure this cannot produce incorrect data, as this is not further checked but only handled with panics
    // TODO make this an internally iterated closure function instead of iter? to avoid allocation and simplify logic?
    #[inline]
    #[must_use]
    pub fn byte_lines_in_block(block: BlockIndex, channels: &ChannelList) -> impl '_ + Iterator<Item=(LineSampleBytes, LineIndex)> {
        Self::full_lines_in_block(block, channels)
            .scan(0_usize, move |byte, line| {
                let channel = &channels.list[line.channel];
                let width = line.full_sample_count;

                let skip_line = !subsampled_image_contains_line(
                    usize_to_i32(channel.sampling.y()),
                    usize_to_i32(line.position.y())
                );

                let samples = {
                    if skip_line { LineSampleBytes::Skipped }
                    else {
                        let byte_len = channel.subsampled_line_bytes(width);
                        let sub_sample_count = channel.subsampled_line_pixels(width);
                        let bytes_in_block = *byte .. *byte + byte_len;

                        *byte += byte_len; // important: only advance the byte reader when the line is not skipped

                        LineSampleBytes::Sampled {
                            sub_sample_count,
                            bytes_in_block
                        }
                    }
                };

                Some((samples, line))
            })
    }

    /// Iterates the lines of this block index in interleaved fashion:
    /// For each line in this block, this iterator steps once through each channel.
    /// This is how lines are stored in a pixel data block.
    /// Respects subsampling.
    ///
    /// Does not check whether `self.layer_index`, `self.level`, `self.size` and `self.position` are valid indices.__
    // TODO be sure this cannot produce incorrect data, as this is not further checked but only handled with panics
    // TODO make this an internally iterated closure function instead of iter? to avoid allocation and simplify logic?
    #[inline]
    #[must_use]
    pub fn full_lines_in_block(block: BlockIndex, channels: &ChannelList) -> impl '_ + Iterator<Item=LineIndex> {
        let channels = channels.list.as_slice();
        let (width, height) = block.pixel_size.into();
        let start_y = block.pixel_position.y();
        let x = block.pixel_position.x();

        return (start_y .. start_y + height)
            .flat_map(move |absolute_y| {
                channels.iter().enumerate()
                    .map(move |(chan_index, chan)| {
                        LineIndex {
                            layer: block.layer,
                            level: block.level,
                            channel: chan_index,
                            position: Vec2(x, absolute_y),
                            full_sample_count: width,
                        }
                    })
            })
    }
}



impl<'s> LineRefMut<'s> {

    /// Writes the samples (f16, f32, u32 values) into this line value reference.
    /// Use `write_samples` if there is not slice available.
    #[inline]
    #[must_use]
    pub fn write_samples_from_slice<T: crate::io::Data>(self, slice: &[T]) -> UnitResult {
        debug_assert_eq!(slice.len(), self.location.sample_count, "slice size does not match the line width");
        debug_assert_eq!(self.value.len(), self.location.sample_count * T::BYTE_SIZE, "sample type size does not match line byte size");

        T::write_slice(&mut Cursor::new(self.value), slice)
    }

    /// Iterate over all samples in this line, from left to right.
    /// The supplied `get_line` function returns the sample value
    /// for a given sample index within the line,
    /// which starts at zero for each individual line.
    /// Use `write_samples_from_slice` if you already have a slice of samples.
    #[inline]
    #[must_use]
    pub fn write_samples<T: crate::io::Data>(self, mut get_sample: impl FnMut(usize) -> T) -> UnitResult {
        debug_assert_eq!(self.value.len(), self.location.sample_count * T::BYTE_SIZE, "sample type size does not match line byte size");

        let mut write = Cursor::new(self.value);

        for index in 0..self.location.sample_count {
            T::write(get_sample(index), &mut write)?;
        }

        Ok(())
    }
}

impl LineRef<'_> {

    /// Read the samples (f16, f32, u32 values) from this line value reference.
    /// Use `read_samples` if there is not slice available.
    pub fn read_samples_into_slice<T: crate::io::Data>(self, slice: &mut [T]) -> UnitResult {
        debug_assert_eq!(slice.len(), self.location.sample_count, "slice size does not match the line width");
        debug_assert_eq!(self.value.len(), self.location.sample_count * T::BYTE_SIZE, "sample type size does not match line byte size");

        T::read_slice(&mut Cursor::new(self.value), slice)
    }

    /// Iterate over all samples in this line, from left to right.
    /// Use `read_sample_into_slice` if you already have a slice of samples.
    pub fn read_samples<T: crate::io::Data>(&self) -> impl Iterator<Item = Result<T>> + '_ {
        debug_assert_eq!(self.value.len(), self.location.sample_count * T::BYTE_SIZE, "sample type size does not match line byte size");

        let mut read = self.value.clone(); // FIXME deep data
        (0..self.location.sample_count).map(move |_| T::read(&mut read))
    }
}