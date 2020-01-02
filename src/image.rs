//! The `image` module is for interpreting the loaded file data.
//!


use smallvec::SmallVec;
use half::f16;
use rayon::prelude::{IntoParallelIterator};
use rayon::iter::{ParallelIterator, IntoParallelRefIterator};

use crate::chunks::*;
use crate::io::*;
use crate::meta::*;
use crate::error::validity::*;
use crate::meta::attributes::*;
//use crate::file::*;

use crate::compression::{ByteVec, Compression};
use crate::error::{ReadResult, WriteResult, WriteError};
use std::io::{BufReader, BufWriter, Seek, SeekFrom, Cursor};


pub use crate::io::Data;

// TODO notes:
// Channels with an x or y sampling rate other than 1 are allowed only in flat, scan-line based images. If an image is deep or tiled, then the x and y sampling rates for all of its channels must be 1.
// Scan-line based images cannot be multi-resolution images.



#[derive(Clone, PartialEq, Debug)]
pub struct Image {
    pub parts: Parts,

    display_window: I32Box2,
    pixel_aspect: f32,
}

/// an exr image can store multiple parts (multiple bitmaps inside one image)
pub type Parts = SmallVec<[Part; 3]>;

#[derive(Clone, PartialEq, Debug)]
pub struct Part {
    pub data_window: I32Box2,
    pub screen_window_center: (f32, f32),
    pub screen_window_width: f32,

    pub name: Option<Text>,
    pub attributes: Attributes,

    /// only the data for this single part,
    /// index can be computed from pixel location and block_kind.
    /// one part can only have one block_kind, not a different kind per block
    /// number of x and y levels can be computed using the header
    ///
    /// That Vec contains one entry per mip map level, or only one if it does not have any,
    /// or a row-major flattened vector of all rip maps like
    /// 1x1, 2x1, 4x1, 8x1, and then
    /// 1x2, 2x2, 4x2, 8x2, and then
    /// 1x4, 2x4, 4x4, 8x4, and then
    /// 1x8, 2x8, 4x8, 8x8.
    ///
    pub channels: Channels,
}


pub type Channels = SmallVec<[Channel; 5]>;

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub name: Text,
    pub content: ChannelData,
    pub is_linear: bool,
    pub sampling: (usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ChannelData {
    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction.
    ///
    /// Currently this crate is using the `half` crate, which is an implementation of the IEEE 754-2008 standard, meeting that requirement.
    F16(SampleMaps<f16>),

    F32(SampleMaps<f32>),

    U32(SampleMaps<u32>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SampleMaps<Sample> {
    Flat (Levels<FlatSamples<Sample>>),
    Deep (Levels<DeepSamples<Sample>>), // TODO can deep images even have levels?
}

// FIXME should be descending and starting with full-res instead!
#[derive(Clone, PartialEq, Debug)]
pub enum Levels<Samples> {
    Singular(SampleBlock<Samples>),
    Mip(LevelMaps<Samples>),
    Rip(RipMaps<Samples>),
}

pub type LevelMaps<Samples> = Vec<SampleBlock<Samples>>;

#[derive(Clone, PartialEq, Debug)]
pub struct RipMaps<Samples> {
    pub map_data: LevelMaps<Samples>,
    pub level_count: (usize, usize),
}

#[derive(Clone, PartialEq, Debug)]
pub struct SampleBlock<Samples> {
    pub resolution: (usize, usize),
    pub samples: Samples
}

pub type FlatSamples<Sample> = Vec<Sample>;

pub type DeepSamples<Sample> = Vec<DeepLine<Sample>>;
// TODO do not store line by line in a separate vector!

#[derive(Clone, Debug, PartialEq)]
pub struct DeepLine<Sample> {
    // TODO do not store line by line in a separate vector!
    pub samples: Vec<Sample>,
    pub index_table: Vec<u32>,
}


/// temporarily used to construct images in parallel
#[derive(Clone, PartialEq, Debug)]
pub struct UncompressedBlock {
    part_index: usize,
    tile: TileIndices,
    data: ByteVec,
}


impl Image {

    #[must_use]
    pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions) -> ReadResult<Self> {
        Self::read_from_unbuffered(std::fs::File::open(path)?, options)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read, options: ReadOptions) -> ReadResult<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered), options)
    }

    /// assumes the reader is buffered (if desired)
    #[must_use]
    pub fn read_from_buffered(read: impl Read, options: ReadOptions) -> ReadResult<Self> {
        let mut read = PeekRead::new(read);
        let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
        let offset_tables = MetaData::read_offset_tables(&mut read, meta_data.requirements, &meta_data.headers)?;
        Self::read_data_by_meta(meta_data, offset_tables, &mut read, options)
    }


    /// assumes the reader is buffered (if desired)
    #[must_use]
    pub fn read_data_by_meta(meta_data: MetaData, offset_tables: OffsetTables, read: &mut impl Read, options: ReadOptions) -> ReadResult<Self> {
        let MetaData { headers, requirements } = meta_data;

        let chunk_reader = ChunkReader::new(read, requirements.is_multipart(), &headers, &offset_tables);

        let mut image = Image::new(headers.as_slice());

        let has_compression = headers.iter() // do not use parallel stuff for uncompressed images
            .find(|header| header.compression != Compression::Uncompressed).is_some();

        if options.parallel_decompression && has_compression {
            let chunks: Vec<ReadResult<Chunk>> = chunk_reader.collect();
            let blocks = chunks.into_par_iter().map(|chunk| chunk.and_then(|chunk|
//                catch_unwind(|| { TODO?
                    UncompressedBlock::from_compressed(chunk, headers.as_slice())
//                })
            ));

            let blocks: Vec<ReadResult<UncompressedBlock>> = blocks.collect(); // TODO without double collect!

            for block in blocks {
                let block = block?; // TODO use write everywhere instead of block allocations?
                image.insert_block(&mut block.data.as_slice(), block.part_index, block.tile)?;
            }
        }
        else {
            let decompressed = chunk_reader // TODO use write everywhere instead of block allocations?
                .map(|chunk| chunk.and_then(|chunk|
                    UncompressedBlock::from_compressed(chunk, headers.as_slice())
                ));

            // TODO avoid all allocations for uncompressed data
            for block in decompressed {
                let block = block?;
                image.insert_block(&mut block.data.as_slice(), block.part_index, block.tile)?;
            }
        }

        Ok(image)
    }


    #[must_use]
    pub fn write_to_file(&self, path: &std::path::Path, options: WriteOptions) -> WriteResult {
        Self::write_to_unbuffered(self, std::fs::File::create(path)?, options)
    }

    /// needs more memory but allows for non-seeking write operations
    #[must_use]
    pub fn write_without_seek(&self, mut unbuffered: impl Write, options: WriteOptions) -> WriteResult {
        let mut bytes = Vec::new();

        // write the image to the seekable vec
        Self::write_to_buffered(self, Cursor::new(&mut bytes), options)?;

        // write the vec into the actual output
        unbuffered.write_all(&bytes)?;
        Ok(())
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn write_to_unbuffered(&self, unbuffered: impl Write + Seek, options: WriteOptions) -> WriteResult {
        Self::write_to_buffered(self, BufWriter::new(unbuffered), options)
    }

    /// assumes the reader is buffered
    #[must_use]
    pub fn write_to_buffered(&self, mut write: impl Write + Seek, options: WriteOptions) -> WriteResult {
        let meta_data = self.infer_meta_data(options)?;
        meta_data.write(&mut write)?;

        let offset_table_start_byte = write.seek(SeekFrom::Current(0))?;

        // skip offset tables for now
        let offset_table_size: u32 = meta_data.headers.iter()
            .map(|header| header.compute_offset_table_size(meta_data.requirements).unwrap()).sum();

        let mut offset_tables: Vec<u64> = vec![0; offset_table_size as usize];
        u64::write_slice(&mut write, offset_tables.as_slice())?;
        offset_tables.clear();

        if !options.parallel_compression {
            for (part_index, part) in self.parts.iter().enumerate() {
                let mut table = Vec::new();

                part.tiles(&meta_data.headers[part_index], &mut |tile| {
                    let data: Vec<u8> = self.compress_block(options.compression_method, part_index, tile)?;

                    let chunk = Chunk {
                        part_number: part_index as i32,
                        block: Block::ScanLine(ScanLineBlock {
                            y_coordinate: part.data_window.y_min + tile.position.1 as i32,
                            compressed_pixels: data
                        }) // FIXME add the other ones?? match???
                    };

                    let block_start_position = write.seek(SeekFrom::Current(0))?;
                    table.push((tile, block_start_position as u64));

                    chunk.write(&mut write, meta_data.headers.len() > 1, meta_data.headers.as_slice())?;

                    Ok(())
                })?;

                // sort by increasing y
                table.sort_by(|(a, _), (b, _)| a.cmp(b));
                offset_tables.extend(table.into_iter().map(|(_, index)| index));
            }
        }
        else {
            unimplemented!()
        }

        // write offset tables after all blocks have been written
        debug_assert_eq!(offset_tables.len(), offset_table_size as usize);
        write.seek(SeekFrom::Start(offset_table_start_byte))?;
        u64::write_slice(&mut write, offset_tables.as_slice())?;

        Ok(())
    }
}


#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {
    pub parallel_compression: bool,
    pub compression_method: Compression,
    pub line_order: LineOrder,
    pub tiles: TileOptions
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TileOptions {
    Tiles { size: (u32, u32), rounding: RoundingMode },
    ScanLineBlocks
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    pub parallel_decompression: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            parallel_compression: true,
            compression_method: Compression::RLE,
            line_order: LineOrder::IncreasingY,
            tiles: TileOptions::ScanLineBlocks
        }
    }
}

impl WriteOptions {
    pub fn debug() -> Self {
        WriteOptions {
            parallel_compression: false,
            compression_method: Compression::Uncompressed,
            line_order: LineOrder::IncreasingY,
            tiles: TileOptions::ScanLineBlocks
        }
    }
}

impl ReadOptions {
    pub fn debug() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions {
            parallel_decompression: true
        }
    }
}


impl TileOptions {
    pub fn has_tiles(&self) -> bool {
        match self {
            TileOptions::Tiles { .. } => true,
            _ => false
        }
    }
}

impl UncompressedBlock {
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    pub fn from_compressed(chunk: Chunk, headers: &[Header]) -> ReadResult<Self> {
        let part_count = headers.len();
        let header: &Header = headers.get(chunk.part_number as usize)
            .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

        let raw_coordinates = header.get_raw_block_coordinates(&chunk.block)?;
        let tile_data_indices = header.get_block_data_indices(&chunk.block)?;
        raw_coordinates.validate(Some(header.data_window.dimensions()))?;

        match chunk.block {
            Block::Tile(TileBlock { compressed_pixels, .. }) |
            Block::ScanLine(ScanLineBlock { compressed_pixels, .. }) => {

                let data = header.compression.decompress_image_section(header, compressed_pixels, raw_coordinates)?;
                Ok(UncompressedBlock { part_index: chunk.part_number as usize, tile: tile_data_indices, data,  })
            },

            _ => unimplemented!()
        }
    }

//    pub fn to_compressed(&self, header: &Header, options: TileOptions) -> Result<Chunk, WriteError> {
//
//    }
}

impl Image {
    pub fn new(headers: &[Header]) -> Self {
        let mut display = headers.iter()
            .map(|header| header.display_window);

        // FIXME check all display windows are the same
        let display_window = display.next().unwrap();

        let mut pixel_aspect = headers.iter()
            .map(|header| header.pixel_aspect);

        // FIXME check all display windows are the same
        let pixel_aspect = pixel_aspect.next().unwrap();

        Image {
            parts: headers.iter().map(Part::new).collect(),
            display_window, pixel_aspect
        }
    }

    pub fn insert_block(&mut self, data: &mut impl Read, part_index: usize, tile: TileIndices) -> ReadResult<()> {
        debug_assert_ne!(tile.size.0, 0);
        debug_assert_ne!(tile.size.1, 0);

        let part_count = self.parts.len();

        let part = self.parts.get_mut(part_index)
            .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

        part.insert_block(data, tile)
    }

    pub fn compress_block(&self, compression: Compression, part: usize, tile: TileIndices) -> Result<ByteVec, WriteError> {
        debug_assert_ne!(tile.size.0, 0);
        debug_assert_ne!(tile.size.1, 0);

        let part_count = self.parts.len();

        let part = self.parts.get(part)
            .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Max(part_count)))?;

        let mut bytes = Vec::new();
        part.decompress_block(tile, &mut bytes)?;

        let bytes = compression.compress_bytes(bytes)?;
        Ok(bytes)
    }

    pub fn infer_meta_data(&self, options: WriteOptions) -> Result<MetaData, WriteError> {
        let headers: Result<Headers, WriteError> = self.parts.iter().map(|part| part.infer_header(self.display_window, self.pixel_aspect, options)).collect();

        let mut headers = headers?;
        headers.sort_by(|a,b| a.name.cmp(&b.name));

        Ok(MetaData {
            requirements: Requirements::new(
                self.minimum_version(options.tiles)?,
                headers.len(),
                match options.tiles {
                    TileOptions::ScanLineBlocks => false,
                    _ => true
                },
                self.has_long_names()?,
                false // TODO
            ),

            headers
            // headers.into_iter().map(|header| header.compute_offset_table_size(version)).collect(),
        })
    }

    pub fn minimum_version(&self, _options: TileOptions) -> Result<u8, Invalid> {
        Ok(2) // TODO pick lowest possible
    }

    pub fn has_long_names(&self) -> Result<bool, Invalid> {
        Ok(true) // TODO check all name string lengths
    }
}

impl Part {

    /// allocates all the memory necessary to hold the pixel data,
    /// zeroed out, ready to be filled with actual pixel data
    pub fn new(header: &Header) -> Self {
        match header.kind {
            None | Some(Kind::ScanLine) | Some(Kind::Tile) => {
                Part {
                    data_window: header.data_window,
//                    display_window: header.display_window,
//                    pixel_aspect: header.pixel_aspect,
                    screen_window_center: header.screen_window_center,
                    screen_window_width: header.screen_window_width,
                    name: header.name.clone(),
                    attributes: header.custom_attributes.clone(),
                    channels: header.channels.list.iter().map(|channel| Channel::new(header, channel)).collect()
                }
            },

            Some(Kind::DeepScanLine) | Some(Kind::DeepTile) => {
                unimplemented!()
            },
        }
    }

    pub fn insert_block(&mut self, data: &mut impl Read, area: TileIndices) -> ReadResult<()> {
        let level = (area.level.0 as usize, area.level.1 as usize);

        for y in area.position.1 .. area.position.1 + area.size.1 {
            for channel in &mut self.channels {
                channel.insert_line(data, level, (area.position.0 as usize, y as usize), area.size.0 as usize)?;
            }
        }

        Ok(())
    }

    pub fn decompress_block(&self, area: TileIndices, write: &mut impl Write) -> WriteResult {
        let level = (area.level.0 as usize, area.level.1 as usize);

        for y in area.position.1 .. area.position.1 + area.size.1 {
            for channel in &self.channels {
                channel.extract_line(write, level, (area.position.0 as usize, y as usize), area.size.0 as usize)?;
            }
        }

        Ok(())
    }

    pub fn infer_header(&self, display_window: I32Box2, pixel_aspect: f32, options: WriteOptions) -> Result<Header, WriteError> {
        Ok(Header {
            channels: ChannelList::new(self.channels.iter().map(|channel| attributes::Channel {
                pixel_type: match channel.content {
                    ChannelData::F16(_) => PixelType::F16,
                    ChannelData::F32(_) => PixelType::F32,
                    ChannelData::U32(_) => PixelType::U32,
                },

                name: channel.name.clone(),
                is_linear: channel.is_linear,
                reserved: [0, 0, 0],
                sampling: (channel.sampling.0 as u32, channel.sampling.1 as u32)
            }).collect()),

            data_window: self.data_window,
            screen_window_center: self.screen_window_center,
            screen_window_width: self.screen_window_width,
            compression: options.compression_method,
            line_order: options.line_order,


            tiles: match options.tiles {
                TileOptions::ScanLineBlocks => None,
                TileOptions::Tiles { size, rounding } => Some(TileDescription {
                    size, level_mode: LevelMode::Singular, // FIXME levels!
                    rounding_mode: rounding
                })
            },

            name: self.name.clone(),

            // TODO deep data:
            kind: Some(match options.tiles {
                TileOptions::ScanLineBlocks => Kind::ScanLine,
                TileOptions::Tiles { .. } => Kind::Tile,
                // TODO
            }),

            // TODO deep/multipart data:
            deep_data_version: None,
            chunk_count: None,
            max_samples_per_pixel: None,
            custom_attributes: self.attributes.clone(),
            display_window, pixel_aspect
        })
    }

    pub fn tiles(&self, header: &Header, action: &mut impl FnMut(TileIndices) -> WriteResult) -> WriteResult {
        fn tiles_of(image_size: (u32, u32), tile_size: (u32, u32), level: (u32, u32), action: &mut impl FnMut(TileIndices) -> WriteResult) -> WriteResult {
            fn divide_and_rest(total_size: u32, block_size: u32, action: &mut impl FnMut(u32, u32) -> WriteResult) -> WriteResult {
                let whole_block_count = total_size / block_size; // RoundingMode::Up.divide(total_size, block_size);

                for whole_block_index in 0 .. whole_block_count {
                    action(whole_block_index * block_size, block_size)?;
                }

                let covered_size = whole_block_count * block_size;

                if covered_size != total_size {
                    let last_position = covered_size;
                    let remaining = last_position + block_size - total_size; // FIXME min(1) should not be required, fix formula instead!

                    debug_assert_eq!(last_position + remaining, total_size);
                    action(last_position, remaining)?;
                }

                Ok(())
            }

            divide_and_rest(image_size.1, tile_size.1, &mut |y, tile_height|{
                divide_and_rest(image_size.0, tile_size.0, &mut |x, tile_width|{
                    action(TileIndices {
                        position: (x, y), level,
                        size: (tile_width, tile_height),
                    })
                })
            })
        }

        let image_size = self.data_window.dimensions();

        if let Some(tiles) = header.tiles {
            match tiles.level_mode {
                LevelMode::Singular => {
                    tiles_of(image_size, tiles.size, (0,0), action)?;
                },
                LevelMode::MipMap => {
                    for level in mip_map_resolutions(tiles.rounding_mode, image_size) {
                        tiles_of(level, tiles.size, level, action)?;
                    }
                },
                LevelMode::RipMap => {
                    for level in rip_map_resolutions(tiles.rounding_mode, image_size) {
                        tiles_of(level, tiles.size, level, action)?;
                    }
                }
            }

            Ok(())
        }
        else {
            let block_height = header.compression.scan_lines_per_block();
            tiles_of(image_size, (image_size.0, block_height), (0,0), action)

            /*let (image_width, image_height) = self.data_window.dimensions();
            let block_size = header.compression.scan_lines_per_block();
            let block_count = compute_scan_line_block_count(image_height, block_size);

            let mut data: Vec<_> = (0.. block_count - 1)
                .map(move |block_index| TileIndices {
                    level: (0, 0), position: (0, block_index * block_size),
                    size: (image_width, block_size)
                })
                .collect();

            let last_y = block_size * (block_count - 1);
            let last_height = (last_y + block_size - image_height).max(1); // FIXME min(1) should not be required, fix formula instead!
            data.push(TileIndices { // TODO level always 0,0?
                level: (0, 0), position: (0, last_y),
                size: (image_width, last_height)
            });

//            println!("blocks: {:?}", data);

            debug_assert_ne!(last_height, 0);
            debug_assert!(last_y < image_height);

            data.into_iter()*/
        }
    }
}

impl Channel {
    pub fn new(header: &Header, channel: &crate::meta::attributes::Channel) -> Self {
        Channel {
            name: channel.name.clone(),
            is_linear: channel.is_linear,
            sampling: (channel.sampling.0 as usize, channel.sampling.1 as usize),

            content: match channel.pixel_type {
                PixelType::F16 => ChannelData::F16(SampleMaps::new(header)),
                PixelType::F32 => ChannelData::F32(SampleMaps::new(header)),
                PixelType::U32 => ChannelData::U32(SampleMaps::new(header)),
            },
        }
    }

    pub fn insert_line(&mut self, block: &mut impl Read, level:(usize, usize), position: (usize, usize), length: usize) -> ReadResult<()> {
        match &mut self.content {
            ChannelData::F16(maps) => maps.insert_line(block, level, position, length),
            ChannelData::F32(maps) => maps.insert_line(block, level, position, length),
            ChannelData::U32(maps) => maps.insert_line(block, level, position, length),
        }
    }

    pub fn extract_line(&self, block: &mut impl Write, level:(usize, usize), position: (usize, usize), length: usize) -> WriteResult {
        match &self.content {
            ChannelData::F16(maps) => maps.extract_line(block, level, position, length),
            ChannelData::F32(maps) => maps.extract_line(block, level, position, length),
            ChannelData::U32(maps) => maps.extract_line(block, level, position, length),
        }
    }
}

impl<Sample: Data + std::fmt::Debug> SampleMaps<Sample> {
    pub fn new(header: &Header) -> Self {
        if header.has_deep_data() {
            SampleMaps::Deep(Levels::new(header))
        }
        else {
            SampleMaps::Flat(Levels::new(header))
        }
    }

    pub fn insert_line(&mut self, block: &mut impl Read, level:(usize, usize), position: (usize, usize), length: usize) -> ReadResult<()> {
        match self {
            SampleMaps::Deep(ref mut levels) => levels.insert_line(block, level, position, length),
            SampleMaps::Flat(ref mut levels) => levels.insert_line(block, level, position, length),
        }
    }

    pub fn extract_line(&self, block: &mut impl Write, level:(usize, usize), position: (usize, usize), length: usize) -> WriteResult {
        match self {
            SampleMaps::Deep(ref levels) => levels.extract_line(block, level, position, length),
            SampleMaps::Flat(ref levels) => levels.extract_line(block, level, position, length),
        }
    }

    pub fn flat_samples(&self) -> Option<&Levels<FlatSamples<Sample>>> {
        match self {
            SampleMaps::Flat(ref levels) => Some(levels),
            _ => None
        }
    }

    pub fn deep_samples(&self) -> Option<&Levels<DeepSamples<Sample>>> {
        match self {
            SampleMaps::Deep(ref levels) => Some(levels),
            _ => None
        }
    }
}

impl<S: Samples> Levels<S> {
    pub fn new(header: &Header) -> Self {
        let data_size = header.data_window.dimensions();

        if let Some(tiles) = &header.tiles {
//            debug_assert_eq!(header.kind, Some(Kind::Tile)); FIXME triggered
            let round = tiles.rounding_mode;

            match tiles.level_mode {
                LevelMode::Singular => Levels::Singular(SampleBlock::new(data_size)),

                LevelMode::MipMap => Levels::Mip(
                    mip_map_resolutions(round, data_size)
                        .map(|level_size| SampleBlock::new(level_size)).collect()
                ),

                // TODO put this into Levels::new(..) ?
                LevelMode::RipMap => Levels::Rip({
                    let level_count_x = compute_level_count(round, data_size.0);
                    let level_count_y = compute_level_count(round, data_size.1);
                    let maps = rip_map_resolutions(round, data_size)
                        .map(|level_size| SampleBlock::new(level_size)).collect();

                    RipMaps { map_data: maps, level_count: (level_count_x as usize, level_count_y as usize) }
                })
            }
        }

        // scan line blocks never have mip maps? // TODO check if this is true
        else {
            Levels::Singular(SampleBlock::new(data_size))
        }
    }

    pub fn insert_line(&mut self, read: &mut impl Read, level:(usize, usize), position: (usize, usize), length: usize) -> ReadResult<()> {
        match self {
            Levels::Singular(ref mut block) => {
                debug_assert_eq!(level, (0,0), "singular image cannot read leveled blocks");
                block.insert_line(read, position, length)?;
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y"); // TODO err instead?
                let max = block.len();

                block.get_mut(level.0)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .insert_line(read, position, length)?;
            },

            Levels::Rip(block) => {
                let max = block.map_data.len();

                block.get_by_level_mut(level)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .insert_line(read, position, length)?;
            }
        }

        Ok(())
    }

    pub fn extract_line(&self, write: &mut impl Write, level:(usize, usize), position: (usize, usize), length: usize) -> WriteResult {
        match self {
            Levels::Singular(ref block) => {
                debug_assert_eq!(level, (0,0), "singular image cannot write leveled blocks");
                block.extract_line(write, position, length)?;
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y"); // TODO err instead?
                let max = block.len();

                block.get(level.0)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .extract_line(write, position, length)?;
            },

            Levels::Rip(block) => {
                let max = block.map_data.len();

                block.get_by_level(level)
                    .ok_or(Invalid::Content(Value::MapLevel, Required::Max(max)))?
                    .extract_line(write, position, length)?;
            }
        }

        Ok(())
    }

    pub fn largest(&self) -> &SampleBlock<S> {
        match self {
            Levels::Singular(data) => data,
            Levels::Mip(maps) => &maps[0], // TODO is this really the largest one?
            Levels::Rip(rip_map) => &rip_map.map_data[0], // TODO test!
        }
    }

    pub fn levels(&self) -> &[SampleBlock<S>] {
        match self {
            Levels::Singular(ref data) => std::slice::from_ref(data),
            Levels::Mip(ref maps) => maps, // TODO is this really the largest one?
            Levels::Rip(ref rip_map) => &rip_map.map_data, // TODO test!
        }
    }
}


impl<S: Samples> SampleBlock<S> {
    pub fn new(resolution: (u32, u32)) -> Self {
        let resolution = (resolution.0 as usize, resolution.1 as usize);
        SampleBlock { resolution, samples: S::new(resolution) }
    }

    pub fn insert_line(&mut self, read: &mut impl Read, position: (usize, usize), length: usize) -> ReadResult<()> {
        debug_assert!(position.1 < self.resolution.1, "y: {}, height: {}", position.1, self.resolution.1);
        debug_assert!(position.0 + length <= self.resolution.0);
        debug_assert_ne!(length, 0);

        self.samples.insert_line(read, position, length, self.resolution.0)
    }

    pub fn extract_line(&self, write: &mut impl Write, position: (usize, usize), length: usize) -> WriteResult {
        debug_assert!(position.1 < self.resolution.1, "y: {}, height: {}", position.1, self.resolution.1);
        debug_assert!(position.0 + length <= self.resolution.0);
        debug_assert_ne!(length, 0);

        self.samples.extract_line(write, position, length, self.resolution.0)
    }
}

pub trait Samples {
    fn new(resolution: (usize, usize)) -> Self;
    fn insert_line(&mut self, read: &mut impl Read, position: (usize, usize), length: usize, image_width: usize) -> ReadResult<()>;
    fn extract_line(&self, write: &mut impl Write, position: (usize, usize), length: usize, image_width: usize) -> WriteResult;
}

impl<Sample: crate::io::Data> Samples for DeepSamples<Sample> {
    fn new(resolution: (usize, usize)) -> Self {
        vec![
            DeepLine { samples: Vec::new(), index_table: vec![0; resolution.0] };
            resolution.1
        ]
    }

    fn insert_line(&mut self, _read: &mut impl Read, _position: (usize, usize), length: usize, image_width: usize) -> ReadResult<()> {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        unimplemented!()

        // TODO err on invalid tile position
//        self[_position.1 as usize] = DeepLine {
//            samples: Sample::read_vec(read, length, 1024*1024*1024)?, // FIXME where tiles, will not be hole line
//            index_table: unimplemented!()
//        };
//
//        Ok(())
    }

    fn extract_line(&self, _write: &mut impl Write, _position: (usize, usize), length: usize, image_width: usize) -> WriteResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        unimplemented!()
    }
}

impl<Sample: crate::io::Data + Default + Clone + std::fmt::Debug> Samples for FlatSamples<Sample> {
    fn new(resolution: (usize, usize)) -> Self {
        let resolution = (resolution.0 as usize, resolution.1 as usize);
        vec![Sample::default(); resolution.0 * resolution.1]
    }

    fn insert_line(&mut self, read: &mut impl Read, position: (usize, usize), length: usize, image_width: usize) -> ReadResult<()> {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        let start_index = position.1 as usize * image_width + position.0 as usize;
        let end_index = start_index + length;

        Sample::read_slice(read, &mut self[start_index .. end_index])?;
        Ok(())
    }

    fn extract_line(&self, write: &mut impl Write, position: (usize, usize), length: usize, image_width: usize) -> WriteResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        let start_index = position.1 as usize * image_width + position.0 as usize;
        let end_index = start_index + length;

        Sample::write_slice(write, &self[start_index .. end_index])?;
        Ok(())
    }
}

impl<Samples> RipMaps<Samples> {
    pub fn get_level_index(&self, level: (usize, usize)) -> usize {
        self.level_count.0 * level.1 as usize + level.0 as usize  // TODO check this calculation (x vs y)
    }

    pub fn get_by_level(&self, level: (usize, usize)) -> Option<&SampleBlock<Samples>> {
        self.map_data.get(self.get_level_index(level))
    }

    pub fn get_by_level_mut(&mut self, level: (usize, usize)) -> Option<&mut SampleBlock<Samples>> {
        let index = self.get_level_index(level);
        self.map_data.get_mut(index)
    }
}
