//! The `image` module is for interpreting the loaded file data.
//!
// TODO documentation

use smallvec::SmallVec;
use half::f16;
use crate::io::*;
use crate::meta::*;
use crate::meta::attributes::*;
use crate::error::{Result, PassiveResult, Error};
use crate::math::*;
use std::io::{Seek, BufReader, Cursor, BufWriter};
use crate::io::Data;
use crate::image::{WriteOptions, ReadOptions, Line, LineIndex};


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

    pub line_order: LineOrder,
    pub compression: Compression,
    pub blocks: Blocks,

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
    pub fn read_from_unbuffered(unbuffered: impl Read + Send, options: ReadOptions) -> Result<Self> {
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
    pub fn read_from_buffered(read: impl Read + Send, options: ReadOptions) -> Result<Self> {
        crate::image::read_all_lines(read, options, FullImage::new, FullImage::insert_line)
    }


    /// assumes the reader is buffered
    #[must_use]
    pub fn write_to_buffered(&self, write: impl Write + Seek, options: WriteOptions) -> PassiveResult {
        crate::image::write_all_lines(
            write, options, self.infer_meta_data()?,
            |location| {
                let mut bytes = Vec::new(); // TODO avoid allocation for each line?
                self.extract_line(location, &mut bytes)?;
                Ok(bytes)
            }
        )
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

    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        debug_assert_ne!(line.location.width, 0);

        let part = self.parts.get_mut(line.location.part)
            .ok_or(Error::invalid("chunk part index"))?;

        part.insert_line(line)
    }

    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) -> PassiveResult {
        debug_assert_ne!(index.width, 0);

        let part = self.parts.get(index.part)
            .ok_or(Error::invalid("chunk part index"))?;

        part.extract_line(index, write)
    }

    pub fn infer_meta_data(&self) -> Result<MetaData> {
        let headers: Result<Headers> = self.parts.iter().map(|part| part.infer_header(self.display_window, self.pixel_aspect)).collect();

        let headers = headers?;
        let has_tiles = headers.iter().any(|header| header.blocks.has_tiles());

        Ok(MetaData {
            requirements: Requirements::new(
                self.minimum_version()?,
                headers.len() > 1,
                has_tiles,
                self.has_long_names()?,
                false // TODO
            ),

            headers
        })
    }

    pub fn minimum_version(&self) -> Result<u8> {
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
        Ok(Part {
            data_window: header.data_window,
            screen_window_center: header.screen_window_center,
            screen_window_width: header.screen_window_width,
            name: header.name.clone(),
            attributes: header.custom_attributes.clone(),
            channels: header.channels.list.iter().map(|channel| Channel::new(header, channel)).collect(),
            compression: header.compression,
            blocks: header.blocks,
            line_order: header.line_order,
        })
    }

    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        debug_assert!(line.location.position.0 + line.location.width <= self.data_window.size.0 as usize);
        debug_assert!(line.location.position.1 < self.data_window.size.1 as usize);

        self.channels.get_mut(line.location.channel)
            .expect("invalid channel index")
            .insert_line(line)
    }

    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) -> PassiveResult {
        debug_assert!(index.position.0 + index.width <= self.data_window.size.0 as usize);
        debug_assert!(index.position.1 < self.data_window.size.1 as usize);

        self.channels.get(index.channel)
            .expect("invalid channel index")
            .extract_line(index, write)
    }

    pub fn infer_header(&self, display_window: Box2I32, pixel_aspect: f32) -> Result<Header> {
//      TODO  assert!(self.channels.is_sorted_by_key(|c| c.name));

        let chunk_count = compute_chunk_count(
            self.compression, self.data_window, self.blocks
        )?;

        Ok(Header {
            chunk_count,

            data_window: self.data_window,
            screen_window_center: self.screen_window_center,
            screen_window_width: self.screen_window_width,
            compression: self.compression,
            blocks: self.blocks,
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

            line_order: self.line_order,


            // TODO deep/multipart data:
            deep_data_version: None,
            max_samples_per_pixel: None,
            custom_attributes: self.attributes.clone(),
            display_window, pixel_aspect,
            deep: false
        })
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

    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        match &mut self.content {
            ChannelData::F16(maps) => maps.insert_line(line),
            ChannelData::F32(maps) => maps.insert_line(line),
            ChannelData::U32(maps) => maps.insert_line(line),
        }
    }

    pub fn extract_line(&self, index: LineIndex, block: &mut impl Write) -> PassiveResult {
        match &self.content {
            ChannelData::F16(maps) => maps.extract_line(index, block),
            ChannelData::F32(maps) => maps.extract_line(index, block),
            ChannelData::U32(maps) => maps.extract_line(index, block),
        }
    }
}


impl<Sample: Data + std::fmt::Debug> SampleMaps<Sample> {
    pub fn new(header: &Header) -> Self {
        if header.deep {
            SampleMaps::Deep(Levels::new(header))
        }
        else {
            SampleMaps::Flat(Levels::new(header))
        }
    }

    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        match self {
            SampleMaps::Deep(ref mut levels) => levels.insert_line(line),
            SampleMaps::Flat(ref mut levels) => levels.insert_line(line),
        }
    }

    pub fn extract_line(&self, index: LineIndex, block: &mut impl Write) -> PassiveResult {
        match self {
            SampleMaps::Deep(ref levels) => levels.extract_line(index, block),
            SampleMaps::Flat(ref levels) => levels.extract_line(index, block),
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

    pub fn level_mode(&self) -> LevelMode {
        match self {
            SampleMaps::Flat(levels) => levels.level_mode(),
            SampleMaps::Deep(levels) => levels.level_mode(),
        }
    }
}

impl<S: Samples> Levels<S> {
    pub fn new(header: &Header) -> Self {
        let data_size = header.data_window.size;

        if let Blocks::Tiles(tiles) = &header.blocks {
            let round = tiles.rounding_mode;

            match tiles.level_mode {
                LevelMode::Singular => Levels::Singular(SampleBlock::new(data_size)),

                LevelMode::MipMap => Levels::Mip(
                    mip_map_levels(round, data_size)
                        .map(|(_, level_size)| SampleBlock::new(level_size)).collect()
                ),

                // TODO put this into Levels::new(..) ?
                LevelMode::RipMap => Levels::Rip({
                    let level_count_x = compute_level_count(round, data_size.0);
                    let level_count_y = compute_level_count(round, data_size.1);
                    let maps = rip_map_levels(round, data_size)
                        .map(|(_, level_size)| SampleBlock::new(level_size)).collect();

                    RipMaps { map_data: maps, level_count: Vec2::try_from(Vec2(level_count_x, level_count_y)).unwrap() }
                })
            }
        }

        // scan line blocks never have mip maps? // TODO check if this is true
        else {
            Levels::Singular(SampleBlock::new(data_size))
        }
    }

    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        self.get_level_mut(line.location.level)?.insert_line(line)
    }

    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) -> PassiveResult {
        self.get_level(index.level)?.extract_line(index, write)
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


    pub fn level_mode(&self) -> LevelMode {
        match self {
            Levels::Singular(_) => LevelMode::Singular,
            Levels::Mip(_) => LevelMode::MipMap,
            Levels::Rip(_) => LevelMode::RipMap,
        }
    }
}


impl<S: Samples> SampleBlock<S> {
    pub fn new(resolution: Vec2<u32>) -> Self {
        let resolution = Vec2::try_from(resolution).unwrap();
        SampleBlock { resolution, samples: S::new(resolution) }
    }

    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        debug_assert_ne!(line.location.width, 0);

        if line.location.position.0 + line.location.width > self.resolution.0 {
            return Err(Error::invalid("data block x coordinate"))
        }

        if line.location.position.1 > self.resolution.1 {
            return Err(Error::invalid("data block y coordinate"))
        }

        self.samples.insert_line(line, self.resolution.0)
    }

    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) -> PassiveResult {
        debug_assert!(index.position.0 + index.width <= self.resolution.0, "x max {} of width {}", index.position.0 + index.width, self.resolution.0); // TODO this should Err() instead
        debug_assert!(index.position.1 < self.resolution.1, "y: {}, height: {}", index.position.1, self.resolution.1);
        debug_assert_ne!(index.width, 0);

        self.samples.extract_line(index, write, self.resolution.0)
    }
}

pub trait Samples {
    fn new(resolution: Vec2<usize>) -> Self;
    fn insert_line(&mut self, line: Line<'_>, image_width: usize) -> PassiveResult;
    fn extract_line(&self, index: LineIndex, write: &mut impl Write, image_width: usize) -> PassiveResult;
}

impl<Sample: crate::io::Data> Samples for DeepSamples<Sample> {
    fn new(resolution: Vec2<usize>) -> Self {
        vec![
            DeepLine { samples: Vec::new(), index_table: vec![0; resolution.0] };
            resolution.1
        ]
    }

    fn insert_line(&mut self, _line: Line<'_>, _width: usize) -> PassiveResult {
//        debug_assert_ne!(image_width, 0);
//        debug_assert_ne!(length, 0);

        Err(Error::unsupported("deep data"))

        // TODO err on invalid tile position
//        self[_position.1 as usize] = DeepLine {
//            samples: Sample::read_vec(read, length, 1024*1024*1024)?, // FIXME where tiles, will not be hole line
//            index_table:
//        };
//
//        Ok(())
    }

    fn extract_line(&self, index: LineIndex, _write: &mut impl Write, image_width: usize) -> PassiveResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(index.width, 0);

        Err(Error::unsupported("deep data"))
    }
}

impl<Sample: crate::io::Data + Default + Clone + std::fmt::Debug> Samples for FlatSamples<Sample> {
    fn new(resolution: Vec2<usize>) -> Self {
        let resolution = (resolution.0, resolution.1);
        vec![Sample::default(); resolution.0 * resolution.1]
    }

    fn insert_line(&mut self, line: Line<'_>, image_width: usize) -> PassiveResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(line.location.width, 0);

        let start_index = line.location.position.1 * image_width + line.location.position.0;
        let end_index = start_index + line.location.width;

        line.read_samples(&mut self[start_index .. end_index])
    }

    fn extract_line(&self, index: LineIndex, write: &mut impl Write, image_width: usize) -> PassiveResult {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(index.width, 0);

        let start_index = index.position.1 * image_width + index.position.0;
        let end_index = start_index + index.width;

        LineIndex::write_samples(&self[start_index .. end_index], write)
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
