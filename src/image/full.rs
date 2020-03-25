
//! Read and write all supported aspects of an exr image, including deep data and multi-resolution levels.
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
use crate::error::{Result, UnitResult, Error};
use crate::math::*;
use std::io::{Seek, BufReader, BufWriter};
use crate::io::Data;
use crate::image::{LineRefMut, LineRef, OnWriteProgress, OnReadProgress, ReadOptions, WriteOptions};

// FIXME this needs some of the changes that were made in simple.rs !!!


/// An exr image.
///
/// Supports all possible exr image features.
/// An exr image may contain multiple layers.
/// All meta data is encoded in this image,
/// including custom attributes.
#[derive(Clone, PartialEq, Debug)]
pub struct Image {

    /// All layers contained in the image file
    pub layers: Layers,

    /// Attributes that apply to the whole image file.
    /// Excludes technical meta data.
    pub attributes: ImageAttributes,
}

pub type Layers = SmallVec<[Layer; 3]>;

/// A single layer of an exr image.
/// Contains meta data and actual pixel information of the channels.
#[derive(Clone, PartialEq, Debug)]
pub struct Layer {

    /// List of channels in this layer.
    /// Contains the actual pixel data of the image.
    pub channels: Channels,

    /// Attributes that apply to this layer. Excludes technical meta data.
    /// May still contain attributes that should be considered global for an image file.
    pub attributes: LayerAttributes,

    /// The rectangle that positions this layer
    /// within the global infinite 2D space of the file.
    pub data_size: Vec2<usize>,

    /// In what order the tiles of this header occur in the file.
    pub line_order: LineOrder,

    /// How the pixel data of all channels in this layer is compressed. May be `Compression::Uncompressed`.
    pub compression: Compression,

    /// Describes how the pixels of this layer are divided into smaller blocks in the file.
    /// A single block can be loaded without processing all bytes of a file.
    ///
    /// Also describes whether a file contains multiple resolution levels: mip maps or rip maps.
    /// This allows loading not the full resolution, but the smallest sensible resolution.
    pub blocks: Blocks,
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

    /// How many of the samples are skipped compared to the other channels in this layer.
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
    /// Used for segmentation of layers.
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

/// The different resolution levels of the layer.
// FIXME should be descending and starting with full-res instead!
#[derive(Clone, PartialEq)]
pub enum Levels<Samples> {

    /// Just the full resolution image.
    Singular(SampleBlock<Samples>),

    /// In addition to the full resolution image,
    /// this layer also contains smaller versions with the same aspect ratio.
    Mip(LevelMaps<Samples>),

    /// In addition to the full resolution image,
    /// this layer also contains smaller versions,
    /// and each smaller version has further versions with varying aspect ratios.
    Rip(RipMaps<Samples>),
}

pub type LevelMaps<Samples> = Vec<SampleBlock<Samples>>;

/// In addition to the full resolution image,
/// this layer also contains smaller versions,
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
    pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions<impl OnReadProgress>) -> Result<Self> {
        Self::read_from_unbuffered(std::fs::File::open(path)?, options)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    #[inline]
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read + Send, options: ReadOptions<impl OnReadProgress>) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered), options)
    }

    /// Read the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    #[inline]
    #[must_use]
    pub fn read_from_buffered(read: impl Read + Send, options: ReadOptions<impl OnReadProgress>) -> Result<Self> {
        crate::image::read_all_lines_from_buffered(
            read,
            Image::allocate,
            |image, _meta, line| Image::insert_line(image, line),
            options
        )
    }

    /// Write the exr image to a file.
    /// Use `write_to_unbuffered` instead if you do not have a file.
    /// If an error occurs, attempts to delete the partially written file.
    #[inline]
    #[must_use]
    pub fn write_to_file(&self, path: impl AsRef<std::path::Path>, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        crate::io::attempt_delete_file_on_write_error(path, move |write|
            self.write_to_unbuffered(write, options)
        )
    }

    /// Buffer the writer and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    #[inline]
    #[must_use]
    pub fn write_to_unbuffered(&self, unbuffered: impl Write + Seek, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        self.write_to_buffered(BufWriter::new(unbuffered), options)
    }

    /// Write the exr image to a writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    #[inline]
    #[must_use]
    pub fn write_to_buffered(&self, write: impl Write + Seek, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        crate::image::write_all_lines_to_buffered(
            write,  self.infer_meta_data(),
            |_meta, line_mut| self.extract_line(line_mut),
            options
        )
    }
}

impl Image {

    /// Allocate an image ready to be filled with pixel data.
    pub fn allocate(headers: &[Header]) -> Result<Self> {
        let shared_attributes = &headers.iter()
            // pick the header with the most attributes
            // (all headers should have the same shared attributes anyways)
            .max_by_key(|header| header.shared_attributes.custom.len())
            .expect("at least one header is required").shared_attributes;

        let headers : Result<_> = headers.iter().map(Layer::allocate).collect();

        Ok(Image {
            layers: headers?,
            attributes: shared_attributes.clone(),
        })
    }

    /// Insert one line of pixel data into this image.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        debug_assert_ne!(line.location.sample_count, 0, "linde index bug");

        let layer = self.layers.get_mut(line.location.layer)
            .ok_or(Error::invalid("chunk layer index"))?;

        layer.insert_line(line)
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        debug_assert_ne!(line.location.sample_count, 0, "line index bug");

        let layer = self.layers.get(line.location.layer)
            .expect("invalid layer index");

        layer.extract_line(line)
    }

    /// Create the meta data that describes this image.
    /// May produce invalid meta data. The meta data will be validated just before writing.
    pub fn infer_meta_data(&self) -> MetaData {
        let headers: Headers = self.layers.iter()
            .map(|layer| layer.infer_header(&self.attributes))
            .collect();

        MetaData::new(headers)
    }
}



impl Layer {

    /// Allocate an layer ready to be filled with pixel data.
    pub fn allocate(header: &Header) -> Result<Self> {
        Ok(Layer {
            data_size: header.data_size,
            attributes: header.own_attributes.clone(),
            channels: header.channels.list.iter().map(|channel| Channel::allocate(header, channel)).collect(),
            compression: header.compression,
            blocks: header.blocks,
            line_order: header.line_order,
        })
    }

    /// Insert one line of pixel data into this layer.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        debug_assert!(line.location.position.0 + line.location.sample_count <= self.data_size.0, "line index bug");
        debug_assert!(line.location.position.1 < self.data_size.1, "line index bug");

        self.channels.get_mut(line.location.channel)
            .expect("invalid channel index")
            .insert_line(line)
    }

    /// Read one line of pixel data from this layer.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        debug_assert!(line.location.position.0 + line.location.sample_count <= self.data_size.0, "line index bug");
        debug_assert!(line.location.position.1 < self.data_size.1, "line index bug");

        self.channels.get(line.location.channel)
            .expect("invalid channel index")
            .extract_line(line)
    }

    /// Create the meta data that describes this layer.
    /// May produce invalid meta data. The meta data will be validated just before writing.
    pub fn infer_header(&self, shared_attributes: &ImageAttributes) -> Header {
        let chunk_count = compute_chunk_count(
            self.compression, self.data_size, self.blocks
        );

        Header {
            chunk_count,

            compression: self.compression,
            blocks: self.blocks,
            channels: ChannelList::new(self.channels.iter().map(Channel::infer_channel_attribute).collect()),
            line_order: self.line_order,

            data_size: self.data_size,
            own_attributes: self.attributes.clone(),
            shared_attributes: shared_attributes.clone(),

            // TODO deep data:
            deep_data_version: None,
            max_samples_per_pixel: None,
            deep: false,
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

            content: match channel.sample_type {
                SampleType::F16 => ChannelData::F16(SampleMaps::allocate(header, channel)),
                SampleType::F32 => ChannelData::F32(SampleMaps::allocate(header, channel)),
                SampleType::U32 => ChannelData::U32(SampleMaps::allocate(header, channel)),
            },
        }
    }

    /// Insert one line of pixel data into this channel.
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        match &mut self.content {
            ChannelData::F16(maps) => maps.insert_line(line),
            ChannelData::F32(maps) => maps.insert_line(line),
            ChannelData::U32(maps) => maps.insert_line(line),
        }
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        match &self.content {
            ChannelData::F16(maps) => maps.extract_line(line),
            ChannelData::F32(maps) => maps.extract_line(line),
            ChannelData::U32(maps) => maps.extract_line(line),
        }
    }

    /// Create the meta data that describes this channel.
    pub fn infer_channel_attribute(&self) -> attributes::Channel {
        attributes::Channel {
            sample_type: match self.content {
                ChannelData::F16(_) => SampleType::F16,
                ChannelData::F32(_) => SampleType::F32,
                ChannelData::U32(_) => SampleType::U32,
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
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        match self {
            SampleMaps::Deep(ref mut levels) => levels.insert_line(line),
            SampleMaps::Flat(ref mut levels) => levels.insert_line(line),
        }
    }

    /// Read one line of pixel data from a level.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        match self {
            SampleMaps::Deep(ref levels) => levels.extract_line(line),
            SampleMaps::Flat(ref levels) => levels.extract_line(line),
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
        let data_size = header.data_size / channel.sampling;

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
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        self.get_level_mut(line.location.level)?.insert_line(line)
    }

    /// Read one line of pixel data from a level.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        self.get_level(line.location.level)
            .expect("invalid level index")
            .extract_line(line)
    }

    pub fn get_level(&self, level: Vec2<usize>) -> Result<&SampleBlock<S>> {
        match self {
            Levels::Singular(ref block) => {
                debug_assert_eq!(level, Vec2(0,0), "singular image cannot write leveled blocks bug");
                Ok(block)
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y bug");
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
                debug_assert_eq!(level, Vec2(0,0), "singular image cannot write leveled blocks bug");
                Ok(block)
            },

            Levels::Mip(block) => {
                debug_assert_eq!(level.0, level.1, "mip map levels must be equal on x and y bug");
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
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        debug_assert_ne!(line.location.sample_count, 0, "line index bug");

        if line.location.position.0 + line.location.sample_count > self.resolution.0 {
            return Err(Error::invalid("data block x coordinate"))
        }

        if line.location.position.1 > self.resolution.1 {
            return Err(Error::invalid("data block y coordinate"))
        }

        self.samples.insert_line(line, self.resolution.0)
    }

    /// Read one line of pixel data from this sample block.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        debug_assert!(line.location.position.0 + line.location.sample_count <= self.resolution.0, "line index bug");
        debug_assert!(line.location.position.1 < self.resolution.1, "line index bug");
        debug_assert_ne!(line.location.sample_count, 0, "line index bug");

        self.samples.extract_line(line, self.resolution.0)
    }
}

pub trait Samples {

    /// Allocate a sample block ready to be filled with pixel data.
    fn allocate(resolution: Vec2<usize>) -> Self;

    /// Insert one line of pixel data into this sample collection.
    fn insert_line(&mut self, line: LineRef<'_>, image_width: usize) -> UnitResult;

    /// Read one line of pixel data from this sample collection.
    /// Panics for an invalid index or write error.
    fn extract_line(&self, line: LineRefMut<'_>, image_width: usize);
}

impl<Sample: crate::io::Data> Samples for DeepSamples<Sample> {
    fn allocate(resolution: Vec2<usize>) -> Self {
        debug_assert!(resolution.area() < 1920*10 * 1920*10, "suspiciously large image");

        vec![
            DeepLine { samples: Vec::new(), index_table: vec![0; resolution.0] };
            resolution.1
        ]
    }

    fn insert_line(&mut self, _line: LineRef<'_>, _image_width: usize) -> UnitResult {
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

    fn extract_line(&self, _line: LineRefMut<'_>, _image_width: usize) {
        debug_assert_ne!(_image_width, 0, "deep image width bug");
        unimplemented!("deep data not supported yet");
    }
}

impl<Sample: crate::io::Data + Default + Clone + std::fmt::Debug> Samples for FlatSamples<Sample> {
    fn allocate(resolution: Vec2<usize>) -> Self {
        let count = resolution.area();
        debug_assert!(count < 1920*20 * 1920*20, "suspiciously large image: {} mega pixels", count / 1_000_000);

        vec![Sample::default(); count]
    }

    fn insert_line(&mut self, line: LineRef<'_>, image_width: usize) -> UnitResult {
        debug_assert_ne!(image_width, 0, "image width calculation bug");
        debug_assert_ne!(line.location.sample_count, 0, "line width calculation bug");

        let start_index = line.location.position.1 * image_width + line.location.position.0;
        let end_index = start_index + line.location.sample_count;

        line.read_samples_into_slice(&mut self[start_index .. end_index])
    }

    fn extract_line(&self, line: LineRefMut<'_>, image_width: usize) {
        debug_assert_ne!(image_width, 0, "image width calculation bug");

        let start_index = line.location.position.1 * image_width + line.location.position.0;
        let end_index = start_index + line.location.sample_count;

        line.write_samples_from_slice(&self[start_index .. end_index])
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
