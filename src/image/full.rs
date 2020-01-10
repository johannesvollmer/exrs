//! The `image` module is for interpreting the loaded file data.
//!

use smallvec::SmallVec;
use half::f16;
use crate::chunks::*;
use crate::io::*;
use crate::meta::*;
use crate::meta::attributes::*;
use crate::compression::{ByteVec};
use crate::error::{Result, PassiveResult, Error};
use crate::math::*;
use std::io::{Seek, SeekFrom, BufReader, Cursor, BufWriter};
use crate::io::Data;
use crate::image::{BlockOptions, WriteOptions, ReadOptions, UncompressedBlock};


#[derive(Clone, PartialEq, Debug)]
pub struct FullImage {
    pub parts: Parts,

    display_window: Box2I32,
    pixel_aspect: f32,
}

/// an exr image can store multiple parts (multiple bitmaps inside one image)
pub type Parts = SmallVec<[Part; 3]>;

#[derive(Clone, PartialEq, Debug)]
pub struct Part {
    pub name: Option<Text>,
    pub attributes: Attributes,

    pub data_window: Box2I32,
    // TODO pub data_offset: (i32, i32),

    pub screen_window_center: Vec2<f32>, // TODO use sensible defaults instead of returning an error on missing?
    pub screen_window_width: f32,

    pub compression: Compression,

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
    pub sampling: Vec2<usize>,
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
#[derive(Clone, PartialEq)]
pub enum Levels<Samples> {
    Singular(SampleBlock<Samples>),
    Mip(LevelMaps<Samples>),
    Rip(RipMaps<Samples>),
}

pub type LevelMaps<Samples> = Vec<SampleBlock<Samples>>;

#[derive(Clone, PartialEq, Debug)]
pub struct RipMaps<Samples> {
    pub map_data: LevelMaps<Samples>,
    pub level_count: Vec2<usize>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SampleBlock<Samples> {
    pub resolution: Vec2<usize>,
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


impl<S> std::fmt::Debug for Levels<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Levels::Singular(image) => write!(
                formatter, "Singular ([{}x{}])",
                image.resolution.0, image.resolution.1
            ),
            Levels::Mip(levels) => write!(
                formatter, "Mip ({:?})",
                levels.iter().map(|level| level.resolution).collect::<Vec<_>>(),
            ),
            Levels::Rip(maps) => write!(
                formatter, "Rip ({:?})",
                maps.map_data.iter().map(|level| level.resolution).collect::<Vec<_>>()
            )
        }
    }
}


impl FullImage {
    // TODO also return the corresponding WriteOptions which can be used to write the most similar file to this?
    #[must_use]
    pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions) -> Result<Self> {
        Self::read_from_unbuffered(std::fs::File::open(path)?, options)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it.
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read, options: ReadOptions) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered), options)
    }



    #[must_use]
    pub fn write_to_file(&self, path: impl AsRef<std::path::Path>, options: WriteOptions) -> PassiveResult {
        self.write_to_unbuffered(std::fs::File::create(path)?, options)
    }

    /// needs more memory but allows for non-seeking write operations
    #[must_use]
    pub fn write_without_seek(&self, mut unbuffered: impl Write, options: WriteOptions) -> PassiveResult {
        let mut bytes = Vec::new();

        // write the image to the seekable vec
        self.write_to_buffered(Cursor::new(&mut bytes), options)?;

        // write the vec into the actual output
        unbuffered.write_all(&bytes)?;
        Ok(())
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn write_to_unbuffered(&self, unbuffered: impl Write + Seek, options: WriteOptions) -> PassiveResult {
        self.write_to_buffered(BufWriter::new(unbuffered), options)
    }

    /// assumes the reader is buffered (if desired)
    #[must_use]
    pub fn read_from_buffered(read: impl Read, options: ReadOptions) -> Result<Self> {
        crate::image::read_all_chunks(read, options, FullImage::new, FullImage::with_block)
    }


    /// assumes the reader is buffered
    #[must_use]
    pub fn write_to_buffered(&self, mut write: impl Write + Seek, options: WriteOptions) -> PassiveResult {
        let meta_data = self.infer_meta_data(options)?;
        meta_data.write(&mut write)?;

        let offset_table_start_byte = write.seek(SeekFrom::Current(0))?;

        // skip offset tables for now
        let offset_table_size: u32 = meta_data.headers.iter()
            .map(|header| header.chunk_count).sum();

//        println!("writing zeroed tables (chunk count: {})", offset_table_size);

        let mut offset_tables: Vec<u64> = vec![0; offset_table_size as usize];
        u64::write_slice(&mut write, offset_tables.as_slice())?;
        offset_tables.clear();

        if !options.parallel_compression {
            for (part_index, part) in self.parts.iter().enumerate() {
                let header = &meta_data.headers[part_index];
                let mut table = Vec::new();

//                println!("writing image part {}", part_index);

                part.tiles(header, &mut |tile| {
                    let data_indices = header.get_absolute_block_indices(tile.location)?;

                    let data_size = Vec2::try_from(data_indices.size).unwrap();
                    let data_position = Vec2::try_from(data_indices.start).unwrap();
                    let data_level = Vec2::try_from(tile.location.level_index).unwrap();

                    let data: Vec<u8> = self.extract_block(part_index, data_position, data_size, data_level)?;

                    let data = header.compression.compress_image_section(data)?;

                    let chunk = Chunk {
                        part_number: part_index as i32,

                        // TODO deep data
                        block: match options.blocks {
                            BlockOptions::ScanLineBlocks => Block::ScanLine(ScanLineBlock {
                                y_coordinate: header.get_block_data_window_coordinates(tile.location)?.start.1,
                                    // part.data_window.y_min + (tile.index.1 * header.compression.scan_lines_per_block()) as i32,
                                compressed_pixels: data
                            }),

                            BlockOptions::TileBlocks { .. } => Block::Tile(TileBlock {
                                compressed_pixels: data,
                                coordinates: tile.location,
                            }),
                        }
                    };

                    let block_start_position = write.seek(SeekFrom::Current(0))?;
                    table.push((tile, block_start_position));


                    chunk.write(&mut write, meta_data.headers.as_slice())?;

                    Ok(())
                })?;

                // sort offset table by increasing y
                table.sort_by(|(a, _), (b, _)| a.cmp(b));
//                println!("write single table with len {} {:?}", table.len(), table);

                offset_tables.extend(table.into_iter().map(|(_, index)| index));
            }
        }
        else {
            return Err(Error::unsupported("parallel compression"));
        }

        // write offset tables after all blocks have been written
        debug_assert_eq!(offset_tables.len(), offset_table_size as usize);
        write.seek(SeekFrom::Start(offset_table_start_byte))?;
        u64::write_slice(&mut write, offset_tables.as_slice())?;

        Ok(())
    }
}

impl FullImage {
    pub fn new(headers: &[Header]) -> Result<Self> {
        let mut display = headers.iter()
            .map(|header| header.display_window);

        // FIXME check all display windows are the same
        let display_window = display.next().unwrap();

        let mut pixel_aspect = headers.iter()
            .map(|header| header.pixel_aspect);

        // FIXME check all display windows are the same
        let pixel_aspect = pixel_aspect.next().unwrap();

        let headers : Result<_> = headers.iter().map(Part::new).collect();

        Ok(FullImage {
            parts: headers?,
            display_window, pixel_aspect
        })
    }

    pub fn with_block(mut self, block: UncompressedBlock) -> Result<Self> {
        self.insert_block(block)?;
        Ok(self)
    }

    pub fn insert_block(&mut self, block: UncompressedBlock) -> PassiveResult {
        debug_assert_ne!(block.data_size.0, 0);
        debug_assert_ne!(block.data_size.1, 0);

        let part = self.parts.get_mut(block.part_index)
            .ok_or(Error::invalid("chunk part index"))?;

        part.insert_block(&mut block.data.as_slice(), block.data_index, block.data_size, block.level)
    }

    pub fn extract_block(&self, part: usize, position: Vec2<usize>, size: Vec2<usize>, level: Vec2<usize>) -> Result<ByteVec> {
        debug_assert_ne!(size.0, 0);
        debug_assert_ne!(size.1, 0);

        let part = self.parts.get(part)
            .ok_or(Error::invalid("chunk part index"))?;

        let mut bytes = Vec::new();
        part.extract_block(position, size, level, &mut bytes)?;
        Ok(bytes)
    }

    pub fn infer_meta_data(&self, options: WriteOptions) -> Result<MetaData> {
        let headers: Result<Headers> = self.parts.iter().map(|part| part.infer_header(self.display_window, self.pixel_aspect, options)).collect();

        let mut headers = headers?;
        headers.sort_by(|a,b| a.name.cmp(&b.name));

        Ok(MetaData {
            requirements: Requirements::new(
                self.minimum_version(options.blocks)?,
                headers.len(),
                match options.blocks {
                    BlockOptions::ScanLineBlocks => false,
                    _ => true
                },
                self.has_long_names()?,
                false // TODO
            ),

            headers
        })
    }

    pub fn minimum_version(&self, _options: BlockOptions) -> Result<u8> {
        Ok(2) // TODO pick lowest possible
    }

    pub fn has_long_names(&self) -> Result<bool> {
        Ok(true) // TODO check all name string lengths
    }
}

impl Part {

    /// allocates all the memory necessary to hold the pixel data,
    /// zeroed out, ready to be filled with actual pixel data
    pub fn new(header: &Header) -> Result<Self> {
        match header.block_type {
            BlockType::ScanLine | BlockType::Tile => {
                Ok(Part {
                    data_window: header.data_window,
                    screen_window_center: header.screen_window_center,
                    screen_window_width: header.screen_window_width,
                    name: header.name.clone(),
                    attributes: header.custom_attributes.clone(),
                    channels: header.channels.list.iter().map(|channel| Channel::new(header, channel)).collect(),
                    compression: header.compression
                })
            },

            BlockType::DeepScanLine | BlockType::DeepTile => {
                return Err(Error::unsupported("deep data"))
            },
        }
    }

    pub fn insert_block(&mut self, data: &mut impl Read, position: Vec2<usize>, size: Vec2<usize>, level: Vec2<usize>) -> PassiveResult {
        debug_assert!(position.0 + size.0 <= self.data_window.size.0 as usize);
        debug_assert!(position.1 + size.1 <= self.data_window.size.1 as usize);

//        println!("position: {:?}, size: {:?}, image: {:?}", position, size, self.data_window);

        for y in position.1 .. position.1 + size.1 {
//            println!("\ty: {}", y);
            for channel in &mut self.channels {
                channel.insert_line(data, level, Vec2(position.0, y), size.0)?;
            }
        }

        Ok(())
    }

    pub fn extract_block(&self, position: Vec2<usize>, size: Vec2<usize>, level: Vec2<usize>, write: &mut impl Write) -> PassiveResult {
        for y in position.1 .. position.1 + size.1 {
            for channel in &self.channels {
                channel.extract_line(write, level, Vec2(position.0, y), size.0)?;
            }
        }

        Ok(())
    }

    pub fn infer_header(&self, display_window: Box2I32, pixel_aspect: f32, options: WriteOptions) -> Result<Header> {
        assert_eq!(options.line_order, LineOrder::Unspecified);
        let tiles = match options.blocks {
            BlockOptions::ScanLineBlocks => None,
            BlockOptions::TileBlocks { size, rounding } => Some(TileDescription {
                tile_size: size, level_mode: LevelMode::Singular, // FIXME levels!
                rounding_mode: rounding
            })
        };

        let chunk_count = compute_chunk_count(
            self.compression, self.data_window, tiles
        )?;

        Ok(Header {
            tiles, chunk_count,

            data_window: self.data_window,
            screen_window_center: self.screen_window_center,
            screen_window_width: self.screen_window_width,
            compression: self.compression,
            name: self.name.clone(),

            channels: ChannelList::new(self.channels.iter().map(|channel| attributes::Channel {
                pixel_type: match channel.content {
                    ChannelData::F16(_) => PixelType::F16,
                    ChannelData::F32(_) => PixelType::F32,
                    ChannelData::U32(_) => PixelType::U32,
                },

                name: channel.name.clone(),
                is_linear: channel.is_linear,
                sampling: Vec2::try_from(channel.sampling).unwrap()
            }).collect()),

            line_order: LineOrder::Unspecified, // TODO

            block_type: match options.blocks { // TODO only write if necessary?
                BlockOptions::ScanLineBlocks => BlockType::ScanLine,
                BlockOptions::TileBlocks { .. } => BlockType::Tile,
                // TODO deep data
            },


            // TODO deep/multipart data:
            deep_data_version: None,
            max_samples_per_pixel: None,
            custom_attributes: self.attributes.clone(),
            display_window, pixel_aspect
        })
    }


    // FIXME return iter instead
    pub fn tiles(&self, header: &Header, action: &mut impl FnMut(TileIndices) -> PassiveResult) -> PassiveResult {
        fn tiles_of(image_size: Vec2<u32>, tile_size: Vec2<u32>, level: Vec2<u32>, action: &mut impl FnMut(TileIndices) -> PassiveResult) -> PassiveResult {
            fn divide_and_rest(total_size: u32, block_size: u32, action: &mut impl FnMut(u32, u32) -> PassiveResult) -> PassiveResult {
                let whole_block_count = total_size / block_size;
                for whole_block_index in 0 .. whole_block_count {
                    action(whole_block_index, block_size)?;
                }

                let whole_block_size = whole_block_count * block_size;
                if whole_block_size != total_size {
                    action(whole_block_count, total_size - whole_block_size)?;
                }

                Ok(())
            }

            divide_and_rest(image_size.1, tile_size.1, &mut |y_index, tile_height|{
                divide_and_rest(image_size.0, tile_size.0, &mut |x_index, tile_width|{
                    action(TileIndices {
                        location: TileCoordinates {
                            tile_index: Vec2::try_from(Vec2(x_index, y_index)).unwrap(),
                            level_index: Vec2::try_from(level).unwrap(),
                        },
                        size: Vec2(tile_width, tile_height),
                    })
                })
            })
        }

        let image_size = self.data_window.size;

        if let Some(tiles) = header.tiles {
            match tiles.level_mode {
                LevelMode::Singular => {
                    tiles_of(image_size, tiles.tile_size, Vec2(0, 0), action)?;
                },
                LevelMode::MipMap => {
                    for level in mip_map_resolutions(tiles.rounding_mode, image_size) {
                        tiles_of(image_size, tiles.tile_size, level, action)?;
                    }
                },
                LevelMode::RipMap => {
                    for level in rip_map_resolutions(tiles.rounding_mode, image_size) {
                        tiles_of(image_size, tiles.tile_size, level, action)?;
                    }
                }
            }
        }
        else {
            tiles_of(image_size, Vec2(image_size.0, header.compression.scan_lines_per_block()), Vec2(0,0), action)?;
        }


        Ok(())
    }
}

impl Channel {
    pub fn new(header: &Header, channel: &crate::meta::attributes::Channel) -> Self {
        Channel {
            name: channel.name.clone(),
            is_linear: channel.is_linear,
            sampling: Vec2::try_from(channel.sampling).unwrap(),  // (channel.sampling.0 as usize, channel.sampling.1 as usize),

            content: match channel.pixel_type {
                PixelType::F16 => ChannelData::F16(SampleMaps::new(header)),
                PixelType::F32 => ChannelData::F32(SampleMaps::new(header)),
                PixelType::U32 => ChannelData::U32(SampleMaps::new(header)),
            },
        }
    }

    pub fn insert_line(&mut self, block: &mut impl Read, level: Vec2<usize>, position: Vec2<usize>, length: usize) -> PassiveResult {
        match &mut self.content {
            ChannelData::F16(maps) => maps.insert_line(block, level, position, length),
            ChannelData::F32(maps) => maps.insert_line(block, level, position, length),
            ChannelData::U32(maps) => maps.insert_line(block, level, position, length),
        }
    }

    pub fn extract_line(&self, block: &mut impl Write, level: Vec2<usize>, position: Vec2<usize>, length: usize) -> PassiveResult {
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

    pub fn insert_line(&mut self, block: &mut impl Read, level: Vec2<usize>, position: Vec2<usize>, length: usize) -> PassiveResult {
        match self {
            SampleMaps::Deep(ref mut levels) => levels.insert_line(block, level, position, length),
            SampleMaps::Flat(ref mut levels) => levels.insert_line(block, level, position, length),
        }
    }

    pub fn extract_line(&self, block: &mut impl Write, level: Vec2<usize>, position: Vec2<usize>, length: usize) -> PassiveResult {
        match self {
            SampleMaps::Deep(ref levels) => levels.extract_line(block, level, position, length),
            SampleMaps::Flat(ref levels) => levels.extract_line(block, level, position, length),
        }
    }

    pub fn as_flat_samples(&self) -> Option<&Levels<FlatSamples<Sample>>> {
        match self {
            SampleMaps::Flat(ref levels) => Some(levels),
            _ => None
        }
    }

    pub fn as_deep_samples(&self) -> Option<&Levels<DeepSamples<Sample>>> {
        match self {
            SampleMaps::Deep(ref levels) => Some(levels),
            _ => None
        }
    }
}

impl<S: Samples> Levels<S> {
    pub fn new(header: &Header) -> Self {
        let data_size = header.data_window.size;

        if let Some(tiles) = &header.tiles {
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

                    RipMaps { map_data: maps, level_count: Vec2::try_from(Vec2(level_count_x, level_count_y)).unwrap() }// Vec2(level_count_x as usize, level_count_y as usize) }
                })
            }
        }

        // scan line blocks never have mip maps? // TODO check if this is true
        else {
            Levels::Singular(SampleBlock::new(data_size))
        }
    }

    pub fn insert_line(&mut self, read: &mut impl Read, level: Vec2<usize>, position: Vec2<usize>, length: usize) -> PassiveResult {
//        println!("level {:?}, dimensions: {:?}", level, self.get_level(level).unwrap().resolution);
        self.get_level_mut(level)?.insert_line(read, position, length)
    }

    pub fn extract_line(&self, write: &mut impl Write, level: Vec2<usize>, position: Vec2<usize>, length: usize) -> PassiveResult {
        self.get_level(level)?.extract_line(write, position, length)
    }

    pub fn get_level(&self, level: Vec2<usize>) -> Result<&SampleBlock<S>> {
        match self {
            Levels::Singular(ref block) => {
                debug_assert_eq!(level, Vec2(0,0), "singular image cannot write leveled blocks");
                Ok(block)
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y"); // TODO err instead?
                block.get(level.0).ok_or(Error::invalid("block mip level index"))
            },

            Levels::Rip(block) => {
                block.get_by_level(level).ok_or(Error::invalid("block rip level index"))
            }
        }
    }

    pub fn get_level_mut(&mut self, level: Vec2<usize>) -> Result<&mut SampleBlock<S>> {
        match self {
            Levels::Singular(ref mut block) => {
                debug_assert_eq!(level, Vec2(0,0), "singular image cannot write leveled blocks");
                Ok(block)
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y"); // TODO err instead?
                block.get_mut(level.0).ok_or(Error::invalid("block mip level index"))
            },

            Levels::Rip(block) => {
                block.get_by_level_mut(level).ok_or(Error::invalid("block rip level index"))
            }
        }
    }

    pub fn largest(&self) -> Result<&SampleBlock<S>> {
        self.get_level(Vec2(0,0))
    }

    pub fn as_slice(&self) -> &[SampleBlock<S>] {
        match self {
            Levels::Singular(ref data) => std::slice::from_ref(data),
            Levels::Mip(ref maps) => maps, // TODO is this really the largest one?
            Levels::Rip(ref rip_map) => &rip_map.map_data, // TODO test!
        }
    }
}


impl<S: Samples> SampleBlock<S> {
    pub fn new(resolution: Vec2<u32>) -> Self {
        let resolution = Vec2::try_from(resolution).unwrap();
        SampleBlock { resolution, samples: S::new(resolution) }
    }

    pub fn insert_line(&mut self, read: &mut impl Read, position: Vec2<usize>, length: usize) -> PassiveResult {
        debug_assert!(position.0 + length <= self.resolution.0, "x max {}, of {}", position.0 + length, self.resolution.0);
        debug_assert!(position.1 < self.resolution.1, "y: {}, height: {}", position.1, self.resolution.1);
        debug_assert_ne!(length, 0);

        self.samples.insert_line(read, position, length, self.resolution.0)
    }

    pub fn extract_line(&self, write: &mut impl Write, position: Vec2<usize>, length: usize) -> PassiveResult {
        debug_assert!(position.0 + length <= self.resolution.0, "x max {} of width {}", position.0 + length, self.resolution.0);
        debug_assert!(position.1 < self.resolution.1, "y: {}, height: {}", position.1, self.resolution.1);
        debug_assert_ne!(length, 0);

        self.samples.extract_line(write, position, length, self.resolution.0)
    }
}

pub trait Samples {
    fn new(resolution: Vec2<usize>) -> Self;
    fn insert_line(&mut self, read: &mut impl Read, position: Vec2<usize>, length: usize, image_width: usize) -> PassiveResult;
    fn extract_line(&self, write: &mut impl Write, position: Vec2<usize>, length: usize, image_width: usize) -> PassiveResult;
}

impl<Sample: crate::io::Data> Samples for DeepSamples<Sample> {
    fn new(resolution: Vec2<usize>) -> Self {
        vec![
            DeepLine { samples: Vec::new(), index_table: vec![0; resolution.0] };
            resolution.1
        ]
    }

    fn insert_line(&mut self, _read: &mut impl Read, _position: Vec2<usize>, length: usize, image_width: usize) -> PassiveResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        Err(Error::unsupported("deep data"))

        // TODO err on invalid tile position
//        self[_position.1 as usize] = DeepLine {
//            samples: Sample::read_vec(read, length, 1024*1024*1024)?, // FIXME where tiles, will not be hole line
//            index_table:
//        };
//
//        Ok(())
    }

    fn extract_line(&self, _write: &mut impl Write, _position: Vec2<usize>, length: usize, image_width: usize) -> PassiveResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        Err(Error::unsupported("deep data"))
    }
}

impl<Sample: crate::io::Data + Default + Clone + std::fmt::Debug> Samples for FlatSamples<Sample> {
    fn new(resolution: Vec2<usize>) -> Self {
        let resolution = (resolution.0, resolution.1);
        vec![Sample::default(); resolution.0 * resolution.1]
    }

    fn insert_line(&mut self, read: &mut impl Read, position: Vec2<usize>, length: usize, image_width: usize) -> PassiveResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        let start_index = position.1 * image_width + position.0;
        let end_index = start_index + length;

        Sample::read_slice(read, &mut self[start_index .. end_index])?;
        Ok(())
    }

    fn extract_line(&self, write: &mut impl Write, position: Vec2<usize>, length: usize, image_width: usize) -> PassiveResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(length, 0);

        let start_index = position.1 * image_width + position.0;
        let end_index = start_index + length;

        Sample::write_slice(write, &self[start_index .. end_index])?;
        Ok(())
    }
}

impl<Samples> RipMaps<Samples> {
    pub fn get_level_index(&self, level: Vec2<usize>) -> usize {
        self.level_count.0 * level.1 + level.0  // TODO check this calculation (x vs y)
    }

    pub fn get_by_level(&self, level: Vec2<usize>) -> Option<&SampleBlock<Samples>> {
        self.map_data.get(self.get_level_index(level))
    }

    pub fn get_by_level_mut(&mut self, level: Vec2<usize>) -> Option<&mut SampleBlock<Samples>> {
        let index = self.get_level_index(level);
        self.map_data.get_mut(index)
    }
}
