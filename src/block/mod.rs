//! Handle compressed and uncompressed pixel byte blocks. Includes compression and decompression,
//! and reading a complete image into blocks.

pub mod lines;
pub mod samples;
pub mod chunk;

use crate::compression::{ByteVec, Compression};
use crate::math::*;
use crate::error::{Result, Error, usize_to_i32, UnitResult, u64_to_usize, usize_to_u64, IoError};
use crate::meta::{MetaData, BlockDescription, OffsetTables, Headers};
use crate::block::chunk::{Chunk, Block, TileBlock, ScanLineBlock, TileCoordinates};
use crate::meta::attribute::{LineOrder, ChannelList};
use smallvec::alloc::collections::{BTreeMap};
use std::convert::TryFrom;
use crate::io::{Tracking, PeekRead, Write, Data};
use std::io::{Seek, Read};
use crate::meta::header::Header;
use crate::block::lines::{LineRef, LineIndex, LineSlice, LineRefMut};
use smallvec::alloc::sync::Arc;
use std::iter::Peekable;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::any::Any;
use std::fmt::Debug;
use std::ops::Not;


/// Specifies where a block of pixel data should be placed in the actual image.
/// This is a globally unique identifier which
/// includes the layer, level index, and pixel location.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub struct BlockIndex {

    /// Index of the layer.
    pub layer: usize,

    /// Index of the bottom left pixel from the block within the data window.
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

/// Decode the meta data from a byte source, keeping the source ready for further reading.
/// Continue decoding the remaining bytes by calling `filtered_chunks` or `all_chunks`.
#[derive(Debug)]
pub struct Reader<R> {
    meta_data: MetaData,
    remaining_reader: PeekRead<Tracking<R>>, // TODO does R need to be Seek or is Tracking enough?
}

impl<R: Read + Seek> Reader<R> {

    /// Start the reading process.
    /// Immediately decodes the meta data into an internal field.
    /// Access it via`meta_data()`.
    pub fn read_from_buffered(read: R, pedantic: bool) -> Result<Self> {
        let mut remaining_reader = PeekRead::new(Tracking::new(read));
        let meta_data = MetaData::read_validated_from_buffered_peekable(&mut remaining_reader, pedantic)?;
        Ok(Self { meta_data, remaining_reader })
    }

    // must not be mutable, as reading the file later on relies on the meta data
    /// The decoded exr meta data from the file.
    pub fn meta_data(&self) -> &MetaData { &self.meta_data }

    /// The decoded exr meta data from the file.
    pub fn headers(&self) -> &[Header] { &self.meta_data.headers }

    /// Obtain the meta data ownership.
    pub fn into_meta_data(self) -> MetaData { self.meta_data }

    /// Prepare to read all the chunks from the file.
    /// Does not decode the chunks now, but returns a decoder.
    /// Reading all chunks reduces seeking the file, but some chunks might be read without being used.
    pub fn all_chunks(mut self, pedantic: bool) -> Result<AllChunksReader<R>> {
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
            remaining_bytes: self.remaining_reader,
            pedantic
        })
    }

    /// Prepare to read some the chunks from the file.
    /// Does not decode the chunks now, but returns a decoder.
    /// Reading only some chunks may seeking the file, potentially skipping many bytes.
    // TODO tile indices add no new information to block index??
    pub fn filter_chunks(mut self, pedantic: bool, mut filter: impl FnMut(&MetaData, TileCoordinates, BlockIndex) -> bool) -> Result<FilteredChunksReader<R>> {
        let offset_tables = MetaData::read_offset_tables(&mut self.remaining_reader, &self.meta_data.headers)?;

        // TODO regardless of pedantic, if invalid, read all chunks instead, and filter after reading each chunk?
        if pedantic {
            validate_offset_tables(
                self.meta_data.headers.as_slice(), &offset_tables,
                self.remaining_reader.byte_position()
            )?;
        }

        let mut filtered_offsets = Vec::with_capacity(
            (self.meta_data.headers.len() * 32).min(2*2048)
        );

        // TODO detect whether the filter actually would skip chunks, and aviod sorting etc when not filtering is applied

        for (header_index, header) in self.meta_data.headers.iter().enumerate() { // offset tables are stored same order as headers
            for (block_index, tile) in header.blocks_increasing_y_order().enumerate() { // in increasing_y order
                let data_indices = header.get_absolute_block_pixel_coordinates(tile.location)?;

                let block = BlockIndex {
                    layer: header_index,
                    level: tile.location.level_index,
                    pixel_position: data_indices.position.to_usize("data indices start")?,
                    pixel_size: data_indices.size,
                };

                if filter(&self.meta_data, tile.location, block) {
                    filtered_offsets.push(offset_tables[header_index][block_index]) // safe indexing from `enumerate()`
                }
            };
        }

        filtered_offsets.sort_unstable(); // enables reading continuously if possible (already sorted where line order increasing)

        if pedantic {
            // table is sorted. if any two neighbours are equal, we have duplicates. this is invalid.
            if filtered_offsets.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(Error::invalid("chunk offset table"))
            }
        }

        Ok(FilteredChunksReader {
            meta_data: self.meta_data,
            expected_filtered_chunk_count: filtered_offsets.len(),
            remaining_filtered_chunk_indices: filtered_offsets.into_iter(),
            remaining_bytes: self.remaining_reader
        })
    }
}

/// Decode the desired chunks and skip the unimportant chunks in the file.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Call `on_progress` to have a callback with each block.
/// Also contains the image meta data.
#[derive(Debug)]
pub struct FilteredChunksReader<R> {
    meta_data: MetaData,
    expected_filtered_chunk_count: usize,
    remaining_filtered_chunk_indices: std::vec::IntoIter<u64>,
    remaining_bytes: PeekRead<Tracking<R>>,
}

/// Decode all chunks in the file without seeking.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Call `on_progress` to have a callback with each block.
/// Also contains the image meta data.
#[derive(Debug)]
pub struct AllChunksReader<R> {
    meta_data: MetaData,
    remaining_chunks: std::ops::Range<usize>,
    remaining_bytes: PeekRead<Tracking<R>>,
    pedantic: bool,
}

/// Decode chunks in the file without seeking.
/// Calls the supplied closure for each chunk.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Also contains the image meta data.
#[derive(Debug)]
pub struct OnProgressChunksReader<R, F> {
    chunks_reader: R,
    decoded_chunks: usize,
    callback: F,
}

/// Decode chunks in the file.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Call `on_progress` to have a callback with each block.
/// Also contains the image meta data.
pub trait ChunksReader: Sized + Iterator<Item=Result<Chunk>> + ExactSizeIterator {

    /// The decoded exr meta data from the file.
    fn meta_data(&self) -> &MetaData;

    /// The decoded exr headers from the file.
    fn headers(&self) -> &[Header] { &self.meta_data().headers }

    /// The number of chunks that this reader will return in total.
    /// Can be less than the total number of chunks in the file, if some chunks are skipped.
    fn expected_chunk_count(&self) -> usize;

    /// Read the next compressed chunk from the file.
    /// Equivalent to `.next()`, as this also is an iterator.
    /// Returns `None` if all chunks have been read.
    fn read_next_chunk(&mut self) -> Option<Result<Chunk>> { self.next() }

    /// Create a new reader that calls the provided progress
    /// callback for each chunk that is read from the file.
    /// If the file can be successfully decoded,
    /// the progress will always at least once include 0.0 at the start and 1.0 at the end.
    fn on_progress<F>(self, on_progress: F) -> OnProgressChunksReader<Self, F> where F: FnMut(f64) {
        OnProgressChunksReader { chunks_reader: self, callback: on_progress, decoded_chunks: 0 }
    }

    /// Decompress all blocks in the file, using multiple cpu cores, and call the supplied closure for each block.
    /// The order of the blocks is not deterministic.
    /// You can also use `parallel_decompressor` to obtain an iterator instead.
    // FIXME try async + futures instead of rayon! Maybe even allows for external async decoding? (-> impl Stream<UncompressedBlock>)
    fn decompress_parallel(
        mut self, pedantic: bool,
        mut insert_block: impl FnMut(&MetaData, UncompressedBlock) -> UnitResult
    ) -> UnitResult
    {
        let mut decompressor = match self.parallel_decompressor(pedantic) {
            Err(old_self) => return old_self.decompress_sequential(pedantic, insert_block),
            Ok(decompressor) => decompressor,
        };

        while let Some(block) = decompressor.next() {
            insert_block(decompressor.meta_data(), block?)?;
        }

        debug_assert_eq!(decompressor.len(), 0);
        Ok(())
    }

    /// Return an iterator that decompresses the chunks with multiple threads.
    /// The order of the blocks is not deterministic.
    /// Use `ParallelBlockDecompressor::new` if you want to use your own thread pool.
    /// By default, this uses as many threads as there are CPUs.
    /// Returns the `self` if the sequential compressor should be used
    /// (due to thread pool errors or no need for parallel decompression).
    fn parallel_decompressor(self, pedantic: bool) -> std::result::Result<ParallelBlockDecompressor<Self>, Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .panic_handler(move |_anything_and_nothing| {
                eprintln!("OpenEXR decompressor thread panicked (maybe a debug assertion failed - use non-parallel decompression to see panic messages)");
                let _ = std::io::stdout().flush();

                panic!(); // abort now. TODO instead, do not recv() block forever (recv_timeout?) maybe send(unwind_catch(decompress))?
            })

            // todo no more threads than remaining block count (self.len())
            .build();

        let pool = match pool {
            Ok(pool) => pool,
            Err(_) => return Err(self),
        };

        ParallelBlockDecompressor::new(self, pedantic, pool)
    }

    /// Return an iterator that decompresses the chunks in this thread.
    /// You can alternatively use `sequential_decompressor` if you prefer an external iterator.
    fn decompress_sequential(
        self, pedantic: bool,
        mut insert_block: impl FnMut(&MetaData, UncompressedBlock) -> UnitResult
    ) -> UnitResult
    {
        let mut decompressor = self.sequential_decompressor(pedantic);
        while let Some(block) = decompressor.next() {
            insert_block(decompressor.meta_data(), block?)?;
        }

        debug_assert_eq!(decompressor.len(), 0);
        Ok(())
    }

    /// Prepare reading the chunks sequentially, only a single thread, but with less memory overhead.
    fn sequential_decompressor(self, pedantic: bool) -> SequentialBlockDecompressor<Self> {
        SequentialBlockDecompressor { remaining_chunks_reader: self, pedantic }
    }
}

impl<R, F> ChunksReader for OnProgressChunksReader<R, F> where R: ChunksReader, F: FnMut(f64) {
    fn meta_data(&self) -> &MetaData { self.chunks_reader.meta_data() }
    fn expected_chunk_count(&self) -> usize { self.chunks_reader.expected_chunk_count() }
}

impl<R, F> ExactSizeIterator for OnProgressChunksReader<R, F> where R: ChunksReader, F: FnMut(f64) {}
impl<R, F> Iterator for OnProgressChunksReader<R, F> where R: ChunksReader, F: FnMut(f64) {
    type Item = Result<Chunk>;

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks_reader.next().map(|item|{
            {
                let total_chunks = self.expected_chunk_count() as f64;
                let callback = &mut self.callback;
                callback(self.decoded_chunks as f64 / total_chunks);
            }

            self.decoded_chunks += 1;
            item
        })
        .or_else(||{
            debug_assert_eq!(self.decoded_chunks, self.expected_chunk_count());
            let callback = &mut self.callback;
            callback(1.0);
            None
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.chunks_reader.size_hint()
    }
}

impl<R: Read + Seek> ChunksReader for AllChunksReader<R> {
    fn meta_data(&self) -> &MetaData { &self.meta_data }
    fn expected_chunk_count(&self) -> usize { self.remaining_chunks.end }
}

impl<R: Read + Seek> ExactSizeIterator for AllChunksReader<R> {}
impl<R: Read + Seek> Iterator for AllChunksReader<R> {
    type Item = Result<Chunk>;

    fn next(&mut self) -> Option<Self::Item> {
        // read as many chunks as the file should contain (inferred from meta data)
        let next_chunk = self.remaining_chunks.next()
            .map(|_| Chunk::read(&mut self.remaining_bytes, &self.meta_data));

        // if no chunks are left, but some bytes remain, return error
        if self.pedantic && next_chunk.is_none() && self.remaining_bytes.peek_u8().is_ok() {
            return Some(Err(Error::invalid("end of file expected")));
        }

        next_chunk
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining_chunks.len(), Some(self.remaining_chunks.len()))
    }
}

impl<R: Read + Seek> ChunksReader for FilteredChunksReader<R> {
    fn meta_data(&self) -> &MetaData { &self.meta_data }
    fn expected_chunk_count(&self) -> usize { self.expected_filtered_chunk_count }
}

impl<R: Read + Seek> ExactSizeIterator for FilteredChunksReader<R> {}
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

        // TODO remember last chunk index and then seek to index+size and check whether bytes are left?
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining_filtered_chunk_indices.len(), Some(self.remaining_filtered_chunk_indices.len()))
    }
}

/// Read all chunks from the file, decompressing each chunk immediately.
#[derive(Debug)]
pub struct SequentialBlockDecompressor<R: ChunksReader> {
    remaining_chunks_reader: R,
    pedantic: bool,
}

impl<R: ChunksReader> SequentialBlockDecompressor<R> {

    /// The extracted meta data from the image file.
    pub fn meta_data(&self) -> &MetaData { self.remaining_chunks_reader.meta_data() }

    /// Read and then decompress a single block of pixels from the byte source.
    pub fn decompress_next_block(&mut self) -> Option<Result<UncompressedBlock>> {
        self.remaining_chunks_reader.read_next_chunk().map(|compressed_chunk|{
            UncompressedBlock::decompress_chunk(compressed_chunk?, &self.remaining_chunks_reader.meta_data(), self.pedantic)
        })
    }
}

/// Decompress the chunks in a file in parallel.
/// The first call to `next` will fill the thread pool with jobs,
/// starting to decompress the next few blocks.
/// These jobs will finish, even if you stop reading more blocks.
pub struct ParallelBlockDecompressor<R: ChunksReader> {
    remaining_chunks: R,
    sender: std::sync::mpsc::Sender<Result<UncompressedBlock>>,
    receiver: std::sync::mpsc::Receiver<Result<UncompressedBlock>>,
    currently_decompressing_count: usize,
    max_threads: usize,

    shared_meta_data_ref: Arc<MetaData>,
    pedantic: bool,

    pool: rayon::ThreadPool,
}

impl<R: ChunksReader> ParallelBlockDecompressor<R> {

    /// Create a new decompressor. Does not immediately spawn any tasks.
    /// Decompression starts after the first call to `next`.
    /// Returns the chunks if parallel decompression should not be used.
    pub fn new(chunks: R, pedantic: bool, pool: ThreadPool) -> std::result::Result<Self, R> {
        if chunks.meta_data().headers.iter()
            .all(|head|head.compression == Compression::Uncompressed)
        {
            return Err(chunks);
        }

        let max_threads = pool.current_num_threads().max(1).min(chunks.len()) + 2; // ca one block for each thread at all times

        let (send, recv) = std::sync::mpsc::channel(); // TODO crossbeam
        Ok(Self {
            shared_meta_data_ref: Arc::new(chunks.meta_data().clone()),
            currently_decompressing_count: 0,
            remaining_chunks: chunks,
            sender: send,
            receiver: recv,
            pedantic,
            max_threads,

            pool,
        })
    }

    /// Fill the pool with decompression jobs. Returns the first job that finishes.
    pub fn decompress_next_block(&mut self) -> Option<Result<UncompressedBlock>> {
        // if self.remaining_chunk_count == 0 { return None; }

        while self.currently_decompressing_count < self.max_threads {
            let block = self.remaining_chunks.next();
            if let Some(block) = block {
                let block = match block {
                    Ok(block) => block,
                    Err(error) => return Some(Err(error))
                };

                // TODO if no compression, return directly
                /*if self.meta_data().headers.get(block.layer_index)
                    .ok_or_else(|| Error::invalid("header index in block"))?
                    .compression == Compression::Uncompressed
                {
                    if self.remaining_chunk_count > 0 {
                        let next = self.remaining_chunks.next();
                        if next.is_some() { self.remaining_chunk_count -= 1; }
                        return UncompressedBlock::decompress(next, headers); // no actual compression, as data is uncompressed
                    }
                }*/


                let sender = self.sender.clone();
                let meta = self.shared_meta_data_ref.clone();
                let pedantic = self.pedantic;

                self.currently_decompressing_count += 1;

                self.pool.spawn(move || {
                    let decompressed_or_err = UncompressedBlock::decompress_chunk(
                        block, &meta, pedantic
                    );

                    // by now, decompressing could have failed in another thread.
                    // the error is then already handled, so we simply
                    // don't send the decompressed block and do nothing
                    let _ = sender.send(decompressed_or_err);
                });
            }
            else {
                // there are no chunks left to decompress
                break;
            }
        }

        if self.currently_decompressing_count > 0 {
            let next = self.receiver.recv()
                .expect("all decompressing senders hung up but more messages were expected");

            self.currently_decompressing_count -= 1;
            Some(next)
        }
        else {
            debug_assert!(self.receiver.try_recv().is_err(), "uncompressed chunks left in channel after decompressing all chunks"); // TODO not reliable
            debug_assert_eq!(self.len(), 0, "compressed chunks left after decompressing all chunks");
            None
        }
    }

    /// The extracted meta data of the image file.
    pub fn meta_data(&self) -> &MetaData { self.remaining_chunks.meta_data() }
}

impl<R: ChunksReader> ExactSizeIterator for SequentialBlockDecompressor<R> {}
impl<R: ChunksReader> Iterator for SequentialBlockDecompressor<R> {
    type Item = Result<UncompressedBlock>;
    fn next(&mut self) -> Option<Self::Item> { self.decompress_next_block() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.remaining_chunks_reader.size_hint() }
}

impl<R: ChunksReader> ExactSizeIterator for ParallelBlockDecompressor<R> {}
impl<R: ChunksReader> Iterator for ParallelBlockDecompressor<R> {
    type Item = Result<UncompressedBlock>;
    fn next(&mut self) -> Option<Self::Item> { self.decompress_next_block() }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining_chunks.len() + self.currently_decompressing_count;
        (remaining, Some(remaining))
    }
}






/// Write an exr file by writing one chunk after another in a closure.
/// In the closure, you are provided a chunk writer, which should be used to write all the chunks.
/// Assumes the your write destination is buffered.
pub fn write_chunks_with<W: Write + Seek>(
    buffered_write: W, headers: Headers, pedantic: bool,
    write_chunks: impl FnOnce(MetaData, &mut ChunkWriter<W>) -> UnitResult
) -> UnitResult {
    // this closure approach ensures that after writing all chunks, the file is always completed and checked and flushed
    let (meta, mut writer) = ChunkWriter::new_for_buffered(buffered_write, headers, pedantic)?;
    write_chunks(meta, &mut writer)?;
    writer.complete_meta_data()
}

/// Can consume compressed pixel chunks, writing them a file.
/// Use `as_blocks_writer` to compress your data.
/// Use `on_progress` to obtain a new writer
/// that triggers a callback for each block.
// #[must_use]
#[derive(Debug)]
#[must_use]
pub struct ChunkWriter<W> {
    header_count: usize,
    byte_writer: Tracking<W>,
    chunk_indices_byte_location: std::ops::Range<usize>,
    chunk_indices_increasing_y: OffsetTables,
    chunk_count: usize, // TODO compose?
}

/// A new writer that triggers a callback
/// for each block written to the inner writer.
#[derive(Debug)]
#[must_use]
pub struct OnProgressChunkWriter<'w, W, F> {
    chunk_writer: &'w mut W,
    written_chunks: usize,
    on_progress: F,
}

/// Write chunks to a byte destination.
/// Then write each chunk with `writer.write_chunk(chunk)`.
pub trait ChunksWriter: Sized {

    /// The total number of chunks that the complete file will contain.
    fn total_chunks_count(&self) -> usize;

    /// Any more calls will result in an error and have no effect.
    /// If writing results in an error, the file and the writer
    /// may remain in an invalid state and should not be used further.
    /// Errors when the chunk at this index was already written.
    fn write_chunk(&mut self, index_in_header_increasing_y: usize, chunk: Chunk) -> UnitResult;

    /// Obtain a new writer that calls the specified closure for each block that is written to this writer.
    fn on_progress<F>(&mut self, on_progress: F) -> OnProgressChunkWriter<'_, Self, F> where F: FnMut(f64) {
        OnProgressChunkWriter { chunk_writer: self, written_chunks: 0, on_progress }
    }

    /// Obtain a new writer that can compress blocks to chunks, which are then passed to this writer.
    fn sequential_blocks_compressor<'w>(&'w mut self, meta: &'w MetaData) -> SequentialBlocksCompressor<'w, Self> {
        SequentialBlocksCompressor::new(meta, self)
    }

    /// Obtain a new writer that can compress blocks to chunks on multiple threads, which are then passed to this writer.
    /// Returns none if the sequential compressor should be used instead (thread pool creation failure or too large performance overhead).
    fn parallel_blocks_compressor<'w>(&'w mut self, meta: &'w MetaData) -> Option<ParallelBlocksCompressor<'w, Self>> {
        let pool = rayon::ThreadPoolBuilder::new()
            .panic_handler(move |_anything_and_nothing| {
                eprintln!("OpenEXR decompressor thread panicked (maybe a debug assertion failed - use non-parallel decompression to see panic messages)");
                let _ = std::io::stdout().flush();

                panic!(); // abort now. TODO instead, do not recv() block forever (recv_timeout?) maybe send(unwind_catch(decompress))?
            })

            // todo no more threads than remaining block count (self.len())
            .build()
            .ok()?;

        ParallelBlocksCompressor::new(meta, self, pool)
    }

    /// Compresses all blocks to the file.
    /// The index of the block must be in increasing line order within the header.
    /// Obtain iterator with `MetaData::collect_ordered_blocks(...)` or similar methods.
    fn compress_all_blocks_sequential(mut self, meta: &MetaData, blocks: impl Iterator<Item=(usize, UncompressedBlock)>) -> UnitResult {
        let mut writer = self.sequential_blocks_compressor(meta);

        // TODO check block order if line order is not unspecified!
        for (index_in_header_increasing_y, block) in blocks {
            writer.compress_block(index_in_header_increasing_y, block)?;
        }

        // TODO debug_assert_eq!(self.is_complete());
        Ok(())
    }

    /// Compresses all blocks to the file.
    /// The index of the block must be in increasing line order within the header.
    /// Obtain iterator with `MetaData::collect_ordered_blocks(...)` or similar methods.
    fn compress_all_blocks_parallel(mut self, meta: &MetaData, blocks: impl Iterator<Item=(usize, UncompressedBlock)>) -> UnitResult {
        let mut parallel_writer = match self.parallel_blocks_compressor(meta) {
            None => return self.compress_all_blocks_sequential(meta, blocks),
            Some(writer) => writer,
        };

        // TODO check block order if line order is not unspecified!
        for (index_in_header_increasing_y, block) in blocks {
            parallel_writer.add_block_to_compression_queue(index_in_header_increasing_y, block)?;
        }

        // TODO debug_assert_eq!(self.is_complete());
        Ok(())
    }
}


impl<W> ChunksWriter for ChunkWriter<W> where W: Write + Seek {

    /// The total number of chunks that the complete file will contain.
    fn total_chunks_count(&self) -> usize { self.chunk_count }

    /// Any more calls will result in an error and have no effect.
    /// If writing results in an error, the file and the writer
    /// may remain in an invalid state and should not be used further.
    /// Errors when the chunk at this index was already written.
    fn write_chunk(&mut self, index_in_header_increasing_y: usize, chunk: Chunk) -> UnitResult {
        let header_chunk_indices = &mut self.chunk_indices_increasing_y[chunk.layer_index];

        if index_in_header_increasing_y >= header_chunk_indices.len() {
            return Err(Error::invalid("too large chunk index"));
        }

        let chunk_index_slot = &mut header_chunk_indices[index_in_header_increasing_y];
        if *chunk_index_slot != 0 {
            return Err(Error::invalid(format!("chunk at index {} is already written", index_in_header_increasing_y)));
        }

        *chunk_index_slot = usize_to_u64(self.byte_writer.byte_position());
        chunk.write(&mut self.byte_writer, self.header_count)?;
        Ok(())
    }
}

impl<W> ChunkWriter<W> where W: Write + Seek {
    // -- the following functions are private, because they must be called in a strict order --

    /// Writes the meta data and zeroed offset tables as a placeholder.
    fn new_for_buffered(buffered_byte_writer: W, headers: Headers, pedantic: bool) -> Result<(MetaData, Self)> {
        let mut write = Tracking::new(buffered_byte_writer);
        let requirements = MetaData::write_validating_to_buffered(&mut write, headers.as_slice(), pedantic)?;

        // TODO: use increasing line order where possible, but this requires us to know whether we want to be parallel right now
        /*// if non-parallel compression, we always use increasing order anyways
        if !parallel || !has_compression {
            for header in &mut headers {
                if header.line_order == LineOrder::Unspecified {
                    header.line_order = LineOrder::Increasing;
                }
            }
        }*/

        let offset_table_size: usize = headers.iter().map(|header| header.chunk_count).sum();

        let offset_table_start_byte = write.byte_position();
        let offset_table_end_byte = write.byte_position() + offset_table_size * u64::BYTE_SIZE;

        // skip offset tables, filling with 0, will be updated after the last chunk has been written
        write.seek_write_to(offset_table_end_byte)?;

        let header_count = headers.len();
        let chunk_indices_increasing_y = headers.iter()
            .map(|header| vec![0_u64; header.chunk_count]).collect();

        let meta_data = MetaData { requirements, headers };

        Ok((meta_data, ChunkWriter {
            header_count,
            byte_writer: write,
            chunk_count: offset_table_size,
            chunk_indices_byte_location: offset_table_start_byte .. offset_table_end_byte,
            chunk_indices_increasing_y,
        }))
    }

    /// Seek back to the meta data, write offset tables, and flush the byte writer.
    /// Leaves the writer seeked to the middle of the file.
    fn complete_meta_data(mut self) -> UnitResult {
        if self.chunk_indices_increasing_y.iter().flatten().any(|&index| index == 0) {
            return Err(Error::invalid("some chunks are not written yet"))
        }

        // write all offset tables
        debug_assert_ne!(self.byte_writer.byte_position(), self.chunk_indices_byte_location.end);
        self.byte_writer.seek_write_to(self.chunk_indices_byte_location.start)?;

        for table in self.chunk_indices_increasing_y {
            u64::write_slice(&mut self.byte_writer, table.as_slice())?;
        }

        self.byte_writer.flush()?; // make sure we catch all (possibly delayed) io errors before returning
        Ok(())
    }

}


impl<'w, W, F> ChunksWriter for OnProgressChunkWriter<'w, W, F> where W: 'w + ChunksWriter, F: FnMut(f64) {
    fn total_chunks_count(&self) -> usize {
        self.chunk_writer.total_chunks_count()
    }

    fn write_chunk(&mut self, index_in_header_increasing_y: usize, chunk: Chunk) -> UnitResult {
        let total_chunks = self.total_chunks_count();
        let on_progress = &mut self.on_progress;

        // guarantee on_progress being called with 0 once
        if self.written_chunks == 0 { on_progress(0.0); }

        self.chunk_writer.write_chunk(index_in_header_increasing_y, chunk)?;

        self.written_chunks += 1;

        on_progress({
            // guarantee finishing with progress 1.0 for last block at least once, float division might slightly differ from 1.0
            if self.written_chunks == total_chunks { 1.0 }
            else { self.written_chunks as f64 / total_chunks as f64 }
        });

        Ok(())
    }
}


/// Write blocks that appear in any order and reorder them before writing.
#[derive(Debug)]
#[must_use]
pub struct SortedBlocksWriter<'w, W> {
    chunk_writer: &'w mut W,
    pending_chunks: BTreeMap<usize, (usize, Chunk)>,
    unwritten_chunk_indices: Peekable<std::ops::Range<usize>>,
    requires_sorting: bool, // using this instead of Option, because of borrowing
}


impl<'w, W> SortedBlocksWriter<'w, W> where W: ChunksWriter {

    /// New sorting writer. Returns `None` if sorting is not required.
    pub fn new(meta_data: &MetaData, chunk_writer: &'w mut W) -> SortedBlocksWriter<'w, W> {
        let requires_sorting = meta_data.headers.iter()
            .any(|header| header.line_order != LineOrder::Unspecified);

        let total_chunk_count = chunk_writer.total_chunks_count();

        SortedBlocksWriter {
            pending_chunks: BTreeMap::new(),
            unwritten_chunk_indices: (0 .. total_chunk_count).peekable(),
            requires_sorting,
            chunk_writer
        }
    }

    /// Write the chunk or stash it. In the closure, write all chunks that can be written now.
    pub fn write_or_stash_chunk(&mut self, chunk_index_in_file: usize, chunk_y_index: usize, chunk: Chunk) -> UnitResult {
        if self.requires_sorting.not() {
            return self.chunk_writer.write_chunk(chunk_y_index, chunk);
        }

        // write this chunk now if possible
        if self.unwritten_chunk_indices.peek() == Some(&chunk_index_in_file){
            self.chunk_writer.write_chunk(chunk_y_index, chunk)?;
            self.unwritten_chunk_indices.next().expect("peeked chunk index is missing");

            // write all pending blocks that are immediate successors of this block
            while let Some((next_chunk_y_index, next_chunk)) = self
                .unwritten_chunk_indices.peek().cloned()
                .and_then(|id| self.pending_chunks.remove(&id))
            {
                self.chunk_writer.write_chunk(next_chunk_y_index, next_chunk)?;
                self.unwritten_chunk_indices.next().expect("peeked chunk index is missing");
            }
        }

        else {
            // the argument block is not to be written now,
            // and all the pending blocks are not next up either,
            // so just stash this block
            self.pending_chunks.insert(chunk_index_in_file, (chunk_y_index, chunk));
        }

        Ok(())
    }

    pub fn inner_chunks_writer(&self) -> &W {
        &self.chunk_writer
    }
}



/// Compress blocks to a chunk writer in this thread.
#[derive(Debug)]
#[must_use]
pub struct SequentialBlocksCompressor<'w, W> {
    meta: &'w MetaData,
    chunks_writer: &'w mut W,
}

impl<'w, W> SequentialBlocksCompressor<'w, W> where W: 'w + ChunksWriter {

    /// New blocks writer.
    pub fn new(meta: &'w MetaData, chunks_writer: &'w mut W) -> Self { Self { meta, chunks_writer, } }

    /// This is where the compressed blocks are written to.
    pub fn inner_chunks_writer(&'w self) -> &'w W { self.chunks_writer }

    /// Compress a single block immediately. The index of the block must be in increasing line order.
    pub fn compress_block(&mut self, index_in_header_increasing_y: usize, block: UncompressedBlock) -> UnitResult {
        self.chunks_writer.write_chunk(
            index_in_header_increasing_y,
            block.compress_to_chunk(&self.meta.headers)?
        )
    }
}

/// Compress blocks to a chunk writer with multiple threads.
#[derive(Debug)]
#[must_use]
pub struct ParallelBlocksCompressor<'w, W> {
    meta: &'w MetaData,
    sorted_writer: SortedBlocksWriter<'w, W>,

    sender: std::sync::mpsc::Sender<Result<(usize, usize, Chunk)>>,
    receiver: std::sync::mpsc::Receiver<Result<(usize, usize, Chunk)>>,
    shared_meta_data_ref: Arc<MetaData>,
    pool: rayon::ThreadPool,

    currently_compressing_count: usize,
    written_chunk_count: usize, // used to check for last chunk
    max_threads: usize,
    next_incoming_chunk_index: usize, // used to remember original chunk order
}

impl<'w, W> ParallelBlocksCompressor<'w, W> where W: 'w + ChunksWriter {

    /// New blocks writer. Returns none if sequential compression should be used.
    pub fn new(meta: &'w MetaData, chunks_writer: &'w mut W, pool: ThreadPool) -> Option<Self> {
        if meta.headers.iter().all(|head|head.compression == Compression::Uncompressed) {
            return None;
        }

        let max_threads = pool.current_num_threads().max(1).min(chunks_writer.total_chunks_count()) + 2; // ca one block for each thread at all times
        let (send, recv) = std::sync::mpsc::channel(); // TODO crossbeam

        Some(Self {
            sorted_writer: SortedBlocksWriter::new(meta, chunks_writer),
            shared_meta_data_ref: Arc::new(meta.clone()),
            next_incoming_chunk_index: 0,
            currently_compressing_count: 0,
            written_chunk_count: 0,
            sender: send,
            receiver: recv,
            max_threads,
            pool,
            meta,
        })
    }

    /// This is where the compressed blocks are written to.
    pub fn inner_chunks_writer(&'w self) -> &'w W { self.sorted_writer.inner_chunks_writer() }

    // private, as may underflow counter in release mode
    fn write_next_decompressed_chunk(&mut self) -> UnitResult {
        debug_assert!(self.currently_compressing_count > 0);

        let some_compressed_chunk = self.receiver.recv()
            .expect("cannot receive compressed block");

        self.currently_compressing_count -= 1;
        let (chunk_file_index, chunk_y_index, chunk) = some_compressed_chunk?;
        self.sorted_writer.write_or_stash_chunk(chunk_file_index, chunk_y_index, chunk)?;

        self.written_chunk_count += 1;
        Ok(())
    }

    /// Wait until all currently compressing chunks in the compressor have been written.
    pub fn write_all_decompressing_chunks(&mut self) -> UnitResult {
        while self.currently_compressing_count > 0 {
            self.write_next_decompressed_chunk()?;
        }

        debug_assert_eq!(self.currently_compressing_count, 0);
        Ok(())
    }

    /// Add a single block to the compressor queue. The index of the block must be in increasing line order.
    /// When calling this function for the last block, this method waits until all the blocks have been written.
    /// This only works when you write as many blocks as the image expects, otherwise you can use `wait_for_all_remaining_chunks`.
    /// Waits for a block from the queue to be written, if the queue already has enough items.
    pub fn add_block_to_compression_queue(&mut self, index_in_header_increasing_y: usize, block: UncompressedBlock) -> UnitResult {

        // if pipe is full, block to wait for a slot to free up
        if self.currently_compressing_count >= self.max_threads {
            self.write_next_decompressed_chunk()?;
        }

        // add the argument chunk to the compression queueue
        let index_in_file = self.next_incoming_chunk_index;
        let sender = self.sender.clone();
        let meta = self.meta.clone();

        self.pool.spawn(move ||{
            let compressed_or_err = block.compress_to_chunk(&meta.headers);

            // by now, decompressing could have failed in another thread.
            // the error is then already handled, so we simply
            // don't send the decompressed block and do nothing
            let _ = sender.send(compressed_or_err.map(move |compressed| (index_in_file, index_in_header_increasing_y, compressed)));
        });

        self.currently_compressing_count += 1;
        self.next_incoming_chunk_index += 1;

        // if this is the last chunk, wait for all chunks to complete before returning
        if self.written_chunk_count + self.currently_compressing_count == self.inner_chunks_writer().total_chunks_count() {
            self.write_all_decompressing_chunks()?;
            debug_assert_eq!(self.written_chunk_count, self.inner_chunks_writer().total_chunks_count());
        }

        Ok(())
    }
}



/// This iterator tells you the block indices of all blocks that must be in the image.
/// The order of the blocks depends on the `LineOrder` attribute
/// (unspecified line order is treated the same as increasing line order).
/// The blocks written to the file must be exactly in this order,
/// except for when the `LineOrder` is unspecified.
/// The index represents the block index, in increasing line order, within the header.
pub fn enumerate_ordered_header_block_indices(headers: &[Header]) -> impl '_ + Iterator<Item=(usize, BlockIndex)> {
    headers.iter().enumerate().flat_map(|(layer_index, header)|{
        header.enumerate_ordered_blocks().map(move |(index_in_header, tile)|{
            let data_indices = header.get_absolute_block_pixel_coordinates(tile.location).expect("tile coordinate bug");

            let block = BlockIndex {
                layer: layer_index,
                level: tile.location.level_index,
                pixel_position: data_indices.position.to_usize("data indices start").expect("data index bug"),
                pixel_size: data_indices.size,
            };

            (index_in_header, block)
        })
    })
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
                BlockDescription::ScanLines => Block::ScanLine(ScanLineBlock {
                    compressed_pixels: compressed_data,

                    // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
                    y_coordinate: usize_to_i32(index.pixel_position.y()) + header.own_attributes.layer_position.y(), // TODO sampling??
                }),

                BlockDescription::Tiles(_) => Block::Tile(TileBlock {
                    compressed_pixels: compressed_data,
                    coordinates: tile_coordinates,
                }),
            }
        })
    }

    pub fn lines(&self, channels: &ChannelList) -> impl Iterator<Item=LineRef<'_>> {
        LineIndex::lines_in_block(self.index, channels)
            .map(move |(bytes, line)| LineSlice { location: line, value: &self.data[bytes] })
    }

    /* TODO pub fn lines_mut<'s>(&'s mut self, header: &Header) -> impl 's + Iterator<Item=LineRefMut<'s>> {
        LineIndex::lines_in_block(self.index, &header.channels)
            .map(move |(bytes, line)| LineSlice { location: line, value: &mut self.data[bytes] })
    }*/

    /*// TODO make iterator
    /// Call a closure for each line of samples in this uncompressed block.
    pub fn for_lines(
        &self, header: &Header,
        mut accept_line: impl FnMut(LineRef<'_>) -> UnitResult
    ) -> UnitResult {
        for (bytes, line) in LineIndex::lines_in_block(self.index, &header.channels) {
            let line_ref = LineSlice { location: line, value: &self.data[bytes] };
            accept_line(line_ref)?;
        }

        Ok(())
    }*/

    // TODO from iterator??
    /// Create an uncompressed block byte vector by requesting one line of samples after another.
    pub fn collect_block_data_from_lines(
        channels: &ChannelList, block_index: BlockIndex,
        mut extract_line: impl FnMut(LineRefMut<'_>)
    ) -> Vec<u8>
    {
        let byte_count = block_index.pixel_size.area() * channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; byte_count];

        for (byte_range, line_index) in LineIndex::lines_in_block(block_index, channels) {
            extract_line(LineRefMut { // TODO subsampling
                value: &mut block_bytes[byte_range],
                location: line_index,
            });
        }

        block_bytes
    }

    /// Create an uncompressed block by requesting one line of samples after another.
    pub fn from_lines(
        channels: &ChannelList, block_index: BlockIndex,
        extract_line: impl FnMut(LineRefMut<'_>)
    ) -> Self {
        Self {
            index: block_index,
            data: Self::collect_block_data_from_lines(channels, block_index, extract_line)
        }
    }
}
