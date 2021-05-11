//! Handle compressed and uncompressed pixel byte blocks. Includes compression and decompression,
//! and reading a complete image into blocks.

pub mod lines;
pub mod samples;
pub mod chunk;

use crate::compression::{ByteVec, Compression};
use crate::math::*;
use crate::error::{Result, Error, usize_to_i32, UnitResult, u64_to_usize, usize_to_u64, IoError};
use crate::meta::{MetaData, Blocks, TileIndices, OffsetTables, Headers};
use crate::block::chunk::{Chunk, Block, TileBlock, ScanLineBlock, TileCoordinates};
use crate::meta::attribute::LineOrder;
use rayon::prelude::*;
use smallvec::alloc::collections::BTreeMap;
use std::convert::TryFrom;
use crate::io::{Tracking, PeekRead, Write, Data};
use std::io::{Seek, Read, ErrorKind};
use crate::meta::header::Header;
use crate::block::lines::{LineRef, LineIndex, LineSlice, LineRefMut};
use std::sync::mpsc::Receiver;


/// Specifies where a block of pixel data should be placed in the actual image.
/// This is a globally unique identifier which
/// includes the layer, level index, and pixel location.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub struct BlockIndex {

    /// Index of the layer.
    pub layer: usize,

    /// Index of the bottom left pixel from the block.
    pub pixel_position: Vec2<usize>,

    /// Number of pixels in this block. Stays the same across all resolution levels.
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

pub struct MetaDataReader<R> {
    meta_data: MetaData,
    remaining_reader: PeekRead<Tracking<R>>,
}

impl<R: Read + Seek> MetaDataReader<R> {
    pub fn read(read: R, pedantic: bool) -> Result<Self> {
        let mut remaining_reader = PeekRead::new(Tracking::new(read));
        let meta_data = MetaData::read_validated_from_buffered_peekable(&mut remaining_reader, pedantic)?;
        Ok(Self { meta_data, remaining_reader })
    }

    // avoid mutable, as relied upon later on
    pub fn meta_data(&self) -> &MetaData { &self.meta_data }


    pub fn all_blocks(mut self, pedantic: bool) -> Result<AllChunksReader<R>> {
        let total_chunk_count = {
            if pedantic {
                let offset_tables = MetaData::read_offset_tables(&mut self.remaining_reader, &self.meta_data.headers)?;
                validate_offset_tables(self.meta_data.headers.as_slice(), &offset_tables, self.remaining_reader.byte_position())?;
                offset_tables.iter().map(|table| table.len()).sum()
            }
            else {
                usize::try_from(MetaData::skip_offset_tables(&mut self.remaining_reader, &self.meta_data.headers)?)
                    .expect("too large chunk count for this machine")
            }
        };

        Ok(AllChunksReader {
            meta_data: self.meta_data,
            remaining_chunks: 0 .. total_chunk_count,
            remaining_bytes: self.remaining_reader
        })
    }

    // TODO tile indices add no new information to block index??
    pub fn filter_blocks(mut self, pedantic: bool, mut filter: impl FnMut((usize, &Header), (/*TODO BlockIndex*/usize, TileIndices)) -> bool) -> Result<FilteredChunksReader<R>> {
        let offset_tables = MetaData::read_offset_tables(&mut self.remaining_reader, &self.meta_data.headers)?;

        // TODO regardless of pedantic, if invalid, read all chunks instead, and filter after reading each chunk?
        if pedantic {
            validate_offset_tables(
                self.meta_data.headers.as_slice(), &offset_tables,
                self.remaining_reader.byte_position()
            )?;
        }

        let mut filtered_offsets = Vec::with_capacity((self.meta_data.headers.len() * 32).min(2*2048));
        for (header_index, header) in self.meta_data.headers.iter().enumerate() { // offset tables are stored same order as headers
            for (block_index, block) in header.blocks_increasing_y_order().enumerate() { // in increasing_y order
                if filter((header_index, header), (block_index, block)) {
                    filtered_offsets.push(offset_tables[header_index][block_index]) // safe indexing from `enumerate()`
                }
            };
        }

        filtered_offsets.sort_unstable(); // enables reading continuously if possible (is probably already sorted)
        let mut filtered_offsets = filtered_offsets.into_iter();

        Ok(FilteredChunksReader {
            meta_data: self.meta_data,
            // filtered_chunk_count: filtered_offsets.len(),
            remaining_filtered_chunk_indices: filtered_offsets.into_iter(),
            remaining_bytes: self.remaining_reader
        })
    }
}

pub struct FilteredChunksReader<R> {
    meta_data: MetaData,
    // filtered_chunk_count: usize,
    remaining_filtered_chunk_indices: std::vec::IntoIter<u64>,
    remaining_bytes: PeekRead<Tracking<R>>,
}

pub struct AllChunksReader<R> {
    meta_data: MetaData,
    remaining_chunks: std::ops::Range<usize>,
    remaining_bytes: PeekRead<Tracking<R>>,
}

pub trait ChunksReader: Sized + Iterator<Item=Result<Chunk>> {
    fn meta_data(&self) -> &MetaData;
    fn read_next_chunk(&mut self) -> Option<Result<Chunk>> { self.next() }

    // TODO fn expected_chunk_count() -> usize;
    // TODO fn remaining_chunk_count() -> usize;

    // TODO calling this immediately starts the decompression process...? acceptable?
    //-/ Requires the byte source (`file` or other `Read`) to also be `Send`.
    //-/ Use `decompress_sequential`, or the `sequential_decompressor` if your byte source is not sendable.
    fn decompress_parallel<Image: Send>(
        self, pedantic: bool,
        initial: Image,
        insert_block: impl Send + Fn(Image, UncompressedBlock) -> Result<Image>
    ) -> Result<Image>
        where Self: Send // requires `Read + Seek + Send`
    {
        // do not use parallel stuff for uncompressed images
        let has_compression = self.meta_data().headers.iter().any(|header| header.compression != Compression::Uncompressed);
        if !has_compression { return self.decompress_sequential(pedantic, initial, insert_block); }

        let meta_clone = self.meta_data().clone(); // TODO no clone?
        self
            .collect::<Vec<_>>().into_par_iter() // FIXME DO NOT allocate all at once
            .map(|compressed_chunk| UncompressedBlock::decompress_chunk(compressed_chunk?, &meta_clone, pedantic))
            .collect::<Result<Vec<_>>>()?.into_iter().try_fold(initial, insert_block)

        /*// if 12 tiles have already been decompressed, wait until they are processed before decompressing more tiles
        let (sender, receiver) = std::sync::mpsc::sync_channel(12);

        // assemble the image by processing block by block, in a new thread
        // TODO when collector has error, it kills the sender, right?
        let collector = std::thread::spawn(move ||{ // TODO async/futures instead?
            // receiver.into_iter().try_fold(image, insert_block)
            let mut image = initial;

            // iter gracefully ends, if the decompressor has had an error
            for block in receiver { image = insert_block(image, block?)?; }

            Ok(image)
        });

        let meta_clone = self.meta_data().clone(); // TODO no clone?
        let all_blocks_were_decompressed = self.par_bridge()
            .map(|compressed_chunk| UncompressedBlock::decompress_chunk(compressed_chunk?, &meta_clone, pedantic))
            .try_fold_with(sender, |sender, block: Result<UncompressedBlock>| {
                // sending may fail when the collector has an Err(), so we gracefully exit this fold operation when sending fails

                block.and_then(|block: UncompressedBlock| sender.send(block).map_err(|_| )))?;
                Ok(sender)
            }).try_reduce_with();

        // if the decompressor errors, this returns and drops the collector thread without joining
        // however, this reports the artificial error from the decompressor, not the original collector error
        all_blocks_were_decompressed?;

        let image = collector.join().expect("thread error");
        image*/
    }

    fn decompress_sequential<Image: Send>(
        self, pedantic: bool,
        initial: Image,
        insert_block: impl Fn(Image, UncompressedBlock) -> Result<Image>
    ) -> Result<Image>
    {
        let image = self.sequential_decompressor(pedantic)
            .try_fold(initial, |image, block| insert_block(image, block?))?;

        // TODO
        // if pedantic && (self.remaining_chunk_count != 0 || self.inner_writer().peek_u8().is_ok()){
        //     Err(Error::invalid("bytes left after all chunks have been processed"))
        // }

        Ok(image)
    }

    /// Prepare reading the chunks sequentially, using one CPU at a time, with less memory overhead.
    fn sequential_decompressor(self, pedantic: bool) -> SequentialBlockReader<Self> {
        SequentialBlockReader { remaining_chunks_reader: self, pedantic }
    }
}

impl<R: Read + Seek> ChunksReader for AllChunksReader<R> {
    fn meta_data(&self) -> &MetaData { &self.meta_data }
}
impl<R: Read + Seek> Iterator for AllChunksReader<R> {
    type Item = Result<Chunk>;

    fn next(&mut self) -> Option<Self::Item> {
        // read as many chunks as the file should contain
        self.remaining_chunks.next()
            .map(|_| Chunk::read(&mut self.remaining_bytes, &self.meta_data))
    }
}

impl<R: Read + Seek> ChunksReader for FilteredChunksReader<R> {
    fn meta_data(&self) -> &MetaData { &self.meta_data }
}
impl<R: Read + Seek> Iterator for FilteredChunksReader<R> {
    type Item = Result<Chunk>;

    fn next(&mut self) -> Option<Self::Item> {
        // read as many chunks as we have desired chunk offsets
        self.remaining_filtered_chunk_indices.next().map(|next_chunk_location|{
            self.remaining_bytes.skip_to( // no-op for seek at current position, uses skip_bytes for small amounts
              usize::try_from(next_chunk_location)
                  .expect("too large chunk position for this machine")
            )?;

            let meta_data = &self.meta_data;
            Chunk::read(&mut self.remaining_bytes, meta_data)
        })

        // TODO else if indices empty but file remains, do a check
        // if pedantic && read.peek_u8().is_ok() {
        //     return Some(Err(Error::invalid("end of file expected")));
        // }
    }
}

pub struct SequentialBlockReader<R: ChunksReader> {
    remaining_chunks_reader: R,
    pedantic: bool,
}


impl<R: ChunksReader> SequentialBlockReader<R> {
    // TODO on_progress(processed_chunk_count as f64 / total_chunk_count as f64);
    // processed_chunk_count += 1;

    /// Read and then decompress a single block of pixels from the byte source.
    pub fn decompress_next_block(&mut self) -> Option<Result<UncompressedBlock>> {
        self.remaining_chunks_reader.read_next_chunk().map(|compressed_chunk|{
            UncompressedBlock::decompress_chunk(compressed_chunk?, &self.remaining_chunks_reader.meta_data(), self.pedantic)
        })
    }
}

impl<R: ChunksReader> Iterator for SequentialBlockReader<R> {
    type Item = Result<UncompressedBlock>;
    fn next(&mut self) -> Option<Self::Item> { self.decompress_next_block() }
}







/// Compresses and writes all lines of an image described by `meta_data` and `get_line` to the writer.
/// Flushes the writer to explicitly handle all errors.
///
/// Attention: Currently, using multi-core compression with [LineOrder::Increasing] or [LineOrder::Decreasing] in any header
/// can potentially allocate large amounts of memory while writing the file. Use unspecified line order for lower memory usage.
///
/// Does not buffer the writer, you should always pass a `BufWriter`.
/// If pedantic, throws errors for files that may produce errors in other exr readers.
#[inline]
#[must_use]
pub fn write_all_blocks_to_buffered(
    write: impl Write + Seek,
    mut headers: Headers,
    get_tile: impl Sync + Fn(&[Header], BlockIndex) -> Vec<u8>, // TODO put these three parameters into a trait?  // TODO why is this sync or send????
    mut on_progress: impl FnMut(f64),
    pedantic: bool, parallel: bool,
) -> UnitResult
{
    let has_compression = headers.iter() // TODO cache this in MetaData.has_compression?
        .any(|header| header.compression != Compression::Uncompressed);

    // if non-parallel compression, we always use increasing order anyways
    if !parallel || !has_compression {
        for header in &mut headers {
            if header.line_order == LineOrder::Unspecified {
                header.line_order = LineOrder::Increasing;
            }
        }
    }

    let mut write = Tracking::new(write);
    MetaData::write_validating_to_buffered(&mut write, headers.as_slice(), pedantic)?;

    let offset_table_start_byte = write.byte_position();

    // skip offset tables for now
    let offset_table_size: usize = headers.iter()
        .map(|header| header.chunk_count).sum();

    write.seek_write_to(write.byte_position() + offset_table_size * std::mem::size_of::<u64>())?;

    let mut offset_tables: Vec<Vec<u64>> = headers.iter()
        .map(|header| vec![0; header.chunk_count]).collect();

    let total_chunk_count = offset_table_size as f64;
    let mut processed_chunk_count = 0; // used for very simple on_progress feedback

    // line order is respected in here
    crate::block::for_compressed_blocks_in_image(headers.as_slice(), get_tile, parallel, |chunk_index, chunk|{
        offset_tables[chunk.layer_index][chunk_index] = usize_to_u64(write.byte_position()); // safe indices from `enumerate()`
        chunk.write(&mut write, headers.as_slice())?;

        on_progress(0.95 * processed_chunk_count as f64 / total_chunk_count /*write.byte_position()*/);
        processed_chunk_count += 1;

        Ok(())
    })?;

    debug_assert_eq!(processed_chunk_count, offset_table_size, "not all chunks were written");

    // write all offset tables
    write.seek_write_to(offset_table_start_byte)?;

    for offset_table in offset_tables {
        u64::write_slice(&mut write, offset_table.as_slice())?;
    }

    write.flush()?; // make sure we catch all (possibly delayed) io errors before returning
    on_progress(1.0);

    Ok(())
}


/// Reads and decompresses all chunks of a file sequentially without seeking.
/// Will not skip any parts of the file. Does not buffer the reader, you should always pass a `BufReader`.
#[inline]
#[must_use]
pub fn read_all_blocks_from_buffered<T>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, &[Header], UncompressedBlock) -> UnitResult,
    on_progress: impl FnMut(f64),
    pedantic: bool, parallel: bool,
) -> Result<T>
{
    let (meta_data, chunk_count, mut read_chunk) = self::read_all_compressed_chunks_from_buffered(read, pedantic)?;
    let meta_data_ref = &meta_data;

    // TODO chunk count for ReadOnProgress!

    let read_chunks = std::iter::from_fn(move || read_chunk(meta_data_ref));
    let mut result = new(meta_data.headers.as_slice())?;

    for_decompressed_blocks_in_chunks(
        read_chunks, &meta_data, chunk_count,
        |meta, block| insert(&mut result, meta, block),
        on_progress, pedantic, parallel
    )?;

    Ok(result)
}



/// Reads ad decompresses all desired chunks of a file sequentially, possibly seeking.
/// Will skip any parts of the file that do not match the specified filter condition.
/// Will never seek if the filter condition matches all chunks.
/// Does not buffer the reader, you should always pass a `BufReader`.
/// This may leave you with an uninitialized image, when all blocks are filtered out.
#[inline]
#[must_use]
pub fn read_filtered_blocks_from_buffered<T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    new: impl FnOnce(&[Header]) -> Result<T>, // TODO put these into a trait?
    filter: impl Fn(&T, (usize, &Header), (usize, &TileIndices)) -> bool,
    mut insert: impl FnMut(&mut T, &[Header], UncompressedBlock) -> UnitResult,
    on_progress: impl FnMut(f64),
    pedantic: bool, parallel: bool,
) -> Result<T>
{
    let (meta_data, mut value, chunk_count, mut read_chunk) = {
        self::read_filtered_chunks_from_buffered(read, new, filter, pedantic)?
    };

    for_decompressed_blocks_in_chunks(
        std::iter::from_fn(|| read_chunk(&meta_data)), &meta_data, chunk_count,
        |meta, line| insert(&mut value, meta, line),
        on_progress, pedantic, parallel,
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
    total_chunk_count: usize,
    mut for_each: impl FnMut(&[Header], UncompressedBlock) -> UnitResult,
    mut on_progress: impl FnMut(f64),
    pedantic: bool, parallel: bool,
) -> UnitResult
{
    // TODO bit-vec keep check that all pixels have been read?
    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .any(|header| header.compression != Compression::Uncompressed);

    let mut processed_chunk_count = 0;

    if parallel && has_compression {
        let (sender, receiver) = std::sync::mpsc::channel();

        chunks.par_bridge()
            .map(|chunk| UncompressedBlock::decompress_chunk(chunk?, &meta_data, pedantic))
            .try_for_each_with(sender, |sender, result| {
                result.map(|block: UncompressedBlock| sender.send(block).expect("threading error"))
            })?;

        for decompressed in receiver {
            on_progress(processed_chunk_count as f64 / total_chunk_count as f64);
            processed_chunk_count += 1;

            for_each(meta_data.headers.as_slice(), decompressed)?; // allows returning `Error::Abort`
        }
    }
    else {
        for chunk in chunks {
            on_progress(processed_chunk_count as f64 / total_chunk_count as f64);
            processed_chunk_count += 1;

            let decompressed = UncompressedBlock::decompress_chunk(chunk?, &meta_data, pedantic)?;
            for_each(meta_data.headers.as_slice(), decompressed)?; // allows returning `Error::Abort`
        }
    }

    debug_assert_eq!(processed_chunk_count, total_chunk_count, "some chunks were not read");
    on_progress(1.0);
    Ok(())
}

/// Read all chunks without seeking.
/// Returns the meta data, number of chunks, and a compressed chunk reader.
/// Does not buffer the reader, you should always pass a `BufReader`.
#[inline]
#[must_use]
pub fn read_all_compressed_chunks_from_buffered<'m>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    pedantic: bool
) -> Result<(MetaData, usize, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let mut read = PeekRead::new(Tracking::new(read));
    let meta_data = MetaData::read_validated_from_buffered_peekable(&mut read, pedantic)?;

    let mut remaining_chunk_count = {
        if pedantic {
            let offset_tables = MetaData::read_offset_tables(&mut read, &meta_data.headers)?;
            validate_offset_tables(meta_data.headers.as_slice(), &offset_tables, read.byte_position())?;
            offset_tables.iter().map(|table| table.len()).sum()
        }
        else {
            usize::try_from(MetaData::skip_offset_tables(&mut read, &meta_data.headers)?)
                .expect("too large chunk count for this machine")
        }
    };

    Ok((meta_data, remaining_chunk_count, move |meta_data| {
        if remaining_chunk_count > 0 {
            remaining_chunk_count -= 1;
            Some(Chunk::read(&mut read, meta_data))
        }
        else {
            if pedantic && read.peek_u8().is_ok() {
                return Some(Err(Error::invalid("end of file expected")));
            }

            None
        }
    }))
}


/// Read all desired chunks, possibly seeking. Skips all chunks that do not match the filter.
/// Returns the compressed chunks. Does not buffer the reader, you should always pass a `BufReader`.
/// This may leave you with an uninitialized image, if all chunks are filtered out.
// TODO this must be tested more
#[inline]
#[must_use]
pub fn read_filtered_chunks_from_buffered<'m, T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    new: impl FnOnce(&[Header]) -> Result<T>,
    filter: impl Fn(&T, (usize, &Header), (usize, &TileIndices)) -> bool,
    pedantic: bool
) -> Result<(MetaData, T, usize, impl FnMut(&'m MetaData) -> Option<Result<Chunk>>)>
{
    let skip_read = Tracking::new(read);
    let mut read = PeekRead::new(skip_read);

    let meta_data = MetaData::read_validated_from_buffered_peekable(&mut read, pedantic)?;
    let value = new(meta_data.headers.as_slice())?;

    let offset_tables = MetaData::read_offset_tables(&mut read, &meta_data.headers)?;

    // TODO regardless of pedantic, if invalid, read all chunks instead, and filter after reading each chunk?
    if pedantic {
        validate_offset_tables(meta_data.headers.as_slice(), &offset_tables, read.byte_position())?;
    }

    let mut filtered_offsets = Vec::with_capacity((meta_data.headers.len() * 32).min(2*2048));
    for (header_index, header) in meta_data.headers.iter().enumerate() { // offset tables are stored same order as headers
        for (block_index, block) in header.blocks_increasing_y_order().enumerate() { // in increasing_y order
            if filter(&value, (header_index, header), (block_index, &block)) {
                filtered_offsets.push(offset_tables[header_index][block_index]) // safe indexing from `enumerate()`
            }
        };
    }

    filtered_offsets.sort_unstable(); // enables reading continuously if possible (is probably already sorted)
    let mut filtered_offsets = filtered_offsets.into_iter();
    let block_count = filtered_offsets.len();

    Ok((meta_data, value, block_count, move |meta_data| {
        filtered_offsets.next().map(|offset|{
            read.skip_to(usize::try_from(offset).expect("too large chunk position for this machine"))?; // no-op for seek at current position, uses skip_bytes for small amounts
            Chunk::read(&mut read, meta_data)
        })
    }))
}

fn validate_offset_tables(headers: &[Header], offset_tables: &OffsetTables, chunks_start_byte: usize) -> UnitResult {
    let max_pixel_bytes: usize = headers.iter() // when compressed, chunks are smaller, but never larger than max
        .map(|header| header.max_pixel_file_bytes())
        .sum();

    // check that each offset is within the bounds
    let end_byte = chunks_start_byte + max_pixel_bytes;
    let is_invalid = offset_tables.iter().flatten().map(|&u64| u64_to_usize(u64))
        .any(|chunk_start| chunk_start < chunks_start_byte || chunk_start > end_byte);

    if is_invalid { Err(Error::invalid("offset table")) }
    else { Ok(()) }
}


/// Iterate over all uncompressed blocks of an image.
/// The image contents are collected by the `get_line` function parameter.
/// Returns blocks in `LineOrder::Increasing`, unless the line order is requested to be decreasing.
#[inline]
#[must_use]
pub fn uncompressed_image_blocks_ordered<'l>(
    headers: &'l [Header],
    get_block: &'l (impl 'l + Sync + (Fn(&[Header], BlockIndex) -> Vec<u8>)) // TODO reduce sync requirements, at least if parrallel is false
) -> impl 'l + Iterator<Item = Result<(usize, UncompressedBlock)>> + Send // TODO reduce sync requirements, at least if parrallel is false
{
    headers.iter().enumerate().flat_map(move |(layer_index, header)|{
        header.enumerate_ordered_blocks().map(move |(chunk_index, tile)|{
            let data_indices = header.get_absolute_block_pixel_coordinates(tile.location).expect("tile coordinate bug");

            let block_indices = BlockIndex {
                layer: layer_index, level: tile.location.level_index,
                pixel_position: data_indices.position.to_usize("data indices start").expect("data index bug"),
                pixel_size: data_indices.size,
            };

            let block_bytes = get_block(headers, block_indices);

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
    headers: &[Header], get_tile: impl Sync + Fn(&[Header], BlockIndex) -> Vec<u8>,
    parallel: bool, mut write_chunk: impl FnMut(usize, Chunk) -> UnitResult
) -> UnitResult
{
    let blocks = uncompressed_image_blocks_ordered(headers, &get_tile);

    let parallel = parallel && headers.iter() // do not use parallel stuff for uncompressed images
        .any(|header| header.compression != Compression::Uncompressed);

    if parallel {
        let requires_sorting = headers.iter()
            .any(|header| header.line_order != LineOrder::Unspecified);

        let (sender, receiver) = std::sync::mpsc::channel();

        blocks.par_bridge()
            .map(|result| Ok({
                let (chunk_index, block) = result?;
                let block = block.compress_to_chunk(headers)?;
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
            let mut expected_id_order = headers.iter().enumerate()
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
            let chunk = uncompressed_block.compress_to_chunk(headers)?;
            write_chunk(chunk_index, chunk)?;
        }
    }

    Ok(())
}


impl UncompressedBlock {

    /// Decompress the possibly compressed chunk and returns an `UncompressedBlock`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    #[inline]
    #[must_use]
    pub fn decompress_chunk(chunk: Chunk, meta_data: &MetaData, pedantic: bool) -> Result<Self> {
        let header: &Header = meta_data.headers.get(chunk.layer_index)
            .ok_or(Error::invalid("chunk layer index"))?;

        let tile_data_indices = header.get_block_data_indices(&chunk.block)?;
        let absolute_indices = header.get_absolute_block_pixel_coordinates(tile_data_indices)?;

        absolute_indices.validate(Some(header.layer_size))?;

        match chunk.block {
            Block::Tile(TileBlock { compressed_pixels, .. }) |
            Block::ScanLine(ScanLineBlock { compressed_pixels, .. }) => {
                Ok(UncompressedBlock {
                    data: header.compression.decompress_image_section(header, compressed_pixels, absolute_indices, pedantic)?,
                    index: BlockIndex {
                        layer: chunk.layer_index,
                        pixel_position: absolute_indices.position.to_usize("data indices start")?,
                        level: tile_data_indices.level_index,
                        pixel_size: absolute_indices.size,
                    }
                })
            },

            _ => return Err(Error::unsupported("deep data not supported yet"))
        }
    }

    /// Consume this block by compressing it, returning a `Chunk`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    #[inline]
    #[must_use]
    pub fn compress_to_chunk(self, headers: &[Header]) -> Result<Chunk> {
        let UncompressedBlock { data, index } = self;

        let header: &Header = headers.get(index.layer)
            .expect("block layer index bug");

        let expected_byte_size = header.channels.bytes_per_pixel * self.index.pixel_size.area(); // TODO sampling??
        if expected_byte_size != data.len() {
            panic!("get_line byte size should be {} but was {}", expected_byte_size, data.len());
        }

        let tile_coordinates = TileCoordinates {
            // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
            tile_index: index.pixel_position / header.max_block_pixel_size(), // TODO sampling??
            level_index: index.level,
        };

        let absolute_indices = header.get_absolute_block_pixel_coordinates(tile_coordinates)?;
        absolute_indices.validate(Some(header.layer_size))?;

        if !header.compression.may_loose_data() { debug_assert_eq!(
            &header.compression.decompress_image_section(
                header,
                header.compression.compress_image_section(header, data.clone(), absolute_indices)?,
                absolute_indices,
                true
            ).unwrap(),
            &data,
            "compression method not round trippin'"
        ); }

        let compressed_data = header.compression.compress_image_section(header, data, absolute_indices)?;

        Ok(Chunk {
            layer_index: index.layer,
            block : match header.blocks {
                Blocks::ScanLines => Block::ScanLine(ScanLineBlock {
                    compressed_pixels: compressed_data,

                    // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
                    y_coordinate: usize_to_i32(index.pixel_position.y()) + header.own_attributes.layer_position.y(), // TODO sampling??
                }),

                Blocks::Tiles(_) => Block::Tile(TileBlock {
                    compressed_pixels: compressed_data,
                    coordinates: tile_coordinates,
                }),
            }
        })
    }


    // TODO make iterator
    /// Call a closure for each line of samples in this uncompressed block.
    pub fn for_lines(
        &self, header: &Header,
        mut accept_line: impl FnMut(LineRef<'_>) -> UnitResult
    ) -> UnitResult {
        for (bytes, line) in LineIndex::lines_in_block(self.index, header) {
            let line_ref = LineSlice { location: line, value: &self.data[bytes] };
            accept_line(line_ref)?;
        }

        Ok(())
    }

    // TODO from iterator??
    /// Create an uncompressed block byte vector by requesting one line of samples after another.
    pub fn collect_block_from_lines(
        header: &Header, block_index: BlockIndex,
        mut extract_line: impl FnMut(LineRefMut<'_>)
    ) -> Vec<u8> {
        let byte_count = block_index.pixel_size.area() * header.channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; byte_count];

        for (byte_range, line_index) in LineIndex::lines_in_block(block_index, header) {
            extract_line(LineRefMut { // TODO subsampling
                value: &mut block_bytes[byte_range],
                location: line_index,
            });
        }

        block_bytes
    }

    // TODO from iterator??
    /// Create an uncompressed block by requesting one line of samples after another.
    pub fn from_lines(
        header: &Header, block_index: BlockIndex,
        extract_line: impl FnMut(LineRefMut<'_>)
    ) -> Self {
        Self {
            index: block_index,
            data: Self::collect_block_from_lines(header, block_index, extract_line)
        }
    }
}