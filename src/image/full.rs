
//! Read and write all supported aspects of an exr image, including deep data and multiresolution levels.
//! Use `exr::image::simple` if you do not need deep data or resolution levels.
//!
//! __This module is currently under construction.__
//! It will be made public as soon as deep data is supported.

// Tasks:
// - [x] fix channel sampling allocation size
// - [ ] nice api to construct and inspect images
//      - [ ] validation
// - [ ] deep data

#![doc(hidden)]

use smallvec::SmallVec;
use half::f16;
use crate::io::*;
use crate::meta::*;
use crate::meta::attributes::*;
use crate::error::{Result, PassiveResult, Error};
use crate::math::*;
use std::io::{Seek, BufReader, BufWriter};
use crate::io::Data;
use crate::image::{Line, LineIndex};

// FIXME this needs some of the changes that were made in simple.rs !!!

/// Specify how to write an exr image.
/// Contains several `override` fields,
/// that, if set, take precedence over
/// the regular image properties.
/// They can be used to write an image with a different
/// configuration than it was read with.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {

    /// Enable multicore compression.
    pub parallel_compression: bool,

    /// Override the line order of all headers in the image.
    pub override_line_order: Option<LineOrder>,

    /// Override the block type of all headers in the image.
    pub override_blocks: Option<Blocks>,

    /// Override the compression method of all headers in the image.
    pub override_compression: Option<Compression>,
}

/// Specify how to read an exr image.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReadOptions {

    /// Enable multicore decompression.
    pub parallel_decompression: bool,
}

/// An exr image.
///
/// Supports all possible exr image features.
/// An exr image may contain multiple image parts.
/// All meta data is encoded in this image,
/// including custom attributes.
#[derive(Clone, PartialEq, Debug)]
pub struct Image {

    /// All image parts contained in the image file
    pub parts: Parts,

    /// The rectangle positioned anywhere in the infinite 2D space that
    /// clips all contents of the file, limiting what should be rendered.
    pub display_window: IntRect,

    /// Aspect ratio of each pixel in this image part.
    pub pixel_aspect: f32,
}

pub type Parts = SmallVec<[Part; 3]>;

/// A single image part of an exr image.
/// Contains meta data and actual pixel information of the channels.
#[derive(Clone, PartialEq, Debug)]
pub struct Part {

    /// The name of the image part.
    /// This is optional for files with only one image part.
    pub name: Option<Text>,

    /// The remaining attributes which are not already in the `Part`.
    /// Includes custom attributes.
    pub attributes: Attributes,

    /// The rectangle that positions this image part
    /// within the global infinite 2D space of the file.
    pub data_window: IntRect,

    /// Part of the perspective projection. Default should be `(0, 0)`.
    pub screen_window_center: Vec2<f32>,

    /// Part of the perspective projection. Default should be `1`.
    pub screen_window_width: f32,

    /// In what order the tiles of this header occur in the file.
    pub line_order: LineOrder,

    /// How the pixel data of all channels in this image part is compressed. May be `Compression::Uncompressed`.
    pub compression: Compression,

    /// Describes how the pixels of this image part are divided into smaller blocks in the file.
    /// A single block can be loaded without processing all bytes of a file.
    ///
    /// Also describes whether a file contains multiple resolution levels: mip maps or rip maps.
    /// This allows loading not the full resolution, but the smallest sensible resolution.
    pub blocks: Blocks,

    /// List of channels in this image part.
    /// Contains the actual pixel data of the image.
    pub channels: Channels,
}


pub type Channels = SmallVec<[Channel; 5]>;

/// Contains an arbitrary list of pixel data.
/// Each channel can have a different pixel type,
/// either f16, f32, or u32.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {

    /// One of "R", "G", or "B" most of the time.
    pub name: Text,

    /// Actual pixel data.
    pub content: ChannelData,

    /// Are the samples in this channel in linear color space?
    pub is_linear: bool,

    /// How many of the samples are skipped compared to the other channels in this image part.
    ///
    /// Can be used for chroma subsampling for manual lossy data compression.
    /// Values other than 1 are allowed only in flat, scan-line based images.
    /// If an image is deep or tiled, x and y sampling rates for all of its channels must be 1.
    pub sampling: Vec2<usize>,
}

/// Actual pixel data in a channel. Is either one of f16, f32, or u32.
#[derive(Clone, Debug, PartialEq)]
pub enum ChannelData {
    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction.
    ///
    /// Currently this crate is using the `half` crate, which is an implementation of the IEEE 754-2008 standard, meeting that requirement.
    F16(SampleMaps<f16>),

    /// 32-bit float samples.
    F32(SampleMaps<f32>),

    /// 32-bit unsigned int samples.
    /// Used for segmentation of image parts.
    U32(SampleMaps<u32>),
}

/// Contains either deep data or flat data.
#[derive(Clone, Debug, PartialEq)]
pub enum SampleMaps<Sample> {

    /// Each pixel has one value per channel.
    Flat (Levels<FlatSamples<Sample>>),

    /// Each pixel has an arbitrary number of samples per channel.
    Deep (Levels<DeepSamples<Sample>>),
}

/// The different resolution levels of the image part.
// FIXME should be descending and starting with full-res instead!
#[derive(Clone, PartialEq)]
pub enum Levels<Samples> {

    /// Just the full resolution image.
    Singular(SampleBlock<Samples>),

    /// In addition to the full resolution image,
    /// this part also contains smaller versions with the same aspect ratio.
    Mip(LevelMaps<Samples>),

    /// In addition to the full resolution image,
    /// this part also contains smaller versions,
    /// and each smaller version has further versions with varying aspect ratios.
    Rip(RipMaps<Samples>),
}

pub type LevelMaps<Samples> = Vec<SampleBlock<Samples>>;

/// In addition to the full resolution image,
/// this part also contains smaller versions,
/// and each smaller version has further versions with varying aspect ratios.
#[derive(Clone, PartialEq, Debug)]
pub struct RipMaps<Samples> {

    /// The actual pixel data
    pub map_data: LevelMaps<Samples>,

    /// The number of levels that were generated along the x-axis and y-axis.
    pub level_count: Vec2<usize>,
}

/// The actual pixel data, finally.
/// Contains a vector of samples.
#[derive(Clone, PartialEq, Debug)]
pub struct SampleBlock<Samples> {

    /// The dimensions of this sample collection
    pub resolution: Vec2<usize>,

    /// The actual pixel samples
    pub samples: Samples
}

/// The samples of a 2D grid, flattened in a single vector.
/// The vector contains each row, one after another.
/// A specific pixel value can be found at the index `samples[y_index * width + x_index]`.
pub type FlatSamples<Sample> = Vec<Sample>;

/// A collection of deep sample lines.
// TODO do not store line by line in a separate vector!
pub type DeepSamples<Sample> = Vec<DeepLine<Sample>>;

/// A single line of deep data.
#[derive(Clone, Debug, PartialEq)]
pub struct DeepLine<Sample> {

    /// The samples of this row of pixels.
    // TODO do not store line by line in a separate vector!
    pub samples: Vec<Sample>,

    /// For each column, this specifies the index in `samples` where to find the next sample.
    /// Therefore, `samples[index_table[column_index - 1]]` contains the start position of the sample.
    pub index_table: Vec<u32>,
}


impl Default for WriteOptions {
    fn default() -> Self { Self::fast() }
}

impl Default for ReadOptions {
    fn default() -> Self { Self::fast() }
}


impl WriteOptions {
    /*pub fn fast_writing() -> Self { // TODO rethink overrides
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
    }*/

    pub fn debug() -> Self {
        WriteOptions {
            parallel_compression: false,
            override_line_order: None,
            override_blocks: None,
            override_compression: None
        }
    }

    pub fn fast() -> Self {
        WriteOptions {
            parallel_compression: false,
            override_line_order: None,
            override_blocks: None,
            override_compression: None
        }
    }
}

impl ReadOptions {
    pub fn fast() -> Self { ReadOptions { parallel_decompression: true } }
    pub fn low() -> Self { ReadOptions { parallel_decompression: false } }
    pub fn debug() -> Self { ReadOptions { parallel_decompression: false } }
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


impl Image {

    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    #[must_use]
    pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions) -> Result<Self> {
        Self::read_from_unbuffered(std::fs::File::open(path)?, options)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read + Send, options: ReadOptions) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered), options)
    }

    /// Read the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    #[must_use]
    pub fn read_from_buffered(read: impl Read + Send, options: ReadOptions) -> Result<Self> {
        crate::image::read_all_lines_from_buffered(read, options.parallel_decompression, Image::allocate, Image::insert_line)
    }

    /// Write the exr image to a file.
    /// Use `write_to_unbuffered` instead if you do not have a file.
    #[must_use]
    pub fn write_to_file(&self, path: impl AsRef<std::path::Path>, options: WriteOptions) -> PassiveResult {
        self.write_to_unbuffered(std::fs::File::create(path)?, options)
    }

    /// Buffer the reader and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    #[must_use]
    pub fn write_to_unbuffered(&self, unbuffered: impl Write + Seek, options: WriteOptions) -> PassiveResult {
        self.write_to_buffered(BufWriter::new(unbuffered), options)
    }

    /// Write the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    #[must_use]
    pub fn write_to_buffered(&self, write: impl Write + Seek, options: WriteOptions) -> PassiveResult {
        crate::image::write_all_lines_to_buffered(
            write, options.parallel_compression, self.infer_meta_data(options),
            |location, write| {
                self.extract_line(location, write)
            }
        )
    }
}

impl Image {

    /// Allocate an image ready to be filled with pixel data.
    pub fn allocate(headers: &[Header]) -> Result<Self> {
        let display_window = headers.iter()
            .map(|header| header.display_window)
            .next().unwrap_or(IntRect::zero()); // default value if no headers are found

        let pixel_aspect = headers.iter()
            .map(|header| header.pixel_aspect)
            .next().unwrap_or(1.0); // default value if no headers are found

        let headers : Result<_> = headers.iter().map(Part::allocate).collect();

        Ok(Image {
            parts: headers?,
            display_window,
            pixel_aspect
        })
    }

    /// Insert one line of pixel data into this image.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        debug_assert_ne!(line.location.width, 0);

        let part = self.parts.get_mut(line.location.part)
            .ok_or(Error::invalid("chunk part index"))?;

        part.insert_line(line)
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) {
        debug_assert_ne!(index.width, 0);

        let part = self.parts.get(index.part)
            .expect("invalid part index");

        part.extract_line(index, write)
    }

    /// Create the meta data that describes this image.
    /// May produce invalid meta data. The meta data will be validated just before writing.
    pub fn infer_meta_data(&self, options: WriteOptions) -> MetaData {
        let headers: Headers = self.parts.iter()
            .map(|part| part.infer_header(self.display_window, self.pixel_aspect, options))
            .collect();

        let has_tiles = headers.iter().any(|header| header.blocks.has_tiles());

        MetaData {
            requirements: Requirements::new(
                self.minimum_version(), headers.len() > 1, has_tiles,
                self.has_long_names(), false // TODO deep data
            ),

            headers
        }
    }

    /// Compute the version number that this image requires to be decoded.
    /// For simple images, this should return `1`.
    ///
    /// Currently always returns `2`.
    pub fn minimum_version(&self) -> u8 {
        2 // TODO pick lowest possible
    }

    /// Check if this file has long name strings.
    ///
    /// Currently always returns `true`.
    pub fn has_long_names(&self) -> bool {
        true // TODO check all name string lengths
    }
}



impl Part {

    /// Allocate an image part ready to be filled with pixel data.
    pub fn allocate(header: &Header) -> Result<Self> {
        Ok(Part {
            data_window: header.data_window,
            screen_window_center: header.screen_window_center,
            screen_window_width: header.screen_window_width,
            name: header.name.clone(),
            attributes: header.custom_attributes.clone(),
            channels: header.channels.list.iter().map(|channel| Channel::allocate(header, channel)).collect(),
            compression: header.compression,
            blocks: header.blocks,
            line_order: header.line_order,
        })
    }

    /// Insert one line of pixel data into this image part.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        debug_assert!(line.location.position.0 + line.location.width <= self.data_window.size.0);
        debug_assert!(line.location.position.1 < self.data_window.size.1);

        self.channels.get_mut(line.location.channel)
            .expect("invalid channel index")
            .insert_line(line)
    }

    /// Read one line of pixel data from this image part.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) {
        debug_assert!(index.position.0 + index.width <= self.data_window.size.0);
        debug_assert!(index.position.1 < self.data_window.size.1);

        self.channels.get(index.channel)
            .expect("invalid channel index")
            .extract_line(index, write)
    }

    /// Create the meta data that describes this image part.
    /// May produce invalid meta data. The meta data will be validated just before writing.
    pub fn infer_header(&self, display_window: IntRect, pixel_aspect: f32, options: WriteOptions) -> Header {
        let chunk_count = compute_chunk_count(
            self.compression, self.data_window, self.blocks
        );

        Header {
            chunk_count,

            name: self.name.clone(),
            data_window: self.data_window,
            screen_window_center: self.screen_window_center,
            screen_window_width: self.screen_window_width,
            compression: options.override_compression.unwrap_or(self.compression),
            blocks: options.override_blocks.unwrap_or(self.blocks),
            channels: ChannelList::new(self.channels.iter().map(Channel::infer_channel_attribute).collect()),
            line_order: options.override_line_order.unwrap_or(self.line_order),

            // TODO deep/multipart data:
            deep_data_version: None,
            max_samples_per_pixel: None,
            custom_attributes: self.attributes.clone(),
            display_window, pixel_aspect,
            deep: false
        }
    }


}

impl Channel {

    /// Allocate a channel ready to be filled with pixel data.
    pub fn allocate(header: &Header, channel: &attributes::Channel) -> Self {
        Channel {
            name: channel.name.clone(),
            is_linear: channel.is_linear,
            sampling: channel.sampling,

            content: match channel.pixel_type {
                PixelType::F16 => ChannelData::F16(SampleMaps::allocate(header, channel)),
                PixelType::F32 => ChannelData::F32(SampleMaps::allocate(header, channel)),
                PixelType::U32 => ChannelData::U32(SampleMaps::allocate(header, channel)),
            },
        }
    }

    /// Insert one line of pixel data into this channel.
    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        match &mut self.content {
            ChannelData::F16(maps) => maps.insert_line(line),
            ChannelData::F32(maps) => maps.insert_line(line),
            ChannelData::U32(maps) => maps.insert_line(line),
        }
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, block: &mut impl Write) {
        match &self.content {
            ChannelData::F16(maps) => maps.extract_line(index, block),
            ChannelData::F32(maps) => maps.extract_line(index, block),
            ChannelData::U32(maps) => maps.extract_line(index, block),
        }
    }

    /// Create the meta data that describes this channel.
    pub fn infer_channel_attribute(&self) -> attributes::Channel {
        attributes::Channel {
            pixel_type: match self.content {
                ChannelData::F16(_) => PixelType::F16,
                ChannelData::F32(_) => PixelType::F32,
                ChannelData::U32(_) => PixelType::U32,
            },

            name: self.name.clone(),
            is_linear: self.is_linear,
            sampling: self.sampling,
        }
    }
}


impl<Sample: Data + std::fmt::Debug> SampleMaps<Sample> {

    /// Allocate a collection of resolution maps ready to be filled with pixel data.
    pub fn allocate(header: &Header, channel: &attributes::Channel) -> Self {
        if header.deep {
            SampleMaps::Deep(Levels::allocate(header, channel))
        }
        else {
            SampleMaps::Flat(Levels::allocate(header, channel))
        }
    }

    /// Insert one line of pixel data into a level.
    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        match self {
            SampleMaps::Deep(ref mut levels) => levels.insert_line(line),
            SampleMaps::Flat(ref mut levels) => levels.insert_line(line),
        }
    }

    /// Read one line of pixel data from a level.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, block: &mut impl Write) {
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

    /// Allocate a collection of resolution maps ready to be filled with pixel data.
    pub fn allocate(header: &Header, channel: &attributes::Channel) -> Self {
        let data_size = header.data_window.size / channel.sampling;

        if let Blocks::Tiles(tiles) = &header.blocks {
            let round = tiles.rounding_mode;

            match tiles.level_mode {
                LevelMode::Singular => Levels::Singular(SampleBlock::allocate(data_size)),

                LevelMode::MipMap => Levels::Mip(
                    mip_map_levels(round, data_size)
                        .map(|(_, level_size)| SampleBlock::allocate(level_size)).collect()
                ),

                // TODO put this into Levels::new(..) ?
                LevelMode::RipMap => Levels::Rip({
                    let level_count_x = compute_level_count(round, data_size.0);
                    let level_count_y = compute_level_count(round, data_size.1);
                    let maps = rip_map_levels(round, data_size)
                        .map(|(_, level_size)| SampleBlock::allocate(level_size)).collect();

                    RipMaps {
                        map_data: maps,
                        level_count: Vec2(level_count_x, level_count_y)
                    }
                })
            }
        }

        // scan line blocks never have mip maps
        else {
            Levels::Singular(SampleBlock::allocate(data_size))
        }
    }

    /// Insert one line of pixel data into a level.
    pub fn insert_line(&mut self, line: Line<'_>) -> PassiveResult {
        self.get_level_mut(line.location.level)?.insert_line(line)
    }

    /// Read one line of pixel data from a level.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) {
        self.get_level(index.level)
            .expect("invalid level index")
            .extract_line(index, write)
    }

    pub fn get_level(&self, level: Vec2<usize>) -> Result<&SampleBlock<S>> {
        match self {
            Levels::Singular(ref block) => {
                debug_assert_eq!(level, Vec2(0,0), "singular image cannot write leveled blocks");
                Ok(block)
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y");
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
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y");
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
            Levels::Mip(ref maps) => maps,
            Levels::Rip(ref rip_map) => &rip_map.map_data,
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

    /// Allocate a sample block ready to be filled with pixel data.
    pub fn allocate(resolution: Vec2<usize>) -> Self {
        SampleBlock { resolution, samples: S::allocate(resolution) }
    }

    /// Insert one line of pixel data into this sample block.
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

    /// Read one line of pixel data from this sample block.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, index: LineIndex, write: &mut impl Write) {
        debug_assert!(index.position.0 + index.width <= self.resolution.0, "x max {} of width {}", index.position.0 + index.width, self.resolution.0);
        debug_assert!(index.position.1 < self.resolution.1, "y: {}, height: {}", index.position.1, self.resolution.1);
        debug_assert_ne!(index.width, 0);

        self.samples.extract_line(index, write, self.resolution.0)
    }
}

pub trait Samples {

    /// Allocate a sample block ready to be filled with pixel data.
    fn allocate(resolution: Vec2<usize>) -> Self;

    /// Insert one line of pixel data into this sample collection.
    fn insert_line(&mut self, line: Line<'_>, image_width: usize) -> PassiveResult;

    /// Read one line of pixel data from this sample collection.
    /// Panics for an invalid index or write error.
    fn extract_line(&self, index: LineIndex, write: &mut impl Write, image_width: usize);
}

impl<Sample: crate::io::Data> Samples for DeepSamples<Sample> {
    fn allocate(resolution: Vec2<usize>) -> Self {
        vec![
            DeepLine { samples: Vec::new(), index_table: vec![0; resolution.0] };
            resolution.1
        ]
    }

    fn insert_line(&mut self, _line: Line<'_>, _width: usize) -> PassiveResult {
//        debug_assert_ne!(image_width, 0);
//        debug_assert_ne!(length, 0);

        unimplemented!("deep data not supported yet");

        // TODO err on invalid tile position
//        self[_position.1 as usize] = DeepLine {
//            samples: Sample::read_vec(read, length, 1024*1024*1024)?, // FIXME where tiles, will not be hole line
//            index_table:
//        };
//
//        Ok(())
    }

    fn extract_line(&self, index: LineIndex, _write: &mut impl Write, image_width: usize) {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(index.width, 0);
        unimplemented!("deep data not supported yet");
    }
}

impl<Sample: crate::io::Data + Default + Clone + std::fmt::Debug> Samples for FlatSamples<Sample> {
    fn allocate(resolution: Vec2<usize>) -> Self {
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

    fn extract_line(&self, index: LineIndex, write: &mut impl Write, image_width: usize) {
        debug_assert_ne!(image_width, 0);
        debug_assert_ne!(index.width, 0);

        let start_index = index.position.1 * image_width + index.position.0;
        let end_index = start_index + index.width;

        LineIndex::write_samples(&self[start_index .. end_index], write)
            .expect("writing line bytes failed");
    }
}

impl<Samples> RipMaps<Samples> {

    /// Flatten the 2D level index to a one dimensional index.
    pub fn get_level_index(&self, level: Vec2<usize>) -> usize {
        self.level_count.0 * level.1 + level.0
    }

    /// Return a level by level index. Level `0` has the largest resolution.
    pub fn get_by_level(&self, level: Vec2<usize>) -> Option<&SampleBlock<Samples>> {
        self.map_data.get(self.get_level_index(level))
    }

    /// Return a mutable level reference by level index. Level `0` has the largest resolution.
    pub fn get_by_level_mut(&mut self, level: Vec2<usize>) -> Option<&mut SampleBlock<Samples>> {
        let index = self.get_level_index(level);
        self.map_data.get_mut(index)
    }
}
