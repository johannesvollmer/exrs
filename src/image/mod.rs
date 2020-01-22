
pub mod full;

use crate::meta::attributes::*;
use crate::compression::{Compression, ByteVec};
use crate::math::*;
use std::io::{Read, Seek, Write};
use crate::error::{Result, Error, PassiveResult};
use crate::meta::{MetaData, Header, TileIndices, Blocks};
use crate::chunks::{Chunk, Block, TileBlock, ScanLineBlock};
use crate::io::{PeekRead, Tracking};
use rayon::iter::{ParallelIterator, ParallelBridge};
use std::convert::TryFrom;
use crate::io::Data;
use smallvec::SmallVec;
use std::ops::Range;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {
    pub parallel_compression: bool,
    pub override_line_order: Option<LineOrder>,
    pub override_blocks: Option<Blocks>, // TODO is this how we imagine write options?
    pub override_compression: Option<Compression>, // TODO is this how we imagine write options?
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    pub parallel_decompression: bool,
}


#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub struct BlockIndex {
    pub part: usize,
    pub position: Vec2<usize>,
    pub size: Vec2<usize>,
    pub level: Vec2<usize>,
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UncompressedBlock {
    pub index: BlockIndex,
    pub data: ByteVec,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Line<'s> {
    pub location: LineIndex,
    pub value: &'s [u8],
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash)]
pub struct LineIndex {
    pub part: usize,
    pub channel: usize,
    pub level: Vec2<usize>,
    pub position: Vec2<usize>,
    pub width: usize,
}

impl<'s> Line<'s> {
    pub fn read_samples<T: crate::io::Data>(&self, slice: &mut [T]) -> PassiveResult {
        debug_assert_eq!(slice.len(), self.location.width);
        T::read_slice(&mut self.value.clone(), slice)
    }
}

impl LineIndex {
    pub fn write_samples<T: crate::io::Data>(slice: &[T], write: &mut impl Write) -> PassiveResult {
        T::write_slice(write, slice)?;
        Ok(())
    }
}


/// reads all chunks sequentially without seeking
pub fn read_all_lines<T>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    options: ReadOptions,
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, Line<'_>) -> PassiveResult
) -> Result<T>
{
    let (meta_data, mut read_chunk) = self::read_all_compressed_chunks(read)?;
    let meta_data_ref = &meta_data;

    let read_chunks = std::iter::from_fn(move || read_chunk(meta_data_ref));
    let mut value = new(meta_data.headers.as_slice())?;

    for_lines_in_chunks(
        read_chunks, &meta_data, options.parallel_decompression,
        |line| insert(&mut value, line)
    )?;

    Ok(value)
}


/// reads all chunks sequentially without seeking
pub fn read_filtered_lines<T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    options: ReadOptions,
    filter: impl Fn(&Header, &TileIndices) -> bool,
    new: impl Fn(&[Header]) -> Result<T>,
    mut insert: impl FnMut(&mut T, Line<'_>) -> PassiveResult
) -> Result<T>
{
    let (meta_data, mut read_chunk) = self::read_filtered_chunks(read, filter)?;
    let read_chunks = std::iter::from_fn(|| read_chunk(&meta_data));
    let mut value = new(meta_data.headers.as_slice())?;

    for_lines_in_chunks(
        read_chunks, &meta_data, options.parallel_decompression,
        |line| insert(&mut value, line)
    )?;

    Ok(value)
}

pub fn for_lines_in_chunks(chunks: impl Send + Iterator<Item = Result<Chunk>>, meta_data: &MetaData, parallel: bool, mut for_each: impl FnMut(Line<'_>) -> PassiveResult) -> PassiveResult {
    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::Uncompressed).is_some();

    if parallel && has_compression {
        let (sender, receiver) = std::sync::mpsc::channel();

        chunks.par_bridge()
            .map(|chunk| UncompressedBlock::decompress_chunk(chunk?, &meta_data))
            .try_for_each_with(sender, |sender, result| {
                result.map(|block: UncompressedBlock| sender.send(block).unwrap())
            })?;

        for decompressed in receiver {
            let header = meta_data.headers.get(decompressed.index.part).unwrap();
            for (bytes, line) in decompressed.index.lines(header) {
                for_each(Line { location: line, value: &decompressed.data[bytes] })?;
            }
        }

        Ok(())
    }
    else {
        for chunk in chunks {
            let decompressed = UncompressedBlock::decompress_chunk(chunk?, &meta_data)?;
            let header = meta_data.headers.get(decompressed.index.part).unwrap();
            for (bytes, line) in decompressed.index.lines(header) {
                for_each(Line { location: line, value: &decompressed.data[bytes] })?;
            }
        }

        Ok(())
    }
}

/// reads all chunks sequentially without seeking
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


/// reads all chunks sequentially without seeking
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




/// assumes the reader is buffered
#[must_use]
pub fn write_all_lines(
    write: impl Write + Seek, options: WriteOptions, mut meta_data: MetaData,
    get_line: impl Fn(LineIndex) -> Result<ByteVec>
) -> PassiveResult
{
    // if non-parallel compression, we always can use increasing order without cost
    if !options.parallel_compression {
        for header in &mut meta_data.headers {
            if header.line_order == LineOrder::Unspecified {
                header.line_order = LineOrder::Increasing;
            }
        }
    }

    let mut write = Tracking::new(write);
    meta_data.write(&mut write)?;

    let offset_table_start_byte = write.byte_position();

    // skip offset tables for now
    let offset_table_size: u32 = meta_data.headers.iter()
        .map(|header| header.chunk_count).sum();

    write.seek_write_to(write.byte_position() + offset_table_size as usize * std::mem::size_of::<u64>())?;

    let mut offset_tables: Vec<Vec<u64>> = meta_data.headers.iter()
        .map(|header| vec![0; header.chunk_count as usize]).collect();

    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::Uncompressed).is_some();

    if options.parallel_compression && has_compression {
        debug_assert_eq!(options.override_line_order, Some(LineOrder::Unspecified));
        unimplemented!()
    }
    else {
        for (part_index, header) in meta_data.headers.iter().enumerate() {

            let mut write_block = |chunk_index: usize, tile: TileIndices| -> Result<()> {
                let data_indices = header.get_absolute_block_indices(tile.location)?;
                let block_indices = BlockIndex {
                    part: part_index,
                    level: Vec2::try_from(tile.location.level_index).unwrap(),
                    position: Vec2::try_from(data_indices.start).unwrap(),
                    size: Vec2::try_from(data_indices.size).unwrap()
                };

                let mut data = Vec::new(); // TODO allocate only block, not lines
                for (byte_range, line_index) in block_indices.lines(header) {
                    debug_assert_eq!(byte_range.start, data.len());
                    data.extend_from_slice(get_line(line_index)?.as_slice());
                    debug_assert_eq!(byte_range.end, data.len());
                }

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

            if options.override_line_order.unwrap_or(header.line_order) == LineOrder::Decreasing {
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

    pub fn lines(&self, header: &Header) -> impl Iterator<Item=(Range<usize>, LineIndex)> {
        struct LineIter {
            part: usize, level: Vec2<usize>, width: usize, end_y: usize, x: usize,
            channel_sizes: SmallVec<[usize; 8]>,
            byte: usize, channel: usize, y: usize,
        };

        impl Iterator for LineIter {
            type Item = (Range<usize>, LineIndex);

            fn next(&mut self) -> Option<Self::Item> {
                if self.y < self.end_y {
                    let byte_index = self.byte;
                    let channel = self.channel;
                    let y = self.y;
                    let byte_len = self.channel_sizes[self.channel];

                    { // increment indices
                        self.byte += byte_len;
                        self.channel += 1;

                        if self.channel == self.channel_sizes.len() {
                            self.channel = 0;
                            self.y += 1;
                        }
                    }

                    return Some((
                        (byte_index .. byte_index + byte_len),
                        LineIndex {
                            channel,
                            part: self.part,
                            level: self.level,
                            position: Vec2(self.x, y),
                            width: self.width,
                        }
                    ))
                }

                None
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

        /*let mut index = 0;
        (self.index.position.1 .. self.index.position.1 + self.index.size.1).flat_map(move |y| {
            channel_line_sizes.iter().enumerate().map(move |(channel_index, byte_len)| {
                let byte_index = index;
                index += byte_len;

                (byte_index, LineIndex {
                    part: self.index.part,
                    channel: channel_index,
                    level: self.index.level,
                    position: Vec2(self.index.position.0, y),
                    width: self.index.size.0,
                })
            })
        })*/
    }
}

impl UncompressedBlock {

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
                    part: usize::try_from(chunk.part_number).unwrap(),
                    position: Vec2::try_from(absolute_indices.start).unwrap(),
                    size: Vec2::try_from(absolute_indices.size).unwrap(),
                    level: Vec2::try_from(tile_data_indices.level_index).unwrap(),
                }
            }),

            _ => return Err(Error::unsupported("deep data"))
        }
    }

    /*pub fn write_to<'b>(&self, lines: impl Iterator<Item= impl Iterator<Item=Result<&'b mut[u8]>> >) -> PassiveResult {
        let mut byte_source = self.data.as_slice();

        for line in lines {
            for bytes in line {
                byte_source.read_exact(bytes?)?;
            }
        }

        Ok(())
    }*/



    /*pub fn read_from<'b>(lines: impl Iterator<Item= impl Iterator<Item=&'b[u8]> >) -> PassiveResult {
        let mut bytes_target = Vec::with_capacity(512);

        for line in lines {
            for bytes in line {
                bytes_target.write_all(bytes)?;
            }
        }

        Ok(())
    }*/


}


impl Default for WriteOptions {
    fn default() -> Self { Self::fast_writing() }
}

impl Default for ReadOptions {
    fn default() -> Self { Self::fast_loading() }
}


impl WriteOptions {
    pub fn fast_writing() -> Self {
        WriteOptions {
            parallel_compression: true,
            override_line_order: Some(LineOrder::Unspecified),
            override_compression: Some(Compression::Uncompressed),
            override_blocks: None,
        }
    }

    pub fn small_image() -> Self {
        WriteOptions {
            parallel_compression: true,
            override_line_order: Some(LineOrder::Unspecified),
            override_compression: Some(Compression::ZIP16),
            override_blocks: None,
        }
    }

    pub fn small_writing() -> Self {
        WriteOptions {
            parallel_compression: false,
            override_line_order: Some(LineOrder::Unspecified),
            override_compression: Some(Compression::Uncompressed),
            override_blocks: None,
        }
    }

    pub fn debug() -> Self {
        WriteOptions {
            parallel_compression: false,
            override_line_order: None,
            override_blocks: None,
            override_compression: None
        }
    }
}

impl ReadOptions {

    pub fn fast_loading() -> Self {
        ReadOptions {
            parallel_decompression: true
        }
    }

    pub fn small_loading() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }

    pub fn debug() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }

}
