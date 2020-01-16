
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

/// temporarily used to construct images in parallel
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UncompressedBlock {
    pub index: BlockIndex,
    pub data: ByteVec,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Line<B> {
    pub part: usize,
    pub channel: usize,
    pub level: Vec2<usize>,
    pub position: Vec2<usize>,
    pub width: usize,
    pub value: B,
}

/// reads all chunks sequentially without seeking
pub fn read_all_chunks<T>(
    read: impl Read + Send, // FIXME does not actually need to be send, only for parallel writing
    options: ReadOptions,
    new: impl Fn(&[Header]) -> Result<T>,
    insert: impl Fn(T, Line<&[u8]>) -> Result<T>
) -> Result<T>
{
    let mut read = PeekRead::new(read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let chunk_count = MetaData::skip_offset_tables(&mut read, &meta_data.headers)? as usize;

    let mut value = new(meta_data.headers.as_slice())?;

    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::Uncompressed).is_some();

    if options.parallel_decompression && has_compression {
        //test read_single_image_non_parallel_zips        ... bench: 225,905,400 ns/iter (+/- 20,668,415)
        //test read_single_image_rle                      ... bench:  39,283,900 ns/iter (+/- 1,979,897)
        //test read_single_image_uncompressed             ... bench:  29,844,112 ns/iter (+/- 3,044,599)
        //test read_single_image_uncompressed_from_buffer ... bench:  22,508,987 ns/iter (+/- 1,813,870)
        //test read_single_image_zips                     ... bench:  56,839,975 ns/iter (+/- 6,729,915)
        let (sender, receiver) = std::sync::mpsc::channel();
        let meta = &meta_data;

        (0..chunk_count)
            .map(|_| Chunk::read(&mut read, meta))
            .par_bridge().map(|chunk| UncompressedBlock::decompress_chunk(chunk?, meta))
            .try_for_each_with(sender, |sender, result| {
                result.map(|block: UncompressedBlock| sender.send(block).unwrap())
            })?;

        for decompressed in receiver {
            let header = meta_data.headers.get(decompressed.index.part).unwrap();
            for line in decompressed.lines(header) {
                value = insert(value, line)?;
            }
        }

        // TODO profile memory usage (current should be better than below)
        //test read_single_image_non_parallel_zips        ... bench: 227,737,450 ns/iter (+/- 18,785,790)
        //test read_single_image_rle                      ... bench:  46,210,075 ns/iter (+/- 8,100,857)
        //test read_single_image_uncompressed             ... bench:  30,559,725 ns/iter (+/- 4,821,182)
        //test read_single_image_uncompressed_from_buffer ... bench:  22,342,850 ns/iter (+/- 3,603,042)
        //test read_single_image_zips                     ... bench:  65,634,725 ns/iter (+/- 10,510,804)
        /*let compressed: Result<Vec<Chunk>> = (0..chunk_count)
            .map(|_| Chunk::read(&mut read, &meta_data))
            .collect();

        let decompress = compressed?.into_par_iter().map(|chunk|
            UncompressedBlock::from_compressed(chunk, &meta_data)
        );

        let decompressed: Result<Vec<UncompressedBlock>> = decompress.collect();

        for decompressed in decompressed? {
            value = insert(value, decompressed)?;
        }*/
    }
    else {
        for _ in 0..chunk_count {
            // TODO avoid all allocations for uncompressed data
            let chunk = Chunk::read(&mut read, &meta_data)?;
            let decompressed = UncompressedBlock::decompress_chunk(chunk, &meta_data)?;

            let header = meta_data.headers.get(decompressed.index.part).unwrap();
            for line in decompressed.lines(header) {
                value = insert(value, line)?;
            }
        }
    }

    Ok(value)
}


/// reads all chunks sequentially without seeking
pub fn read_filtered_chunks<T>(
    read: impl Read + Seek + Send, // FIXME does not always need be Send
    options: ReadOptions,
    filter: impl Fn(&Header, &TileIndices) -> bool,
    new: impl Fn(&[Header]) -> Result<T>,
    insert: impl Fn(T, UncompressedBlock) -> Result<T>
) -> Result<T>
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

    offsets.sort();

    let mut value = new(meta_data.headers.as_slice())?;

    let has_compression = meta_data.headers.iter() // do not use parallel stuff for uncompressed images
        .find(|header| header.compression != Compression::Uncompressed).is_some();

    if options.parallel_decompression && has_compression {
        let (sender, receiver) = std::sync::mpsc::channel();

        offsets.into_iter()
            .map(|offset| {
                read.skip_to(offset as usize)?; // this is only ever going to skip forward, use skip_bytes for small amounts instead?
                Chunk::read(&mut read, &meta_data)
            })
            .par_bridge().map(|chunk| UncompressedBlock::decompress_chunk(chunk?, &meta_data))
            .try_for_each_with(sender, |sender, result| {
                result.map(|block: UncompressedBlock| sender.send(block).unwrap())
            })?;

        for decompressed in receiver {
            value = insert(value, decompressed)?;
        }
    }
    else {
        for offset in offsets {
            // TODO avoid all allocations for uncompressed data
            read.skip_to(offset as usize)?; // this is only ever going to skip forward, use skip_bytes for small amounts instead?
            let chunk = Chunk::read(&mut read, &meta_data)?;
            let decompressed = UncompressedBlock::decompress_chunk(chunk, &meta_data)?;

            value = insert(value, decompressed)?;
        }
    }

    Ok(value)
}


/// assumes the reader is buffered
#[must_use]
pub fn write_chunks(
    write: impl Write + Seek, options: WriteOptions, mut meta_data: MetaData,
    get_block: impl Fn(BlockIndex) -> Result<ByteVec>
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
       /* for (part_index, header) in meta_data.headers.iter().enumerate() {
//            let header = &meta_data.headers[part_index];
            let mut table = Vec::new();

            for tile in header.blocks() {
//                debug_assert_eq!(header.display_window, image.display_window);
//                debug_assert_eq!(header.data_window, image.parts[part_index].data_window);
                let data_indices = header.get_absolute_block_indices(tile.location)?;

                let data_size = Vec2::try_from(data_indices.size).unwrap();
                let data_position = Vec2::try_from(data_indices.start).unwrap();
                let data_level = Vec2::try_from(tile.location.level_index).unwrap();

                let data = get_block(part_index, data_position, data_size, data_level)?;
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
                            compressed_pixels: data,
                            coordinates: tile.location,
                        }),
                    }
                };

                let block_start_position = write.byte_position();
                table.push((tile, block_start_position as u64));


                chunk.write(&mut write, meta_data.headers.as_slice())?;
            }

            // sort offset table by increasing y
            table.sort_by(|(a, _), (b, _)| a.cmp(b));
            offset_tables.extend(table.into_iter().map(|(_, index)| index));
        }*/
    }
    else {
        for (part_index, header) in meta_data.headers.iter().enumerate() {

            let mut write_block = |chunk_index: usize, tile: TileIndices| -> Result<()> {
                let data_indices = header.get_absolute_block_indices(tile.location)?;

                let data = get_block(BlockIndex {
                    part: part_index,
                    level: Vec2::try_from(tile.location.level_index).unwrap(),
                    position: Vec2::try_from(data_indices.start).unwrap(),
                    size: Vec2::try_from(data_indices.size).unwrap()
                })?;

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

    pub fn lines<'s>(&'s self, header: &'s Header) -> impl Iterator<Item=Line<&'s [u8]>> {
        let mut index = 0;

        (self.index.position.1 .. self.index.position.1 + self.index.size.1).flat_map(move |y| {
            header.channels.list.iter().enumerate().map(move |(channel_index, channel)| {
                let byte_len = self.index.size.0 * channel.pixel_type.bytes_per_sample() as usize;

                let line = &self.data[index .. index + byte_len];
                index += byte_len;

                Line {
                    part: self.index.part,
                    channel: channel_index,
                    level: self.index.level,
                    position: Vec2(self.index.position.0, y),
                    width: self.index.size.0,
                    value: line,
                }
            })
        })
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



    pub fn read_from<'b>(lines: impl Iterator<Item= impl Iterator<Item=&'b[u8]> >) -> PassiveResult {
        let mut bytes_target = Vec::with_capacity(512);

        for line in lines {
            for bytes in line {
                bytes_target.write_all(bytes)?;
            }
        }

        Ok(())
    }


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
