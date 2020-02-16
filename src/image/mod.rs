
//! Read and write an exr image.
//! Use `exr::image::simple` or `exr::image::full` for actually reading a complete image.

pub mod full;
pub mod simple;
pub mod rgba;

use crate::meta::attributes::*;
use crate::compression::{Compression, ByteVec};
use crate::math::*;
use std::io::{Read, Seek, Write, Cursor};
use crate::error::{Result, Error, PassiveResult, usize_to_i32};
use crate::meta::{MetaData, Header, TileIndices, Blocks};
use crate::chunks::{Chunk, Block, TileBlock, ScanLineBlock, TileCoordinates};
use crate::io::{PeekRead, Tracking};
use rayon::iter::{ParallelIterator, ParallelBridge};
use crate::io::Data;
use smallvec::SmallVec;
use std::ops::Range;
use std::convert::TryFrom;
use std::collections::BTreeMap;


/// Specifies where a block of pixel data should be placed in the actual image.
/// This is a globally unique identifier which
/// includes the layer, level index, and pixel location.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub struct BlockIndex {

    /// Index of the layer.
    pub layer: usize,

    /// Pixel position of the bottom left corner of the block.
    pub pixel_position: Vec2<usize>,

    /// Pixel size of the block.
    pub pixel_size: Vec2<usize>,

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

impl<'s> LineRefMut<'s> {

    /// Writes the samples (f16, f32, u32 values) into this line value reference.
    /// Use `write_samples` if there is not slice available.
    pub fn write_samples_from_slice<T: crate::io::Data>(self, slice: &[T]) -> PassiveResult {
        debug_assert_eq!(slice.len(), self.location.sample_count, "slice size does not match the line width");
        debug_assert_eq!(self.value.len(), self.location.sample_count * T::BYTE_SIZE, "sample type size does not match line byte size");

        T::write_slice(&mut Cursor::new(self.value), slice)
    }

    /// Iterate over all samples in this line, from left to right.
    /// The supplied `get_line` function returns the sample value
    /// for a given sample index within the line,
    /// which starts at zero for each individual line.
    /// Use `write_samples_from_slice` if you already have a slice of samples.
    pub fn write_samples<T: crate::io::Data>(self, mut get_sample: impl FnMut(usize) -> T) -> PassiveResult {
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
    pub fn read_samples_into_slice<T: crate::io::Data>(self, slice: &mut [T]) -> PassiveResult {
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


/// Reads and decompresses all chunks of a file sequentially without seeking.
/// Will not skip any parts of the file. Does not buffer the reader, you should always pass a `BufReader`.
pub fn read_all_lines_from_buffered<T>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    parallel: bool,
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, LineRef<'_>) -> PassiveResult
) -> Result<T>
{
    let (meta_data, mut read_chunk) = self::read_all_compressed_chunks_from_buffered(read)?;
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
/// Does not buffer the reader, you should always pass a `BufReader`.
pub fn read_filtered_lines_from_buffered<T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    parallel: bool,
    filter: impl Fn(&Header, &TileIndices) -> bool,
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, LineRef<'_>) -> PassiveResult
) -> Result<T>
{
    let (meta_data, mut read_chunk) = self::read_filtered_chunks_from_buffered(read, filter)?;
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
fn for_lines_in_chunks(
    chunks: impl Send + Iterator<Item = Result<Chunk>>,
    meta_data: &MetaData, parallel: bool, mut for_each: impl FnMut(LineRef<'_>) -> PassiveResult
) -> PassiveResult
{
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
            let header = meta_data.headers.get(decompressed.index.layer)
                .ok_or(Error::invalid("chunk index"))?;

            for (bytes, line) in decompressed.index.line_indices(header) {
                for_each(LineSlice { location: line, value: &decompressed.data[bytes] })?;
            }
        }

        Ok(())
    }
    else {
        for chunk in chunks {
            let decompressed = UncompressedBlock::decompress_chunk(chunk?, &meta_data)?;
            let header = meta_data.headers.get(decompressed.index.layer)
                .ok_or(Error::invalid("chunk index"))?;

            for (bytes, line) in decompressed.index.line_indices(header) {
                for_each(LineSlice { location: line, value: &decompressed.data[bytes] })?;
            }
        }

        Ok(())
    }
}

/// Read all chunks without seeking.
/// Returns the compressed chunks.
/// Does not buffer the reader, you should always pass a `BufReader`.
pub fn read_all_compressed_chunks_from_buffered<'m>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
) -> Result<(MetaData, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let mut read = PeekRead::new(read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let mut remaining_chunk_count = usize::try_from(MetaData::skip_offset_tables(&mut read, &meta_data.headers)?)
        .expect("too large chunk count for this machine");

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


/// Read all desired chunks, possibly seeking. Skips all chunks that do not match the filter.
/// Returns the compressed chunks. Does not buffer the reader, you should always pass a `BufReader`.
// TODO this must be tested more
pub fn read_filtered_chunks_from_buffered<'m>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    filter: impl Fn(&Header, &TileIndices) -> bool,
) -> Result<(MetaData, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let skip_read = Tracking::new(read);
    let mut read = PeekRead::new(skip_read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let offset_tables = MetaData::read_offset_tables(&mut read, &meta_data.headers)?;

    let mut offsets = Vec::with_capacity(meta_data.headers.len() * 32);
    for (header_index, header) in meta_data.headers.iter().enumerate() { // offset tables are stored same order as headers
        for (block_index, block) in header.blocks_increasing_y_order().enumerate() { // in increasing_y order
            if filter(header, &block) {
                offsets.push(offset_tables[header_index][block_index]) // safe indexing from `enumerate()`
            }
        };
    }

    offsets.sort(); // enables reading continuously if possible
    let mut offsets = offsets.into_iter();

    Ok((meta_data, move |meta_data| {
        offsets.next().map(|offset|{
            read.skip_to(usize::try_from(offset).expect("too large chunk position for this machine"))?; // no-op for seek at current position, uses skip_bytes for small amounts
            Chunk::read(&mut read, meta_data)
        })
    }))
}



/// Iterate over all uncompressed blocks of an image.
/// The image contents are collected by the `get_line` function parameter.
/// Returns blocks in `LineOrder::Increasing`, unless the line order is requested to be decreasing.
pub fn uncompressed_image_blocks_ordered<'l>(
    meta_data: &'l MetaData,
    get_line: &'l (impl Fn(LineRefMut<'_>) + 'l + Sync) // TODO reduce sync requirements, at least if parrallel is false
) -> impl Iterator<Item = (usize, UncompressedBlock)> + 'l + Send // TODO reduce sync requirements, at least if parrallel is false
{
    meta_data.headers.iter().enumerate()
        .flat_map(move |(layer_index, header)|{
            header.enumerate_ordered_blocks().map(move |(chunk_index, tile)|{
                let data_indices = header.get_absolute_block_indices(tile.location).expect("tile coordinate bug");

                let block_indices = BlockIndex {
                    layer: layer_index, level: tile.location.level_index,
                    pixel_position: data_indices.start.to_usize("data indices start").expect("data index bug"),
                    pixel_size: data_indices.size,
                };

                let mut block_bytes = Vec::with_capacity(header.max_block_byte_size());
                for (byte_range, line_index) in block_indices.line_indices(header) {
                    block_bytes.resize(byte_range.end, 0_u8);

                    let line_mut = LineRefMut {
                        value: &mut block_bytes[byte_range],
                        location: line_index,
                    };

                    get_line(line_mut);
                }

                // byte length is validated in block::compress_to_chunk
                (chunk_index, UncompressedBlock {
                    index: block_indices,
                    data: block_bytes
                })
            })
        })
}



/// Compress all chunks in the image described by `meta_data` and `get_line`.
/// Calls `write_chunk` for each compressed chunk, while respecting the `line_order` of the image.
///
/// Attention: Currently, using multicore compression with `LineOrder::Increasing` or `LineOrder::Decreasing` in any header
/// will allocate large amounts of memory while writing the file. Use unspecified line order for lower memory usage.
pub fn for_compressed_blocks_in_image(
    meta_data: &MetaData, get_line: impl Fn(LineRefMut<'_>) + Sync,
    parallel: bool, mut write_chunk: impl FnMut(usize, Chunk) -> PassiveResult
) -> PassiveResult
{
    let blocks = uncompressed_image_blocks_ordered(meta_data, &get_line);

    let parallel = parallel && meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .any(|header| header.compression != Compression::Uncompressed);

    let requires_sorting = meta_data.headers.iter()
        .any(|header| header.line_order != LineOrder::Unspecified);


    if parallel {
        let (sender, receiver) = std::sync::mpsc::channel();

        blocks.par_bridge()
            .map(|(chunk_index, block)| Ok((chunk_index, block.compress_to_chunk(meta_data)?)))
            .try_for_each_with(sender, |sender, result: Result<(usize, Chunk)>| {
                result.map(|block| sender.send(block).expect("threading error"))
            })?;

        if !requires_sorting {
            // FIXME does the original openexr library support unspecified line orders that have mixed up headers???
            //       Or must the header order always be contiguous without overlaps?
            for (chunk_index, compressed_chunk) in receiver {
                write_chunk(chunk_index, compressed_chunk)?;
            }
        }

        // write parallel chunks with sorting
        else {

            // the block indices, in the order which must be apparent in the file
            let mut expected_id_order = meta_data.headers.iter().enumerate()
                .flat_map(|(layer, header)| header.enumerate_ordered_blocks().map(move |(chunk, _)| (layer, chunk)));

            // the next id, pulled from expected_id_order: the next block that must be written
            let mut next_id = expected_id_order.next();

            // set of blocks that have been compressed but not written yet
            let mut pending_blocks = BTreeMap::new();

            // receive the compressed blocks
            for (chunk_index, compressed_chunk) in receiver {
                pending_blocks.insert((compressed_chunk.layer_index, chunk_index), compressed_chunk);

                // write all pending blocks that are immediate successors
                while let Some(pending_chunk) = next_id.as_ref().and_then(|id| pending_blocks.remove(id)) {
                    let pending_chunk_index = next_id.unwrap().1; // must be safe in this branch
                    write_chunk(pending_chunk_index, pending_chunk)?;
                    next_id = expected_id_order.next();
                }
            }

            assert!(expected_id_order.next().is_none(), "expected more blocks bug");
            assert_eq!(pending_blocks.len(), 0, "pending blocks left after processing bug");
        }
    }

    else {
        for (chunk_index, uncompressed_block) in blocks {
            let chunk = uncompressed_block.compress_to_chunk(meta_data)?;
            write_chunk(chunk_index, chunk)?;
        }
    }

    Ok(())
}

/// Compresses and writes all lines of an image described by `meta_data` and `get_line` to the writer.
///
/// Attention: Currently, using multicore compression with `LineOrder::Increasing` or `LineOrder::Decreasing` in any header
/// will allocate large amounts of memory while writing the file. Use unspecified line order for lower memory usage.
///
/// Does not buffer the writer, you should always pass a `BufWriter`.
/// If pedantic, throws errors for files that may produce errors in other exr readers.
#[must_use]
pub fn write_all_lines_to_buffered(
    write: impl Write + Seek,
    parallel: bool, pedantic: bool,
    mut meta_data: MetaData,
    get_line: impl Fn(LineRefMut<'_>) + Sync // TODO why is this sync or send????
) -> PassiveResult
{
    // if non-parallel compression, we always use increasing order anyways
    if !parallel {
        for header in &mut meta_data.headers {
            if header.line_order == LineOrder::Unspecified {
                header.line_order = LineOrder::Increasing;
            }
        }
    }

    let mut write = Tracking::new(write);
    meta_data.write_validating_to_buffered(&mut write, pedantic)?; // also validates meta data

    let offset_table_start_byte = write.byte_position();

    // skip offset tables for now
    let offset_table_size: usize = meta_data.headers.iter()
        .map(|header| header.chunk_count).sum();

    write.seek_write_to(write.byte_position() + offset_table_size * std::mem::size_of::<u64>())?;

    let mut offset_tables: Vec<Vec<u64>> = meta_data.headers.iter()
        .map(|header| vec![0; header.chunk_count]).collect();

    // line order is respected in here
    for_compressed_blocks_in_image(&meta_data, get_line, parallel, |chunk_index, chunk|{
        offset_tables[chunk.layer_index][chunk_index] = write.byte_position() as u64; // safe indices from `enumerate()`
        chunk.write(&mut write, meta_data.headers.as_slice())
    })?;

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
    /// Does not check whether `self.layer_index`, `self.level`, `self.size` and `self.position` are valid indices.__
    // TODO be sure this cannot produce incorrect data, as this is not further checked but only handled with panics
    pub fn line_indices(&self, header: &Header) -> impl Iterator<Item=(Range<usize>, LineIndex)> {
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
            .map(move |channel| self.pixel_size.0 * channel.pixel_type.bytes_per_sample()) // FIXME is it fewer samples per tile or just fewer tiles for sampled images???
            .collect();

        LineIter {
            layer: self.layer,
            level: self.level,
            width: self.pixel_size.0,
            x: self.pixel_position.0,
            end_y: self.pixel_position.1 + self.pixel_size.1,
            channel_sizes: channel_line_sizes,

            byte: 0,
            channel: 0,
            y: self.pixel_position.1
        }
    }
}

impl UncompressedBlock {

    /// Decompress the possibly compressed chunk and returns an `UncompressedBlock`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    pub fn decompress_chunk(chunk: Chunk, meta_data: &MetaData) -> Result<Self> {
        let header: &Header = meta_data.headers.get(chunk.layer_index)
            .ok_or(Error::invalid("chunk layer index"))?;

        let tile_data_indices = header.get_block_data_indices(&chunk.block)?;
        let absolute_indices = header.get_absolute_block_indices(tile_data_indices)?;

        absolute_indices.validate(header.data_window.size)?;

        match chunk.block {
            Block::Tile(TileBlock { compressed_pixels, .. }) |
            Block::ScanLine(ScanLineBlock { compressed_pixels, .. }) => Ok(UncompressedBlock {
                data: header.compression.decompress_image_section(header, compressed_pixels, absolute_indices)?,
                index: BlockIndex {
                    layer: chunk.layer_index,
                    pixel_position: absolute_indices.start.to_usize("data indices start")?,
                    level: tile_data_indices.level_index,
                    pixel_size: absolute_indices.size,
                }
            }),

            _ => return Err(Error::unsupported("deep data not supported yet"))
        }
    }

    /// Consume this block by compressing it, returning a `Chunk`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    pub fn compress_to_chunk(self, meta_data: &MetaData) -> Result<Chunk> {
        let UncompressedBlock { data, index } = self;

        let header: &Header = meta_data.headers.get(index.layer)
            .expect("block layer index bug");

        let expected_byte_size = header.channels.bytes_per_pixel * self.index.pixel_size.area(); // TODO sampling??
        if expected_byte_size != data.len() {
            panic!("get_line byte size should be {} but was {}", expected_byte_size, data.len());
        }

        let compressed_data = header.compression.compress_image_section(data)?;

        Ok(Chunk {
            layer_index: index.layer,
            block : match header.blocks {
                Blocks::ScanLines => Block::ScanLine(ScanLineBlock {
                    compressed_pixels: compressed_data,

                    // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
                    y_coordinate: usize_to_i32(index.pixel_position.1) + header.data_window.start.1,
                }),

                Blocks::Tiles(tiles) => Block::Tile(TileBlock {
                    compressed_pixels: compressed_data,
                    coordinates: TileCoordinates {
                        level_index: index.level,

                        // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
                        tile_index: index.pixel_position / tiles.tile_size,
                    },

                }),
            }
        })
    }
}

