//! Handle compressed and uncompressed pixel byte blocks. Includes compression and decompression,
//! and reading a complete image into blocks.

pub mod lines;
pub mod samples;
pub mod chunk;

use crate::compression::{ByteVec, Compression};
use crate::math::*;
use crate::error::{Result, Error, usize_to_i32, UnitResult, u64_to_usize, usize_to_u64};
use crate::meta::{MetaData, BlockDescription, TileIndices, OffsetTables, Headers};
use crate::block::chunk::{Chunk, Block, TileBlock, ScanLineBlock, TileCoordinates};
use crate::meta::attribute::{LineOrder, ChannelList};
use rayon::prelude::*;
use smallvec::alloc::collections::{BTreeMap};
use std::convert::TryFrom;
use crate::io::{Tracking, PeekRead, Write, Data};
use std::io::{Seek, Read};
use crate::meta::header::Header;
use crate::block::lines::{LineRef, LineIndex, LineSlice, LineRefMut};
use smallvec::alloc::sync::Arc;
use std::iter::Peekable;
use std::ops::Not;
use smallvec::SmallVec;


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

/// Decode the meta data from a byte source, keeping the source ready for further reading.
/// Continue decoding the remaining bytes by calling `filtered_chunks` or `all_chunks`.
#[derive(Debug)]
pub struct MetaDataReader<R> {
    meta_data: MetaData,
    remaining_reader: PeekRead<Tracking<R>>, // TODO does R need to be Seek or is Tracking enough?
}

impl<R: Read + Seek> MetaDataReader<R> {

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
    pub fn filter_chunks(mut self, pedantic: bool, mut filter: impl FnMut((usize, &Header), (/*TODO BlockIndex*/usize, TileIndices)) -> bool) -> Result<FilteredChunksReader<R>> {
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
            for (block_index, block) in header.blocks_increasing_y_order().enumerate() { // in increasing_y order
                if filter((header_index, header), (block_index, block)) {
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
            filtered_chunk_count: filtered_offsets.len(),
            remaining_filtered_chunk_indices: filtered_offsets.into_iter(),
            remaining_bytes: self.remaining_reader
        })
    }
}

/// Decode the desired chunks and skip the unimportant chunks in the file.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Also contains the image meta data. Supports `on_progress`.
#[derive(Debug)]
pub struct FilteredChunksReader<R> {
    meta_data: MetaData,
    filtered_chunk_count: usize,
    remaining_filtered_chunk_indices: std::vec::IntoIter<u64>,
    remaining_bytes: PeekRead<Tracking<R>>,
}

/// Decode all chunks in the file without seeking.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Also contains the image meta data. Supports `on_progress`.
#[derive(Debug)]
pub struct AllChunksReader<R> {
    meta_data: MetaData,
    remaining_chunks: std::ops::Range<usize>,
    remaining_bytes: PeekRead<Tracking<R>>,
    pedantic: bool,
}

/// Decode all chunks in the file without seeking.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Also contains the image meta data and a callback for the progress.
#[derive(Debug)]
pub struct OnProgressChunksReader<R, F> {
    chunks_reader: R,
    decoded_chunks: usize,
    callback: F,
}

/// Decode all or some chunks in the file.
/// The decoded chunks can be decompressed by calling
/// `decompress_parallel`, `decompress_sequential`, or `sequential_decompressor`.
/// Also contains the image meta data.
pub trait ChunksReader: Sized + Iterator<Item=Result<Chunk>> + ExactSizeIterator {

    /// The decoded exr meta data from the file.
    fn meta_data(&self) -> &MetaData;

    /// The decoded exr headers from the file.
    fn headers(&self) -> &[Header] { &self.meta_data().headers }

    /// The number of chunks that this reader will return in total.
    /// Can be less than the total number of chunks in the file, if some chunks are skipped.
    fn chunk_count(&self) -> usize { self.len() }

    /// Read the next compressed chunk from the file. Equivalent to `.next()`, as this also is an iterator.
    fn read_next_chunk(&mut self) -> Option<Result<Chunk>> { self.next() }

    /// Create a new `ChunkReader` that triggers the provided progress
    /// callback for each chunk that is read from the file.
    /// If the file can be successfully decoded,
    /// the progress is guaranteed to include 0.0 at the start and 1.0 at the end.
    fn on_progress<F>(self, on_progress: F) -> OnProgressChunksReader<Self, F> where F: FnMut(f64) {
        OnProgressChunksReader { chunks_reader: self, callback: on_progress, decoded_chunks: 0 }
    }

    /// Decompress all blocks in the file, and accumulate the result using a `fold` operation.
    /// You provide the initial image value, based on the decoded meta data, and you specify
    /// how each block of pixels is inserted into the image.
    ///
    /// __WARNING__: This currently allocates way too much memory, probably twice the file size. Can't find a simple solution.
    // Requires the byte source (`file` or other `Read`) to also be `Send`.
    // Use `decompress_sequential`, or the `sequential_decompressor` if your byte source is not sendable.

    // FIXME try async + futures instead of rayon! Maybe even allows for external async decoding? (-> impl Stream<UncompressedBlock>)
    fn decompress_parallel(
        mut self, pedantic: bool,
        mut insert_block: impl FnMut(&MetaData, UncompressedBlock) -> UnitResult
    ) -> UnitResult
    {
        // do not use parallel procedure for uncompressed images
        let has_compression = self.meta_data().headers.iter().any(|header| header.compression != Compression::Uncompressed);
        if !has_compression || true /*FIXME*/ { return self.decompress_sequential(pedantic, insert_block); }

        #[allow(unused)]
        let mut remaining_chunks = self.chunk_count() as i64; // used for debug_assert

        let meta_data_arc = Arc::new(self.meta_data().clone());

        let pool = rayon::ThreadPoolBuilder::new().build().expect("thread error");

        let (send, recv) = std::sync::mpsc::channel(); // TODO crossbeam?
        let mut currently_running = 0;

        while let Some(chunk) = self.read_next_chunk() {
            while currently_running >= 12 {
                let decompressed = recv.recv().expect("thread error")?;
                insert_block(self.meta_data(), decompressed)?;
                currently_running -= 1;
                remaining_chunks -= 1;
            }

            let send = send.clone();
            let meta_data_arc = meta_data_arc.clone();
            currently_running += 1;

            pool.spawn(move || {
                let decompressed = chunk.and_then(|chunk| UncompressedBlock::decompress_chunk(chunk, &meta_data_arc, pedantic));
                send.send(decompressed).expect("thread error");
            });
        }

        while currently_running > 0 {
            let decompressed = recv.recv().expect("thread error")?;
            insert_block(self.meta_data(), decompressed)?;
            currently_running -= 1;
            remaining_chunks -= 1;
        }

        assert_eq!(remaining_chunks, 0);
        Ok(())
    }

    /// Decompress all blocks in the file, and accumulate the result using a `fold` operation.
    /// You provide the initial image value, based on the decoded meta data, and you specify
    /// how each block of pixels is inserted into the image.
    ///
    /// You can alternatively use `sequential_decompressor` if you prefer an external iterator.
    fn decompress_sequential(
        self, pedantic: bool,
        mut insert_block: impl FnMut(&MetaData, UncompressedBlock) -> UnitResult
    ) -> UnitResult
    {
        let mut decompressor = self.sequential_decompressor(pedantic);
        while let Some(block) = decompressor.next() {
            insert_block(decompressor.remaining_chunks_reader.meta_data(), block?)?;
        }

        Ok(())
    }

    /// Prepare reading the chunks sequentially, only a single thread, but with less memory overhead.
    fn sequential_decompressor(self, pedantic: bool) -> SequentialBlockReader<Self> {
        SequentialBlockReader { remaining_chunks_reader: self, pedantic }
    }
}

/*impl<W> FilteredChunksReader<W> where Self: ChunksReader {
    fn parallel_decompressor(self, pedantic: bool) -> impl futures::stream::Stream<Item=Result<UncompressedBlock>> {
        let meta = self.meta_data.clone();
        futures::stream::iter(self).map(move |chunk|
            UncompressedBlock::decompress_chunk(chunk?, &meta, pedantic)
        )
    }

    fn decompress_parallel(self, pedantic: bool) -> impl Iterator<Item=Result<UncompressedBlock>> {
        // self.parallel_decompressor(pedantic).try_fold()


        // futures::executor::ThreadPool::new().unwrap();
        // futures::executor::block_on_stream(self.parallel_decompressor(pedantic)).


        /*let pool = futures::executor::ThreadPool::new().unwrap();

        futures::executor::block_on(async move {
            let stream = self.parallel_decompressor(pedantic);
            for item in stream {
                let handle = pool.spawn_with_handle();
            }
        })*/

        futures::executor::block_on_stream(self.parallel_decompressor(pedantic))
    }
}*/

impl<R, F> ChunksReader for OnProgressChunksReader<R, F> where R: ChunksReader, F: FnMut(f64) {
    fn meta_data(&self) -> &MetaData { self.chunks_reader.meta_data() }
}

impl<R, F> ExactSizeIterator for OnProgressChunksReader<R, F> where R: ChunksReader, F: FnMut(f64) {}
impl<R, F> Iterator for OnProgressChunksReader<R, F> where R: ChunksReader, F: FnMut(f64) {
    type Item = Result<Chunk>;

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks_reader.next().map(|item|{
            {
                let total_chunks = self.chunk_count() as f64;
                let callback = &mut self.callback;
                callback(self.decoded_chunks as f64 / total_chunks);
            }

            self.decoded_chunks += 1;
            item
        }).or_else(||{
            debug_assert_eq!(self.decoded_chunks, self.chunk_count());
            let callback = &mut self.callback;
            callback(1.0);
            None
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) { self.chunks_reader.size_hint() }
}

impl<R: Read + Seek> ChunksReader for AllChunksReader<R> {
    fn meta_data(&self) -> &MetaData { &self.meta_data }
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
        (self.remaining_chunks.end, Some(self.remaining_chunks.end))
    }
}

impl<R: Read + Seek> ChunksReader for FilteredChunksReader<R> {
    fn meta_data(&self) -> &MetaData { &self.meta_data }
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
        (self.filtered_chunk_count, Some(self.filtered_chunk_count))
    }
}

/// Read all chunks from the file, decompressing each chunk immediately.
#[derive(Debug)]
pub struct SequentialBlockReader<R: ChunksReader> {
    remaining_chunks_reader: R,
    pedantic: bool,
}

impl<R: ChunksReader> SequentialBlockReader<R> {

    /// Read and then decompress a single block of pixels from the byte source.
    pub fn decompress_next_block(&mut self) -> Option<Result<UncompressedBlock>> {
        self.remaining_chunks_reader.read_next_chunk().map(|compressed_chunk|{
            UncompressedBlock::decompress_chunk(compressed_chunk?, &self.remaining_chunks_reader.meta_data(), self.pedantic)
        })
    }
}

/*pub struct ParallelBlockReader<R: ChunksReader> {
    remaining_chunks: R,
    sender: std::sync::mpsc::Sender<Result<UncompressedBlock>>,
    receiver: std::sync::mpsc::Receiver<Result<UncompressedBlock>>,
    currently_decompressing_count: usize,
}*/

/*impl<R: ChunksReader> ParallelBlockReader<R> {
    pub fn new(chunks: R) -> Self {
        let (send, recv) = std::sync::mpsc::channel(); // TODO crossbeam
        Self {
            remaining_chunks: chunks,
            sender: send,
            receiver: recv,
            currently_decompressing_count: 0
        }
    }

    pub fn decompress_next_block(&mut self) -> Option<Result<UncompressedBlock>> {
        let max_parallel_blocks = 12; // TODO num cpu cores?

        while self.currently_decompressing_count < max_parallel_blocks && self.remaining_chunks.peek().next().is_some() {
            let chunk = self.remaining_chunks.next()?; // fail fast if none left
            self.currently_decompressing_count += 1;
            let sender = self.sender.clone();
            rayon::spawn(move || sender.send(
                chunk.map(|chunk| UncompressedBlock::decompress_chunk(chunk, meta, false))
            ).expect("thread error"));
        }


        // return none when all senders are done
        return self.receiver.recv().unwrap_or(None);

 */

        /*if let Some(chunk) = self.remaining_chunks.next() {
            // if self.currently_decompressing_count == 0 { }

            if self.currently_decompressing_count < max_parallel_blocks {
                let send = self.sender.clone();
                currently_running += 1;
                rayon::spawn(move || {
                    let answer = compute(chunk);
                    send.send(answer);
                });
            }

            //if self.currently_decompressing_count > 0 {
                let answer = recv.recv();
                self.currently_decompressing_count -= 1;

                Some(answer)
            //}
        }
        else if self.currently_decompressing_count > 0 {
            let answer = recv.recv();
            self.currently_decompressing_count -= 1;
            Some(answer)
        }
        else {
            None
        }
    }
}*/

impl<R: ChunksReader> ExactSizeIterator for SequentialBlockReader<R> {}
impl<R: ChunksReader> Iterator for SequentialBlockReader<R> {
    type Item = Result<UncompressedBlock>;
    fn next(&mut self) -> Option<Self::Item> { self.decompress_next_block() }
    fn size_hint(&self) -> (usize, Option<usize>) { self.remaining_chunks_reader.size_hint() }
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

    fn on_progress<F>(&mut self, on_progress: F) -> OnProgressChunkWriter<'_, Self, F> where F: FnMut(f64) {
        OnProgressChunkWriter { chunk_writer: self, written_chunks: 0, on_progress }
    }

    fn as_blocks_writer<'w>(&'w mut self, meta: &'w MetaData) -> BlocksWriter<'w, Self> {
        BlocksWriter::new(meta, self)
    }
}


impl<W> ChunksWriter for ChunkWriter<W> where W: Write + Seek {

    /// The total number of chunks that the complete file will contain.
    fn total_chunks_count(&self) -> usize { self.chunk_count }

    /*/// The number of chunks that have already been written.
    pub fn written_chunks_count(&self) -> usize { self.chunk_indices_increasing_y.len() }


    /// Have the right number of chunks been written to the byte writer?
    pub fn all_chunks_are_written(&self) -> bool {
        debug_assert!(self.chunk_indices_increasing_y.len() <= self.chunk_count);
        self.chunk_indices_increasing_y.len() == self.chunk_count
    }*/

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
            return Err(Error::invalid("chunk at this index is already written"));
        }

        *chunk_index_slot = usize_to_u64(self.byte_writer.byte_position());
        chunk.write(&mut self.byte_writer, self.header_count)?;
        Ok(())
    }
}

impl<W> ChunkWriter<W> where W: Write + Seek {
    // -- the following functions are private, because they must be called in a strict order --

    /*/// Returns the next block index that has to be written.
    /// After the correct number of chunks has been written, this updates the offset table and flushes the writer.
    /// Any more calls will result in an error and have no effect.
    /// If writing results in an error, the file and the writer
    /// may remain in an invalid state and should not be used further.
    fn write_chunk_unchecked(&mut self, increasing_y_index: usize, chunk: Chunk) -> UnitResult {
        self.chunk_indices_increasing_y[increasing_y_index] = usize_to_u64(self.byte_writer.byte_position());
        chunk.write(&mut self.byte_writer, self.header_count)?;
        Ok(())
    }*/

    /// Writes the meta data and zeroed offset tables as a placeholder.
    fn new_for_buffered(buffered_byte_writer: W, headers: Headers, pedantic: bool) -> Result<(MetaData, Self)> {
        let mut write = Tracking::new(buffered_byte_writer);
        let requirements = MetaData::write_validating_to_buffered(&mut write, headers.as_slice(), pedantic)?;

        // TODO: use increasing line order where possible
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
        on_progress(self.written_chunks as f64 / total_chunks as f64); // 1.0 for last block

        Ok(())
    }
}



pub struct BlocksWriter<'w, W> {
    meta: &'w MetaData,
    chunks_writer: &'w mut W,
}

pub struct SortedBlocksWriter {
    pending_chunks: BTreeMap<usize, Chunk>,
    unwritten_chunk_indices: Peekable<std::ops::Range<usize>>,
}


impl SortedBlocksWriter {

    pub fn new(total_chunk_count: usize, headers: &[Header]) -> Option<SortedBlocksWriter> {
        let requires_sorting = headers.iter()
            .any(|header| header.line_order != LineOrder::Unspecified);

        if requires_sorting {
            Some(SortedBlocksWriter {
                pending_chunks: BTreeMap::new(),
                unwritten_chunk_indices: (0 .. total_chunk_count).peekable(),
            })
        }
        else {
            None
        }
    }

    pub fn write_or_stash_chunk(&mut self, chunk_index: usize, compressed_chunk: Chunk, mut write_chunk: impl FnMut(Chunk) -> UnitResult) -> UnitResult {
        // TODO not insert if happens to be correct?
        self.pending_chunks.insert(chunk_index, compressed_chunk);

        // TODO return iter instead of calling closure?
        // write all pending blocks that are immediate successors
        while let Some(next_chunk) = self
            .unwritten_chunk_indices.peek().cloned()
            .and_then(|id| self.pending_chunks.remove(&id))
        {
            write_chunk(next_chunk)?;
            self.unwritten_chunk_indices.next().expect("peeked chunk index missing");
        }

        Ok(())
    }
}

impl<'w, W> BlocksWriter<'w, W> where W: 'w + ChunksWriter {

    pub fn new(meta: &'w MetaData, chunks_writer: &'w mut W) -> Self { Self { meta, chunks_writer, } }
    pub fn inner_chunks_writer(&'w self) -> &'w W { self.chunks_writer }

    fn compress_block(&mut self, index_in_header_increasing_y: usize, block: UncompressedBlock) -> UnitResult {
        self.chunks_writer.write_chunk(
            index_in_header_increasing_y,
            block.compress_to_chunk(&self.meta.headers)?
        )
    }

    /// Obtain iterator with `MetaData::collect_ordered_blocks(...)` or similar methods.
    pub fn compress_all_blocks_sequential(mut self, blocks: impl Iterator<Item=(usize, UncompressedBlock)>) -> UnitResult {
        // TODO check block order if line order is not unspecified!
        for (index_in_header_increasing_y, block) in blocks {
            self.compress_block(index_in_header_increasing_y, block)?;
        }

        Ok(())
    }

    /// Obtain iterator with `MetaData::collect_ordered_blocks(...)` or similar methods.
    pub fn compress_all_blocks_parallel(mut self, blocks: impl Iterator<Item=(usize, UncompressedBlock)>) -> UnitResult {
        // do not use parallel procedure for uncompressed images
        let has_compression = self.meta.headers.iter().any(|header| header.compression != Compression::Uncompressed);
        if !has_compression || true /*FIXME*/ {
            return self.compress_all_blocks_sequential(blocks);
        }

        // #[allow(unused)]
        // let mut remaining_chunks = self.chunks_writer.total_chunks_count() as i64; // used for debug_assert
        let meta_data_arc = Arc::new(self.meta.clone());

        let mut sorted_blocks_writer = SortedBlocksWriter::new(
            self.chunks_writer.total_chunks_count(), &self.meta.headers
        );

        let pool = rayon::ThreadPoolBuilder::new().build().expect("thread error");

        let (send, recv) = std::sync::mpsc::channel(); // TODO crossbeam?
        let mut currently_running = 0;

        for (block_file_index, (block_y_index, block)) in blocks.enumerate() {
            while currently_running >= 12 {
                let (chunk_file_index, chunk_y_index, chunk) = recv.recv().expect("thread error")?;
                if let Some(ref mut writer) = sorted_blocks_writer {
                    writer.write_or_stash_chunk(chunk_file_index, chunk, |chunk| {
                        self.chunks_writer.write_chunk(chunk_y_index, chunk)
                    })?;
                }
                else {
                    self.chunks_writer.write_chunk(chunk_y_index, chunk)?;
                }

                currently_running -= 1;
                // remaining_chunks -= 1;
            }

            let send = send.clone();
            let meta_data_arc = meta_data_arc.clone();

            currently_running += 1;

            pool.spawn(move || {
                let compressed = block.compress_to_chunk(&meta_data_arc.headers);
                send.send(compressed.map(|compressed| (block_file_index, block_y_index, compressed))).expect("thread error");
            });
        }

        while currently_running > 0 {
            let (chunk_file_index, chunk_y_index, chunk) = recv.recv().expect("thread error")?;
            if let Some(ref mut writer) = sorted_blocks_writer {
                writer.write_or_stash_chunk(chunk_file_index, chunk, |chunk| {
                    self.chunks_writer.write_chunk(chunk_y_index, chunk)
                })?;
            }
            else {
                self.chunks_writer.write_chunk(chunk_y_index, chunk)?;
            }

            currently_running -= 1;
            // remaining_chunks -= 1;
        }

        if let Some(writer) = sorted_blocks_writer {
            debug_assert_eq!(writer.unwritten_chunk_indices.len(), 0);
        }

        // assert_eq!(remaining_chunks, 0);
        Ok(())
    }
}

/*

// #[must_use]
/// Write chunks to a byte destination.
/// Use `ChunksWriter::write_to_buffered(destination, headers)` to obtain a writer for a specific file.
/// Then write each chunk with `writer.write_chunk(chunk)`.
///
/// No further action is required to complete the file,
/// as soon as the correct number of chunks have been written.
/// When an insufficient number of chunks has been written
/// and the writer is dropped, the file will remain incomplete.
///
/// Use `writer.total_chunk_count()` and `writer.written_chunks_count()` to observe progress.
#[derive(Debug)]
#[must_use]
pub struct ChunksWriter<W> {
    //meta_data: MetaData,
    header_count: usize,
    byte_writer: Tracking<W>,
    offset_table_byte_location: std::ops::Range<usize>,
    collected_offset_table: Vec<u64>,
    chunk_count: usize, // TODO compose?
}

impl<W> ChunksWriter<W> where W: Write + Seek {

    // The meta data which has been written.
    // pub fn meta_data(&self) -> &MetaData { &self.meta_data }

    /// The number of chunks that have already been written.
    pub fn written_chunks_count(&self) -> usize { self.collected_offset_table.len() }

    /// The total number of chunks that the complete file will contain.
    pub fn total_chunks_count(&self) -> usize { self.chunk_count }

    /// Are all chunks written to the byte writer?
    pub fn all_chunks_are_written(&self) -> bool {
        debug_assert!(self.collected_offset_table.len() <= self.chunk_count);
        self.collected_offset_table.len() == self.chunk_count
    }

    /// Write all chunks in the iterator.
    /// Errors if there were not enough or too many chunks in the iterator.
    /// If writing results in an error, the file remains incomplete.
    /// The contents of the chunks are not validated in any way.
    /// Use `write_all_chunks_with` if you want a push-based chunk writer instead.
    pub fn write_all_chunks(mut self, chunks: impl IntoIterator<Item=Chunk>) -> UnitResult {
        self.write_all_chunks_with(|write_chunk|{
            for chunk in chunks { write_chunk(chunk)?; }
            Ok(())
        })
    }

    /// Enter a new scope,
    /// Errors if there were not enough or too many chunks in the iterator.
    /// If writing results in an error, the file remains incomplete.
    /// The contents of the chunks are not validated in any way.
    pub fn write_all_chunks_with<F>(self, mut write_all_chunks: impl FnOnce(ChunkWriter<W>) -> UnitResult) -> UnitResult {
        let writer = ChunkWriter { chunks_writer: self };
        write_all_chunks(writer)?;

        let this = this?;
        if this.all_chunks_are_written() { this.complete_meta_data() }
        else { Err(Error::invalid("not enough chunks provided")) }
    }

    /*/// Write the next compressed chunk to the file.
    /// After the correct number of chunks has been written,
    /// this call updates the offset table and flushes the writer,
    /// and then returns None.
    ///
    /// If writing results in an error, the file remains incomplete.
    /// The contents of the chunks are not validated in any way.
    // these methods return self by value, instead of borrowing,
    // such that in case an error occurs,
    // the writer with invalid state cannot accidentally be used again
    fn write_chunk(mut self, chunk: Chunk) -> Result<Option<Self>> {
        // debug_assert!(self.all_chunks_are_written().not());

        self = self.write_chunk_unchecked(chunk)?;

        if self.all_chunks_are_written() {
            self.complete_meta_data()?;
            Ok(None)
        }
        else {
            Ok(Some(self))
        }
    }*/

    /// Returns the next block index that has to be written.
    /// After the correct number of chunks has been written, this updates the offset table and flushes the writer.
    /// Any more calls will result in an error and have no effect.
    /// If writing results in an error, the file and the writer
    /// may remain in an invalid state and should not be used further.
    fn write_chunk_unchecked(&mut self, chunk: Chunk) -> UnitResult {
        chunk.write(&mut self.byte_writer, self.header_count)?;
        self.collected_offset_table.push(usize_to_u64(self.byte_writer.byte_position()));
        Ok(())
    }

    /// Seek back to the meta data, write offset tables, and flush the byte writer.
    /// Leaves the writer seeked to the middle of the file.
    fn complete_meta_data(mut self) -> UnitResult {

        // write all offset tables
        debug_assert!(self.all_chunks_are_written(), "chunks still missing when attempting to complete");
        debug_assert_ne!(self.byte_writer.byte_position(), self.offset_table_byte_location.end, "already completed");

        self.byte_writer.seek_write_to(self.offset_table_byte_location.start)?;
        u64::write_slice(&mut self.byte_writer, self.collected_offset_table.as_slice())?;

        self.byte_writer.flush()?; // make sure we catch all (possibly delayed) io errors before returning
        Ok(()) // this could return ownership of the meta data
    }

    /// Writes the meta data and zeroed offset tables as a placeholder.
    pub fn write_to_buffered(byte_writer: W, headers: Headers, pedantic: bool) -> Result<(MetaData, Self)> {
        let mut write = Tracking::new(byte_writer);
        let requirements = MetaData::write_validating_to_buffered(&mut write, headers.as_slice(), pedantic)?;

        let offset_table_size: usize = headers.iter().map(|header| header.chunk_count).sum();

        let offset_table_start_byte = write.byte_position();
        let offset_table_end_byte = write.byte_position() + offset_table_size * u64::BYTE_SIZE;

        // skip offset tables, filling with 0, will be updated after the last chunk has been written
        write.seek_write_to(offset_table_end_byte)?;

        let header_count = headers.len();
        let meta_data = MetaData { requirements, headers };

        Ok((meta_data, ChunksWriter {
            header_count,
            byte_writer: write,
            offset_table_byte_location: offset_table_start_byte .. offset_table_end_byte,
            collected_offset_table: Vec::with_capacity(offset_table_size),
            chunk_count: offset_table_size,
        }))
    }
}

pub struct ChunkWriter<W> {
    // private such that cannot construct outside this module
    chunks_writer: ChunksWriter<W>
}

impl<W: Write + Seek> ChunkWriter<W> {
    pub fn chunks_writer(&self) -> &ChunksWriter<W> { &self.chunks_writer }

    pub fn write_chunk(&mut self, chunk: Chunk) -> UnitResult {
        if self.chunks_writer.all_chunks_are_written() {
            return Err(Error::invalid("too many chunks provided"));
        }

        self.chunks_writer = self.chunks_writer.write_chunk_unchecked(chunk)?;
        Ok(())
    }
}*/

/*impl<W> Drop for ChunksWriter<W> where W: Write + Seek {
    fn drop(&mut self) {
        debug_assert!(
            self.all_chunks_are_written(),
            "not all chunks have been written when dropping chunk writer"
        )
    }
}*/


/*#[must_use]
#[derive(Debug)]
pub struct SequentialBlockWriter<W, I: Iterator<Item=BlockIndex>> {
    chunks_writer: ChunksWriter<W>,
    meta_data: MetaData,

    // #[allow(unused)]
    remaining_block_indices: Peekable<I>,
}

impl<W, I> SequentialBlockWriter<W, I> where W: Write + Seek, I: Iterator<Item=BlockIndex> {

    /*/// All blocks in the image, with the same order as they should appear in the file.
    pub fn ordered_block_indices(&self) -> impl Iterator<Item=BlockIndex> {
        ordered_blocks_indices(self.chunks_writer.meta_data().headers.as_slice())
    }*/

    pub fn next_block_index(&mut self) -> Option<BlockIndex> {
        self.remaining_block_indices.peek().cloned()
    }

    /// Returns None if no more blocks should be written.
    /// If writing results in an error, the file remains incomplete.
    /// The order must be exactly as the order of `ordered_blocks_indices()`.
    /// The pixel contents of the block are not validated in any way.
    pub fn compress_and_write_next_block(self, block: UncompressedBlock) -> Result<Option<Self>> {
        let Self { mut chunks_writer, meta_data, mut remaining_block_indices } = self;

        let expected_index = *remaining_block_indices.peek().expect("block indices chunk count mismatch");
        if expected_index != block.index { return Err(Error::invalid("wrong block to be written")); }

        let chunk = block.compress_to_chunk(meta_data.headers.as_slice())?;

        Ok(chunks_writer.write_chunk(chunk)?.map(|chunks_writer| {
            remaining_block_indices.next();
            Self { chunks_writer, meta_data, remaining_block_indices }
        }))
    }
}*/



/*pub struct SequentialBlockWriter<W> {
    chunks_writer: ChunksWriter<W>,
}

impl<W> SequentialBlockWriter<W> where W: Write + Seek {

    pub fn write_to_buffered(byte_writer: W, mut headers: Headers, pedantic: bool) -> Self {
        let has_compression = headers.iter() // TODO cache this in MetaData.has_compression?
            .any(|header| header.compression != Compression::Uncompressed);

        let parallel = false;
        // TODO if non-parallel compression, we always use increasing order anyways
        if !parallel || !has_compression {
            for header in &mut headers {
                if header.line_order == LineOrder::Unspecified {
                    header.line_order = LineOrder::Increasing;
                }
            }
        }

        ChunksWriter::write_to_buffered(byte_writer, headers, pedantic)
    }

    /// Compresses and then writes the block.
    pub fn write_all_block(&mut self, block: UncompressedBlock) -> UnitResult {
        let chunk = block.compress_to_chunk(self.chunks_writer.meta_data().headers.as_slice())?;
        self.chunks_writer.write_chunk(chunk)
    }

    /*/// Compresses each blocks and writes it to the file. Errors if an incorrect number of blocks is supplied.
    pub fn write_all_blocks(&mut self, blocks: impl Iterator<Item=UncompressedBlock>) -> UnitResult {
        self.chunks_writer.write_all_chunks(blocks.map(||))
        // let chunk = block.compress_to_chunk(self.chunks_writer.meta_data().headers.as_slice())?;
        // self.chunks_writer.write_chunk(chunk)
    }*/

    /// The meta data which has been written.
    pub fn meta_data(&self) -> &MetaData { &self.chunks_writer.meta_data }

    /// The number of chunks that have already been written.
    pub fn written_chunks_count(&self) -> usize { self.chunks_writer.written_chunks_count() }

    /// The total number of chunks that the complete file will contain.
    pub fn total_chunks_count(&self) -> usize { self.chunks_writer.total_chunks_count() }

    /// Are all chunks written to the byte writer?
    pub fn is_complete(&self) -> bool { self.chunks_writer.all_chunks_are_written() }


    // pub fn next_block_index(&mut self) -> Option<> {
    //
    // }
}*/





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
fn write_all_blocks_to_buffered(
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
        chunk.write(&mut write, headers.len())?;

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
fn read_all_blocks_from_buffered<T>(
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
pub /*TODO Remove*/ fn read_filtered_blocks_from_buffered<T>(
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
    let has_compression = meta_data.headers.iter() // do not use parallel procedure for uncompressed images
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
fn read_all_compressed_chunks_from_buffered<'m>(
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
fn read_filtered_chunks_from_buffered<'m, T>(
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


/// Iterate over all uncompressed blocks of an image.
/// The image contents are collected by the `get_line` function parameter.
/// Returns blocks in `LineOrder::Increasing`, unless the line order is requested to be decreasing.
#[inline]
#[must_use]
fn uncompressed_image_blocks_ordered<'l>(
    headers: &'l [Header],
    get_block: &'l (impl 'l + Sync + (Fn(&[Header], BlockIndex) -> Vec<u8>)) // TODO reduce sync requirements, at least if parrallel is false
) -> impl 'l + Iterator<Item = Result<(usize, UncompressedBlock)>> + Send // TODO reduce sync requirements, at least if parrallel is false
{
    headers.iter().enumerate().flat_map(move |(layer_index, header)|{
        header.enumerate_ordered_blocks().map(move |(_index_in_header_increasing_y, tile)|{
            if true { unimplemented!() }
            let data_indices = header.get_absolute_block_pixel_coordinates(tile.location).expect("tile coordinate bug");

            let block_indices = BlockIndex {
                layer: layer_index, level: tile.location.level_index,
                pixel_position: data_indices.position.to_usize("data indices start").expect("data index bug"),
                pixel_size: data_indices.size,
            };

            let block_bytes = get_block(headers, block_indices);

            // byte length is validated in block::compress_to_chunk
            Ok((0, UncompressedBlock {
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
fn for_compressed_blocks_in_image(
    headers: &[Header], get_tile: impl Sync + Fn(&[Header], BlockIndex) -> Vec<u8>,
    parallel: bool, mut write_chunk: impl FnMut(usize, Chunk) -> UnitResult
) -> UnitResult
{
    let blocks = uncompressed_image_blocks_ordered(headers, &get_tile);

    let parallel = parallel && headers.iter() // do not use parallel procedure for uncompressed images
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
            unimplemented!();
            /*// the block indices, in the order which must be apparent in the file
            let mut expected_id_order = headers.iter().enumerate()
                .flat_map(|(layer, header)| header.ordered_blocks().map(move |(chunk, _)| (layer, chunk)));

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
            assert_eq!(pending_blocks.len(), 0, "pending blocks left after processing bug");*/
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
