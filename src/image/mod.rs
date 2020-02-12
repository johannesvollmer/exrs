
//! Read and write an exr image.
//! Use `exr::image::simple` or `exr::image::full` for actually reading a complete image.

pub mod full;
pub mod simple;

use crate::meta::attributes::*;
use crate::compression::{Compression, ByteVec};
use crate::math::*;
use std::io::{Read, Seek, Write};
use crate::error::{Result, Error, PassiveResult, i32_to_usize};
use crate::meta::{MetaData, Header, TileIndices, Blocks};
use crate::chunks::{Chunk, Block, TileBlock, ScanLineBlock};
use crate::io::{PeekRead, Tracking};
use rayon::iter::{ParallelIterator, ParallelBridge};
use std::convert::TryFrom;
use crate::io::Data;
use smallvec::SmallVec;
use std::ops::Range;


/// Specifies where a block of pixel data should be placed in the actual image.
/// This is a globally unique identifier which
/// includes the image part, level index, and pixel location.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub struct BlockIndex {

    /// Index of the image part.
    pub part: usize,

    /// Pixel position of the bottom left corner of the block.
    pub position: Vec2<usize>,

    /// Pixel size of the block.
    pub size: Vec2<usize>,

    /// Index of the mip or rip level in the image.
    pub level: Vec2<usize>,
}

/// Contains a block of pixel data and where that data should be placed in the actual image.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UncompressedBlock {

    /// Location of the data inside the image.
    pub index: BlockIndex,

    /// Uncompressed pixel values of the whole block.
    /// One or more scan lines may be stored together as a scan line block.
    /// This byte vector contains all pixel rows, one after another.
    /// For each line in the tile, for each channel, the row values are contiguous.
    pub data: ByteVec,
}

/// A single line of pixels.
/// May go across the whole image or just a tile section of it.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Line<'s> {

    /// Where this line is located inside the image.
    pub location: LineIndex,

    /// The raw bytes of the pixel line.
    /// Must be re-interpreted as slice of f16, f32, or u32,
    /// according to the channel data type.
    pub value: &'s [u8],
}

/// Specifies where a row of pixels lies inside an image.
/// This is a globally unique identifier which
/// includes the image part, channel index, and pixel location.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash)]
pub struct LineIndex {

    /// Index of the image part.
    pub part: usize,

    /// The channel index of the image part.
    pub channel: usize,

    /// Index of the mip or rip level in the image.
    pub level: Vec2<usize>,

    /// Position of the most left pixel of the row.
    pub position: Vec2<usize>,

    /// Number of samples in this row, that is, the number of f16, f32, or u32 values.
    pub width: usize,
}

impl<'s> Line<'s> {

    /// Read the values of this line into a slice of samples.
    /// Panics if the slice is not as long as `self.location.width`.
    pub fn read_samples<T: crate::io::Data>(&self, slice: &mut [T]) -> PassiveResult {
        debug_assert_eq!(slice.len(), self.location.width);
        T::read_slice(&mut self.value.clone(), slice)
    }
}

impl LineIndex {

    /// Writes the samples (f16, f32, u32 values) into the writer.
    pub fn write_samples<T: crate::io::Data>(slice: &[T], write: &mut impl Write) -> PassiveResult {
        T::write_slice(write, slice)?;
        Ok(())
    }
}


/// Reads and decompresses all chunks of a file sequentially without seeking.
/// Will not skip any parts of the file.
pub fn read_all_lines<T>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    parallel: bool,
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, Line<'_>) -> PassiveResult
) -> Result<T>
{
    let (meta_data, mut read_chunk) = self::read_all_compressed_chunks(read)?;
    let meta_data_ref = &meta_data;

    let read_chunks = std::iter::from_fn(move || read_chunk(meta_data_ref));
    let mut result = new(meta_data.headers.as_slice())?;

    for_lines_in_chunks(
        read_chunks, &meta_data, parallel,
        |line| insert(&mut result, line)
    )?;

    Ok(result)
}


/// Reads ad decompresses all desired chunks of a file sequentially, possibly seeking.
/// Will skip any parts of the file that do not match the specified filter condition.
/// Will never seek if the filter condition matches all chunks.
pub fn read_filtered_lines<T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    parallel: bool,
    filter: impl Fn(&Header, &TileIndices) -> bool,
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, Line<'_>) -> PassiveResult
) -> Result<T>
{
    let (meta_data, mut read_chunk) = self::read_filtered_chunks(read, filter)?;
    let read_chunks = std::iter::from_fn(|| read_chunk(&meta_data));
    let mut value = new(meta_data.headers.as_slice())?;

    for_lines_in_chunks(
        read_chunks, &meta_data, parallel,
        |line| insert(&mut value, line)
    )?;

    Ok(value)
}

/// Iterates through all lines of all supplied chunks.
/// Decompresses the chunks either in parallel or sequentially.
fn for_lines_in_chunks(chunks: impl Send + Iterator<Item = Result<Chunk>>, meta_data: &MetaData, parallel: bool, mut for_each: impl FnMut(Line<'_>) -> PassiveResult) -> PassiveResult {
    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::Uncompressed).is_some();

    if parallel && has_compression {
        let (sender, receiver) = std::sync::mpsc::channel();

        chunks.par_bridge()
            .map(|chunk| UncompressedBlock::decompress_chunk(chunk?, &meta_data))
            .try_for_each_with(sender, |sender, result| {
                result.map(|block: UncompressedBlock| sender.send(block).expect("threading error"))
            })?;

        for decompressed in receiver {
            let header = meta_data.headers.get(decompressed.index.part)
                .ok_or(Error::invalid("chunk index"))?;

            for (bytes, line) in decompressed.index.lines(header) {
                for_each(Line { location: line, value: &decompressed.data[bytes] })?;
            }
        }

        Ok(())
    }
    else {
        for chunk in chunks {
            let decompressed = UncompressedBlock::decompress_chunk(chunk?, &meta_data)?;
            let header = meta_data.headers.get(decompressed.index.part)
                .ok_or(Error::invalid("chunk index"))?;

            for (bytes, line) in decompressed.index.lines(header) {
                for_each(Line { location: line, value: &decompressed.data[bytes] })?;
            }
        }

        Ok(())
    }
}

/// Read all chunks without seeking.
/// Returns the compressed chunks.
pub fn read_all_compressed_chunks<'m>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
) -> Result<(MetaData, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let mut read = PeekRead::new(read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let mut remaining_chunk_count = MetaData::skip_offset_tables(&mut read, &meta_data.headers)? as usize;

    Ok((meta_data, move |meta_data| {
        if remaining_chunk_count > 0 {
            remaining_chunk_count -= 1;
            Some(Chunk::read(&mut read, meta_data))
        }
        else {
            None
        }
    }))
}


/// Read all desired chunks, possibly seeking.
/// Skips all chunks that do not match the filter.
/// Returns the compressed chunks.
pub fn read_filtered_chunks<'m>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    filter: impl Fn(&Header, &TileIndices) -> bool,
) -> Result<(MetaData, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let skip_read = Tracking::new(read);
    let mut read = PeekRead::new(skip_read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let offset_tables = MetaData::read_offset_tables(&mut read, &meta_data.headers)?;

    let mut offsets = Vec::with_capacity(meta_data.headers.len() * 32);
    for (header_index, header) in meta_data.headers.iter().enumerate() {
        for (block_index, block) in header.blocks_increasing_y_order().enumerate() { // in increasing_y order
            if filter(header, &block) {
                offsets.push(offset_tables[header_index][block_index])
            }
        };
    }

    offsets.sort(); // enables reading continuously if possible
    let mut offsets = offsets.into_iter();

    Ok((meta_data, move |meta_data| {
        offsets.next().map(|offset|{
            read.skip_to(offset as usize)?; // no-op for seek at current position, uses skip_bytes for small amounts
            Chunk::read(&mut read, meta_data)
        })
    }))
}




/// Compresses and writes all lines of an image to the writer.
/// Uses multicore compression if desired.
#[must_use]
pub fn write_all_lines_to_buffered(
    write: impl Write + Seek,
    parallel: bool,
    mut meta_data: MetaData,
    get_line: impl Fn(LineIndex) -> ByteVec
) -> PassiveResult
{
    // if non-parallel compression, we always can use increasing order without cost
    if !parallel {
        for header in &mut meta_data.headers {
            if header.line_order == LineOrder::Unspecified {
                header.line_order = LineOrder::Increasing;
            }
        }
    }

    let mut write = Tracking::new(write);
    meta_data.write_to_buffered(&mut write)?;

    let offset_table_start_byte = write.byte_position();

    // skip offset tables for now
    let offset_table_size: u32 = meta_data.headers.iter()
        .map(|header| header.chunk_count).sum();

    write.seek_write_to(write.byte_position() + offset_table_size as usize * std::mem::size_of::<u64>())?;

    let mut offset_tables: Vec<Vec<u64>> = meta_data.headers.iter()
        .map(|header| vec![0; header.chunk_count as usize]).collect();

    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::Uncompressed).is_some();

    if parallel && has_compression {
        // debug_assert_eq!(options.override_line_order, Some(LineOrder::Unspecified));
        unimplemented!()
    }
    else {
        for (part_index, header) in meta_data.headers.iter().enumerate() {

            let mut write_block = |chunk_index: usize, tile: TileIndices| -> Result<()> {
                let data_indices = header.get_absolute_block_indices(tile.location)?;
                let block_indices = BlockIndex {
                    part: part_index,
                    level: tile.location.level_index.to_usize("level index")?,
                    position: data_indices.start.to_usize("data indices start")?,
                    size: data_indices.size.to_usize(),
                };

                let mut data = Vec::new(); // TODO allocate only block, not lines
                for (byte_range, line_index) in block_indices.lines(header) {
                    debug_assert_eq!(byte_range.start, data.len());
                    data.extend_from_slice(get_line(line_index).as_slice());
                    debug_assert_eq!(byte_range.end, data.len());
                }

                // TODO check if data length matches expected byte size

                let data = header.compression.compress_image_section(data)?;

                let chunk = Chunk {
                    part_number: part_index as i32,

                    // TODO deep data
                    block: match header.blocks {
                        Blocks::ScanLines => Block::ScanLine(ScanLineBlock {
                            y_coordinate: header.get_block_data_window_coordinates(tile.location)?.start.1,
                            compressed_pixels: data
                        }),

                        Blocks::Tiles(_) => Block::Tile(TileBlock {
                            coordinates: tile.location,
                            compressed_pixels: data,
                        }),
                    }
                };

                offset_tables[part_index][chunk_index] = write.byte_position() as u64;
                chunk.write(&mut write, meta_data.headers.as_slice())?;

                Ok(())
            };

            if header.line_order == LineOrder::Decreasing {
                for (chunk_index, tile) in header.blocks_increasing_y_order().enumerate().rev() {
                    write_block(chunk_index, tile)?;
                }
            }
            else {
                // TODO
                let _line_order = LineOrder::Increasing; // does not have to be unspecified
                for (chunk_index, tile) in header.blocks_increasing_y_order().enumerate() {
                    write_block(chunk_index, tile)?;
                }
            }
        }
    }

    // write all offset tables
    write.seek_write_to(offset_table_start_byte)?;

    for offset_table in offset_tables {
        u64::write_slice(&mut write, offset_table.as_slice())?;
    }

    Ok(())
}



impl BlockIndex {

    /// Iterates the lines of this block index in interleaved fashion:
    /// For each line in this block, this iterator steps once through each channel.
    /// This is how lines are stored in a pixel data block.
    ///
    /// Does not check whether `self.part_index`, `self.level`, `self.size` and `self.position` are valid indices.__
    // TODO be sure this cannot produce incorrect data, as this is not further checked but only handled with panics
    pub fn lines(&self, header: &Header) -> impl Iterator<Item=(Range<usize>, LineIndex)> {
        struct LineIter {
            part: usize, level: Vec2<usize>, width: usize,
            end_y: usize, x: usize, channel_sizes: SmallVec<[usize; 8]>,
            byte: usize, channel: usize, y: usize,
        };

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
                            part: self.part,
                            level: self.level,
                            position: Vec2(self.x, self.y),
                            width: self.width,
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
            .map(move |channel| self.size.0 * channel.pixel_type.bytes_per_sample() as usize)
            .collect();

        LineIter {
            part: self.part,
            level: self.level,
            width: self.size.0,
            x: self.position.0,
            end_y: self.position.1 + self.size.1,
            channel_sizes: channel_line_sizes,

            byte: 0,
            channel: 0,
            y: self.position.1
        }
    }
}

impl UncompressedBlock {

    /// Decompress the possibly compressed chunk and returns an `UncompressedBlock`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    pub fn decompress_chunk(chunk: Chunk, meta_data: &MetaData) -> Result<Self> {
        let header: &Header = meta_data.headers.get(chunk.part_number as usize)
            .ok_or(Error::invalid("chunk part index"))?;

        let tile_data_indices = header.get_block_data_indices(&chunk.block)?;
        let absolute_indices = header.get_absolute_block_indices(tile_data_indices)?;

        absolute_indices.validate(header.data_window.size)?;

        match chunk.block {
            Block::Tile(TileBlock { compressed_pixels, .. }) |
            Block::ScanLine(ScanLineBlock { compressed_pixels, .. }) => Ok(UncompressedBlock {
                data: header.compression.decompress_image_section(header, compressed_pixels, absolute_indices)?,
                index: BlockIndex {
                    part: i32_to_usize(chunk.part_number, "chunk part number")?,
                    position: absolute_indices.start.to_usize("data indices start")?,
                    level: tile_data_indices.level_index.to_usize("level index")?,
                    size: absolute_indices.size.to_usize(),
                }
            }),

            _ => return Err(Error::unsupported("deep data"))
        }
    }
}

