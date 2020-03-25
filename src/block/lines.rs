//! Extract lines from a block of pixel bytes.

use crate::meta::attributes::*;
use crate::compression::{Compression};
use crate::math::*;
use std::io::{Read, Seek, Write, Cursor};
use crate::error::{Result, Error, UnitResult};
use crate::meta::{MetaData, Header, TileIndices};
use crate::io::{Tracking};
use crate::io::Data;
use smallvec::SmallVec;
use std::ops::Range;
use crate::block::{BlockIndex, UncompressedBlock};
use crate::image::{WriteOptions, OnWriteProgress, OnReadProgress, ReadOptions, read_all_blocks_from_buffered, read_filtered_blocks_from_buffered, for_compressed_blocks_in_image};

/// A single line of pixels.
/// Use `LineRef` or `LineRefMut` for easier type names.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct LineSlice<T> {

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


/// Reads and decompresses all chunks of a file sequentially without seeking.
/// Will not skip any parts of the file. Does not buffer the reader, you should always pass a `BufReader`.
#[inline]
#[must_use]
pub fn read_all_lines_from_buffered<T>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, &[Header], LineRef<'_>) -> UnitResult,
    options: ReadOptions<impl OnReadProgress>,
) -> Result<T>
{
    let insert = |value: &mut T, headers: &[Header], decompressed: UncompressedBlock| {
        let header = headers.get(decompressed.index.layer)
            .ok_or(Error::invalid("chunk index"))?;

        for (bytes, line) in LineIndex::lines_in_block(decompressed.index, header) {
            insert(value, headers, LineSlice { location: line, value: &decompressed.data[bytes] })?; // allows returning `Error::Abort`
        }

        Ok(())
    };

    read_all_blocks_from_buffered(read, new, insert, options)
}

/// Reads and decompresses all desired chunks of a file sequentially, possibly seeking.
/// Will skip any parts of the file that do not match the specified filter condition.
/// Will never seek if the filter condition matches all chunks.
/// Does not buffer the reader, you should always pass a `BufReader`.
#[inline]
#[must_use]
pub fn read_filtered_lines_from_buffered<T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    new: impl Fn(&[Header]) -> Result<T>, // TODO put these into a trait?
    filter: impl Fn(&T, &Header, &TileIndices) -> bool,
    mut insert: impl FnMut(&mut T, &[Header], LineRef<'_>) -> UnitResult,
    options: ReadOptions<impl OnReadProgress>,
) -> Result<T>
{
    let insert = |value: &mut T, headers: &[Header], decompressed: UncompressedBlock| {
        let header = headers.get(decompressed.index.layer)
            .ok_or(Error::invalid("chunk index"))?;

        for (bytes, line) in LineIndex::lines_in_block(decompressed.index, header) {
            insert(value, headers, LineSlice { location: line, value: &decompressed.data[bytes] })?; // allows returning `Error::Abort`
        }

        Ok(())
    };

    read_filtered_blocks_from_buffered(read, new, filter, insert, options)
}



/// Compresses and writes all lines of an image described by `meta_data` and `get_line` to the writer.
/// Flushes the writer to explicitly handle all errors.
///
/// Attention: Currently, using multi-core compression with `LineOrder::Increasing` or `LineOrder::Decreasing` in any header
/// can potentially allocate large amounts of memory while writing the file. Use unspecified line order for lower memory usage.
///
/// Does not buffer the writer, you should always pass a `BufWriter`.
/// If pedantic, throws errors for files that may produce errors in other exr readers.
#[inline]
#[must_use]
pub fn write_all_lines_to_buffered(
    write: impl Write + Seek, meta_data: MetaData,
    get_line: impl Sync + Fn(&[Header], LineRefMut<'_>), // TODO put these three parameters into a trait?  // TODO why is this sync or send????
    options: WriteOptions<impl OnWriteProgress>,
) -> UnitResult
{
    let get_block = |headers: &[Header], block_index: BlockIndex| {
        let header: &Header = &headers.get(block_index.layer).expect("invalid block index");

        let bytes = block_index.pixel_size.area() * header.channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; bytes];

        for (byte_range, line_index) in LineIndex::lines_in_block(block_index, header) {
            get_line(headers, LineRefMut {
                value: &mut block_bytes[byte_range],
                location: line_index,
            });
        }

        block_bytes
    };

    write_all_tiles_to_buffered(write, meta_data, get_block, options)
}

/// Compresses and writes all lines of an image described by `meta_data` and `get_line` to the writer.
/// Flushes the writer to explicitly handle all errors.
///
/// Attention: Currently, using multi-core compression with `LineOrder::Increasing` or `LineOrder::Decreasing` in any header
/// can potentially allocate large amounts of memory while writing the file. Use unspecified line order for lower memory usage.
///
/// Does not buffer the writer, you should always pass a `BufWriter`.
/// If pedantic, throws errors for files that may produce errors in other exr readers.
#[inline]
#[must_use]
pub fn write_all_tiles_to_buffered(
    write: impl Write + Seek,
    mut meta_data: MetaData,
    get_tile: impl Sync + Fn(&[Header], BlockIndex) -> Vec<u8>, // TODO put these three parameters into a trait?  // TODO why is this sync or send????
    mut options: WriteOptions<impl OnWriteProgress>,
) -> UnitResult
{
    let has_compression = meta_data.headers.iter() // TODO cache this in MetaData.has_compression?
        .any(|header| header.compression != Compression::Uncompressed);

    // if non-parallel compression, we always use increasing order anyways
    if !options.parallel_compression || !has_compression {
        for header in &mut meta_data.headers {
            if header.line_order == LineOrder::Unspecified {
                header.line_order = LineOrder::Increasing;
            }
        }
    }

    let mut write = Tracking::new(write);
    meta_data.write_validating_to_buffered(&mut write, options.pedantic)?; // also validates meta data

    let offset_table_start_byte = write.byte_position();

    // skip offset tables for now
    let offset_table_size: usize = meta_data.headers.iter()
        .map(|header| header.chunk_count).sum();

    write.seek_write_to(write.byte_position() + offset_table_size * std::mem::size_of::<u64>())?;

    let mut offset_tables: Vec<Vec<u64>> = meta_data.headers.iter()
        .map(|header| vec![0; header.chunk_count]).collect();

    let total_chunk_count = offset_table_size as f32;
    let mut processed_chunk_count = 0; // very simple on_progress feedback

    // line order is respected in here
    for_compressed_blocks_in_image(&meta_data, get_tile, options.parallel_compression, |chunk_index, chunk|{
        offset_tables[chunk.layer_index][chunk_index] = write.byte_position() as u64; // safe indices from `enumerate()`
        chunk.write(&mut write, meta_data.headers.as_slice())?;

        options.on_progress.on_write_progressed(
            processed_chunk_count as f32 / total_chunk_count, write.byte_position()
        )?;

        processed_chunk_count += 1;
        Ok(())
    })?;

    // write all offset tables
    write.seek_write_to(offset_table_start_byte)?;

    for offset_table in offset_tables {
        u64::write_slice(&mut write, offset_table.as_slice())?;
    }

    write.flush()?; // make sure we catch all (possibly delayed) io errors before returning

    Ok(())
}


impl LineIndex {

    /// Iterates the lines of this block index in interleaved fashion:
    /// For each line in this block, this iterator steps once through each channel.
    /// This is how lines are stored in a pixel data block.
    ///
    /// Does not check whether `self.layer_index`, `self.level`, `self.size` and `self.position` are valid indices.__
    // TODO be sure this cannot produce incorrect data, as this is not further checked but only handled with panics
    #[inline]
    #[must_use]
    pub fn lines_in_block(block: BlockIndex, header: &Header) -> impl Iterator<Item=(Range<usize>, LineIndex)> {
        struct LineIter {
            layer: usize, level: Vec2<usize>, width: usize,
            end_y: usize, x: usize, channel_sizes: SmallVec<[usize; 8]>,
            byte: usize, channel: usize, y: usize,
        };

        // FIXME what about sub sampling??

        impl Iterator for LineIter {
            type Item = (Range<usize>, LineIndex);

            fn next(&mut self) -> Option<Self::Item> {
                if self.y < self.end_y {

                    // compute return value before incrementing
                    let byte_len = self.channel_sizes[self.channel];
                    let return_value = (
                        (self.byte .. self.byte + byte_len),
                        LineIndex {
                            channel: self.channel,
                            layer: self.layer,
                            level: self.level,
                            position: Vec2(self.x, self.y),
                            sample_count: self.width,
                        }
                    );

                    { // increment indices
                        self.byte += byte_len;
                        self.channel += 1;

                        if self.channel == self.channel_sizes.len() {
                            self.channel = 0;
                            self.y += 1;
                        }
                    }

                    Some(return_value)
                }

                else {
                    None
                }
            }
        }

        let channel_line_sizes: SmallVec<[usize; 8]> = header.channels.list.iter()
            .map(move |channel| block.pixel_size.0 * channel.sample_type.bytes_per_sample()) // FIXME is it fewer samples per tile or just fewer tiles for sampled images???
            .collect();

        LineIter {
            layer: block.layer,
            level: block.level,
            width: block.pixel_size.0,
            x: block.pixel_position.0,
            end_y: block.pixel_position.1 + block.pixel_size.1,
            channel_sizes: channel_line_sizes,

            byte: 0,
            channel: 0,
            y: block.pixel_position.1
        }
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