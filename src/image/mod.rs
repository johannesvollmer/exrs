
//! Read and write an exr image.
//! Use `exr::image::simple` or `exr::image::full` for actually reading a complete image.

pub mod full;
pub mod simple;
pub mod rgba;

use crate::meta::attributes::*;
use crate::compression::{Compression};
use std::io::{Read, Seek};
use crate::error::{Result, UnitResult};
use crate::meta::{MetaData, Header, TileIndices};
use crate::chunk::{Chunk};
use crate::io::{PeekRead, Tracking};
use rayon::iter::{ParallelIterator, ParallelBridge};
use std::convert::TryFrom;
use std::collections::BTreeMap;
use crate::block::{BlockIndex, UncompressedBlock};


/// Specify how to write an exr image.
#[derive(Debug)]
pub struct WriteOptions<P: OnWriteProgress> {

    /// Enable multi-core compression.
    pub parallel_compression: bool,

    /// If enabled, writing an image throws errors
    /// for files that may look invalid to other exr readers.
    /// Should always be true. Only set this to false
    /// if you can risk never opening the file with another exr reader again,
    /// __ever__, really.
    pub pedantic: bool,

    /// Called occasionally while writing a file.
    /// The first argument is the progress, a float from 0 to 1.
    /// The second argument contains the total number of bytes written.
    /// May return `Error::Abort` to cancel writing the file.
    /// Can be a closure accepting a float and a usize, see `OnWriteProgress`.
    pub on_progress: P,
}

/// Specify how to read an exr image.
#[derive(Debug)]
pub struct ReadOptions<P: OnReadProgress> {

    /// Enable multi-core decompression.
    pub parallel_decompression: bool,

    /// Called occasionally while reading a file.
    /// The argument is the progress, a float from 0 to 1.
    /// May return `Error::Abort` to cancel reading the file.
    /// Can be a closure accepting a float, see `OnReadProgress`.
    pub on_progress: P,

    /// Reading an image is aborted if the memory required for the pixels is too large.
    /// The default value of 1GB avoids reading invalid files.
    pub max_pixel_bytes: Option<usize>,
}


/// A collection of preset `WriteOptions` values.
pub mod write_options {
    use super::*;

    /// High speed but also slightly higher memory requirements.
    pub fn default() -> WriteOptions<()> { self::high() }

    /// Higher speed, but slightly higher memory requirements, and __higher risk of incompatibility to other exr readers__.
    /// Only use this if you are confident that the file to write is valid.
    pub fn higher() -> WriteOptions<()> {
        WriteOptions {
            parallel_compression: true,
            pedantic: false,
            on_progress: (),
        }
    }

    /// High speed but also slightly higher memory requirements.
    pub fn high() -> WriteOptions<()> {
        WriteOptions {
            parallel_compression: true, pedantic: true,
            on_progress: (),
        }
    }

    /// Lower speed but also lower memory requirements.
    pub fn low() -> WriteOptions<()> {
        WriteOptions {
            parallel_compression: false, pedantic: true,
            on_progress: (),
        }
    }
}

/// A collection of preset `ReadOptions` values.
pub mod read_options {
    use super::*;

    const GIGABYTE: usize = 1_000_000_000;


    /// High speed but also slightly higher memory requirements.
    pub fn default() -> ReadOptions<()> { self::high() }

    /// High speed but also slightly higher memory requirements.
    /// Aborts reading images that would require more than 1GB of memory.
    pub fn high() -> ReadOptions<()> {
        ReadOptions {
            parallel_decompression: true,
            max_pixel_bytes: Some(GIGABYTE),
            on_progress: (),
        }
    }

    /// Lower speed but also lower memory requirements.
    /// Aborts reading images that would require more than 1GB of memory.
    pub fn low() -> ReadOptions<()> {
        ReadOptions {
            parallel_decompression: false,
            max_pixel_bytes: Some(GIGABYTE),
            on_progress: (),
        }
    }
}


/// Called occasionally when writing a file.
/// Implemented by any closure that matches `|progress: f32, bytes_written: usize| -> UnitResult`.
pub trait OnWriteProgress {

    /// The progress is a float from 0 to 1.
    /// May return `Error::Abort` to cancel writing the file.
    #[must_use]
    fn on_write_progressed(&mut self, progress: f32, bytes_written: usize) -> UnitResult;
}

/// Called occasionally when reading a file.
/// Implemented by any closure that matches `|progress: f32| -> UnitResult`.
pub trait OnReadProgress {

    /// The progress is a float from 0 to 1.
    /// May return `Error::Abort` to cancel reading the file.
    #[must_use]
    fn on_read_progressed(&mut self, progress: f32) -> UnitResult;
}

impl<F> OnWriteProgress for F where F: FnMut(f32, usize) -> UnitResult {
    #[inline] fn on_write_progressed(&mut self, progress: f32, bytes_written: usize) -> UnitResult { self(progress, bytes_written) }
}

impl<F> OnReadProgress for F where F: FnMut(f32) -> UnitResult {
    #[inline] fn on_read_progressed(&mut self, progress: f32) -> UnitResult { self(progress) }
}

impl OnWriteProgress for () {
    #[inline] fn on_write_progressed(&mut self, _progress: f32, _bytes_written: usize) -> UnitResult { Ok(()) }
}

impl OnReadProgress for () {
    #[inline] fn on_read_progressed(&mut self, _progress: f32) -> UnitResult { Ok(()) }
}





/// Reads and decompresses all chunks of a file sequentially without seeking.
/// Will not skip any parts of the file. Does not buffer the reader, you should always pass a `BufReader`.
#[inline]
#[must_use]
pub fn read_all_blocks_from_buffered<T>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, &[Header], UncompressedBlock) -> UnitResult,
    options: ReadOptions<impl OnReadProgress>,
) -> Result<T>
{
    let (meta_data, chunk_count, mut read_chunk) = self::read_all_compressed_chunks_from_buffered(read, options.max_pixel_bytes)?;
    let meta_data_ref = &meta_data;

    let read_chunks = std::iter::from_fn(move || read_chunk(meta_data_ref));
    let mut result = new(meta_data.headers.as_slice())?;

    for_decompressed_blocks_in_chunks(
        read_chunks, &meta_data,
        |meta, block| insert(&mut result, meta, block),
        chunk_count, options
    )?;

    Ok(result)
}



/// Reads ad decompresses all desired chunks of a file sequentially, possibly seeking.
/// Will skip any parts of the file that do not match the specified filter condition.
/// Will never seek if the filter condition matches all chunks.
/// Does not buffer the reader, you should always pass a `BufReader`.
#[inline]
#[must_use]
pub fn read_filtered_blocks_from_buffered<T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    new: impl FnOnce(&[Header]) -> Result<T>, // TODO put these into a trait?
    filter: impl Fn(&T, &Header, &TileIndices) -> bool,
    mut insert: impl FnMut(&mut T, &[Header], UncompressedBlock) -> UnitResult,
    options: ReadOptions<impl OnReadProgress>,
) -> Result<T>
{
    let (meta_data, mut value, chunk_count, mut read_chunk) = {
        self::read_filtered_chunks_from_buffered(read, new, filter, options.max_pixel_bytes)?
    };

    for_decompressed_blocks_in_chunks(
        std::iter::from_fn(|| read_chunk(&meta_data)), &meta_data,
        |meta, line| insert(&mut value, meta, line),
        chunk_count, options
    )?;

    Ok(value)
}

/// Iterates through all lines of all supplied chunks.
/// Decompresses the chunks either in parallel or sequentially.
#[inline]
#[must_use]
fn for_decompressed_blocks_in_chunks(
    chunks: impl Send + Iterator<Item = Result<Chunk>>,
    meta_data: &MetaData,
    mut for_each: impl FnMut(&[Header], UncompressedBlock) -> UnitResult,
    total_chunk_count: usize,
    mut options: ReadOptions<impl OnReadProgress>,
) -> UnitResult
{
    // TODO bit-vec keep check that all pixels have been read?
    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .any(|header| header.compression != Compression::Uncompressed);

    let mut processed_chunk_count = 0;

    if options.parallel_decompression && has_compression {
        let (sender, receiver) = std::sync::mpsc::channel();

        chunks.par_bridge()
            .map(|chunk| UncompressedBlock::decompress_chunk(chunk?, &meta_data))
            .try_for_each_with(sender, |sender, result| {
                result.map(|block: UncompressedBlock| sender.send(block).expect("threading error"))
            })?;

        for decompressed in receiver {
            options.on_progress.on_read_progressed(processed_chunk_count as f32 / total_chunk_count as f32)?;
            processed_chunk_count += 1;

            for_each(meta_data.headers.as_slice(), decompressed)?; // allows returning `Error::Abort`
        }

        Ok(())
    }
    else {
        for chunk in chunks {
            options.on_progress.on_read_progressed(processed_chunk_count as f32 / total_chunk_count as f32)?;
            processed_chunk_count += 1;

            let decompressed = UncompressedBlock::decompress_chunk(chunk?, &meta_data)?;
            for_each(meta_data.headers.as_slice(), decompressed)?; // allows returning `Error::Abort`
        }

        Ok(())
    }
}

/// Read all chunks without seeking.
/// Returns the meta data, number of chunks, and a compressed chunk reader.
/// Does not buffer the reader, you should always pass a `BufReader`.
#[inline]
#[must_use]
pub fn read_all_compressed_chunks_from_buffered<'m>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    max_pixel_bytes: Option<usize>,
) -> Result<(MetaData, usize, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let mut read = PeekRead::new(read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read, max_pixel_bytes)?;
    let mut remaining_chunk_count = usize::try_from(MetaData::skip_offset_tables(&mut read, &meta_data.headers)?)
        .expect("too large chunk count for this machine");

    Ok((meta_data, remaining_chunk_count, move |meta_data| {
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
#[inline]
#[must_use]
pub fn read_filtered_chunks_from_buffered<'m, T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    new: impl FnOnce(&[Header]) -> Result<T>,
    filter: impl Fn(&T, &Header, &TileIndices) -> bool,
    max_pixel_bytes: Option<usize>,
) -> Result<(MetaData, T, usize, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let skip_read = Tracking::new(read);
    let mut read = PeekRead::new(skip_read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read, max_pixel_bytes)?;

    let value = new(meta_data.headers.as_slice())?;

    let offset_tables = MetaData::read_offset_tables(&mut read, &meta_data.headers)?;

    let mut offsets = Vec::with_capacity(meta_data.headers.len() * 32);
    for (header_index, header) in meta_data.headers.iter().enumerate() { // offset tables are stored same order as headers
        for (block_index, block) in header.blocks_increasing_y_order().enumerate() { // in increasing_y order
            if filter(&value, header, &block) {
                offsets.push(offset_tables[header_index][block_index]) // safe indexing from `enumerate()`
            }
        };
    }

    offsets.sort(); // enables reading continuously if possible (is probably already sorted)
    let mut offsets = offsets.into_iter();
    let block_count = offsets.len();

    Ok((meta_data, value, block_count, move |meta_data| {
        offsets.next().map(|offset|{
            read.skip_to(usize::try_from(offset).expect("too large chunk position for this machine"))?; // no-op for seek at current position, uses skip_bytes for small amounts
            Chunk::read(&mut read, meta_data)
        })
    }))
}



/// Iterate over all uncompressed blocks of an image.
/// The image contents are collected by the `get_line` function parameter.
/// Returns blocks in `LineOrder::Increasing`, unless the line order is requested to be decreasing.
#[inline]
#[must_use]
pub fn uncompressed_image_blocks_ordered<'l>(
    meta_data: &'l MetaData,
    get_block: &'l (impl 'l + Sync + (Fn(&[Header], BlockIndex) -> Vec<u8>)) // TODO reduce sync requirements, at least if parrallel is false
) -> impl 'l + Iterator<Item = Result<(usize, UncompressedBlock)>> + Send // TODO reduce sync requirements, at least if parrallel is false
{
    meta_data.headers.iter().enumerate()
        .flat_map(move |(layer_index, header)|{
            header.enumerate_ordered_blocks().map(move |(chunk_index, tile)|{
                let data_indices = header.get_absolute_block_indices(tile.location).expect("tile coordinate bug");

                let block_indices = BlockIndex {
                    layer: layer_index, level: tile.location.level_index,
                    pixel_position: data_indices.position.to_usize("data indices start").expect("data index bug"),
                    pixel_size: data_indices.size,
                };

                let block_bytes = get_block(meta_data.headers.as_slice(), block_indices);

                // byte length is validated in block::compress_to_chunk
                Ok((chunk_index, UncompressedBlock {
                    index: block_indices,
                    data: block_bytes
                }))
            })
        })
}



/// Compress all chunks in the image described by `meta_data` and `get_line`.
/// Calls `write_chunk` for each compressed chunk, while respecting the `line_order` of the image.
///
/// Attention: Currently, using multi-core compression with `LineOrder::Increasing` or `LineOrder::Decreasing` in any header
/// will allocate large amounts of memory while writing the file. Use unspecified line order for lower memory usage.
#[inline]
#[must_use]
pub fn for_compressed_blocks_in_image(
    meta_data: &MetaData, get_tile: impl Sync + Fn(&[Header], BlockIndex) -> Vec<u8>,
    parallel: bool, mut write_chunk: impl FnMut(usize, Chunk) -> UnitResult
) -> UnitResult
{
    let blocks = uncompressed_image_blocks_ordered(meta_data, &get_tile);

    let parallel = parallel && meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .any(|header| header.compression != Compression::Uncompressed);

    let requires_sorting = meta_data.headers.iter()
        .any(|header| header.line_order != LineOrder::Unspecified);


    if parallel {
        let (sender, receiver) = std::sync::mpsc::channel();

        blocks.par_bridge()
            .map(|result| Ok({
                let (chunk_index, block) = result?;
                let block = block.compress_to_chunk(meta_data)?;
                (chunk_index, block)
            }))
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
        for result in blocks {
            let (chunk_index, uncompressed_block) = result?; // enable `Error::Abort`
            let chunk = uncompressed_block.compress_to_chunk(meta_data)?;
            write_chunk(chunk_index, chunk)?;
        }
    }

    Ok(())
}
