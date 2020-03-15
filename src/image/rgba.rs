
//! Read and write a truly minimal RGBA image.
//! This module loads only images with RGBA channels if they all have the same data type (either f16, f32, or u32).
//! Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
//!
//! This module should rather be seen as an example
//! and only be used if you are confident that your images are really RGBA.
//! Use `exr::image::simple` if you need custom channels or specialized error handling.


use std::path::Path;
use std::fs::File;
use std::io::{Read, Seek, BufReader, Write, BufWriter};
use crate::math::{Vec2, RoundingMode};
use crate::error::{Result, Error, UnitResult};
use crate::meta::attributes::{PixelType, Channel, Text, LineOrder, TileDescription, LevelMode};
use std::convert::TryInto;
use crate::meta::{Header, ImageAttributes, LayerAttributes, MetaData, Blocks};
use half::f16;
use crate::image::{ReadOptions, OnReadProgress, WriteOptions, OnWriteProgress};
use crate::compression::Compression;


/// A simple RGBA with one 32-bit float per pixel for each channel.
/// Stores all samples in a flattened vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {

    /// A typical flattened RGBA sample array.
    /// If `has_alpha_channel` is false, this contains only RGB values.
    ///
    /// Stores in order red, green, blue, then alpha components.
    /// All lines of the image are appended one after another, __bottom to top__.
    ///
    /// To calculate an index, you can use `Image::vector_index_of_first_pixel_component(Vec2<usize>) -> usize`,
    /// which returns the corresponding one-dimensional index of a pixel in this array.
    // TODO make this an interface for custom data storage.
    pub data: Pixels,

    /// The dimensions of this Image, width times height.
    pub resolution: Vec2<usize>,

    /// Specifies if the `data` vector contains 3 or 4 values per pixel.
    pub has_alpha_channel: bool,

    /// Specifies if this image is in a linear color space.
    pub is_linear: bool,

    /// The attributes of the exr image.
    pub image_attributes: ImageAttributes,

    /// The attributes of the exr layer.
    pub layer_attributes: LayerAttributes,

    /// Specifies how the pixel data is formatted inside the file,
    /// for example, compression and tiling.
    pub encoding: Encoding,
}

/// Specifies how the pixel data is formatted inside the file.
/// Does not affect any visual aspect, like positioning or orientation.
// TODO alsop nest encoding like this for meta::Header and simple::Image or even reuse this in image::simple
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Encoding {

    /// What type of compression the pixel data in the file is compressed with.
    pub compression: Compression,

    /// If this is some pair of numbers, the image is divided into tiles of that size.
    /// If this is none, the image is divided into scan line blocks, depending on the compression method.
    pub tile_size: Option<Vec2<usize>>,

    /// In what order the tiles of this header occur in the file.
    /// Does not change any actual image orientation.
    pub line_order: LineOrder,
}

/// A one dimensional array of samples.
///
/// Stores in order red, green, blue, then alpha components.
/// All lines of the image are appended one after another, top to bottom.
#[derive(Clone, PartialEq)]
pub enum Pixels {

    /// 16-bit floating point number samples in a flattened array.
    F16(Vec<f16>),

    /// 32-bit floating point number samples in a flattened array.
    F32(Vec<f32>),

    /// 32-bit unsigned integer samples in a flattened array.
    U32(Vec<u32>),
}


impl Encoding {

    /// Chooses an optimal tile size and line order for the specified compression.
    #[inline]
    pub fn compress(compression: Compression) -> Self {
        match compression {
            Compression::Uncompressed => Self {
                tile_size: None, // scan lines have maximum width, which is best for efficient line memcpy
                line_order: LineOrder::Increasing, // order does not really matter, as no compression is parrallelized
                compression,
            },

            Compression::RLE => Self {
                tile_size: None, // scan lines have maximum width, which is best for long RLE runs
                line_order: LineOrder::Increasing, // cannot be unspecified with scan line blocks??
                compression,
            },

            Compression::ZIP16 | Compression::ZIP1 => Self {
                tile_size: None, // maximum data size for zip compression
                line_order: LineOrder::Increasing, // cannot be unspecified with scan line blocks??
                compression,
            },

            _ => Self {
                compression,
                tile_size: None,
                line_order: LineOrder::Increasing // scan line blocks cannot have unspecified order??
            }
        }
    }

    /// Uses RLE compression with scan line blocks.
    #[inline]
    pub fn fast() -> Self {
        Self::compress(Compression::RLE)
    }

    /// Uses ZIP16 compression with scan line blocks.
    #[inline]
    pub fn small() -> Self {
        Self::compress(Compression::ZIP16)
    }
}


impl Image {

    /// Create an image with the resolution, alpha channel, linearity, and actual pixel data.
    pub fn new(resolution: Vec2<usize>, has_alpha_channel: bool, is_linear: bool, data: Pixels) -> Self {
        let result = Self {
            data, resolution, has_alpha_channel, is_linear,
            image_attributes: ImageAttributes::new(resolution),
            layer_attributes: LayerAttributes::new(Text::from("RGBA").expect("ascii bug")),
            encoding: Encoding::fast(),
        };

        let data_len = result.channel_count() * resolution.area();
        debug_assert_eq!(data_len, result.data.len(), "pixel data length must be {} but was {}", data_len, result.data.len());

        result
    }

    /// Set the display window and data window position of this image.
    pub fn with_position(mut self, position: Vec2<i32>) -> Self {
        self.image_attributes.display_window.position = position;
        self.layer_attributes.data_position = position;
        self
    }

    /// Set custom attributes for the exr image.
    #[inline]
    pub fn with_image_attributes(self, image_attributes: ImageAttributes) -> Self {
        Self { image_attributes, ..self }
    }

    /// Set custom attributes for the layer in the exr image.
    #[inline]
    pub fn with_layer_attributes(self, layer_attributes: LayerAttributes) -> Self {
        Self { layer_attributes, ..self }
    }

    /// Specify how this image should be formatted in the file. Does not affect visual content.
    #[inline]
    pub fn with_encoding(self, encoding: Encoding) -> Self {
        Self { encoding, ..self }
    }

    /// Is 4 if this is an RGBA image. Is 3 if this is an RGB image.
    #[inline]
    pub fn channel_count(&self) -> usize {
        if self.has_alpha_channel { 4 } else { 3 }
    }

    /// Computes the one-dimensional index from a two-dimensional pixel location.
    /// The index points to the red component of the pixel.
    /// The green and blue pixel values can be found directly after it.
    #[inline]
    pub fn calculate_vector_index_of_first_pixel_component(resolution: Vec2<usize>, channel_count: usize, pixel: Vec2<usize>) -> usize {
        debug_assert!(pixel.0 < resolution.0 && pixel.1 < resolution.1, "coordinate out of range");
        (pixel.1 * resolution.0 + pixel.0) * channel_count
    }

    /// Computes the one-dimensional index from a two-dimensional pixel location.
    /// The index points to the red component of the pixel.
    /// The green and blue pixel values can be found directly after it.
    /// Does not consider data window offset or display window offset.
    /// Also see `Image::calculate_vector_index_of_first_pixel_component`.
    #[inline]
    pub fn vector_index_of_first_pixel_component(&self, pixel: Vec2<usize>) -> usize {
        Self::calculate_vector_index_of_first_pixel_component(self.resolution, self.channel_count(), pixel)
    }

    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    // TODO add read option: skip alpha channel even if present.
    #[inline]
    #[must_use]
    pub fn read_from_file(path: impl AsRef<Path>, options: ReadOptions<impl OnReadProgress>) -> Result<Self> {
        Self::read_from_unbuffered(File::open(path)?, options)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    ///
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[inline]
    #[must_use]
    pub fn read_from_unbuffered(read: impl Read + Seek + Send, options: ReadOptions<impl OnReadProgress>) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(read), options)
    }

    /// Read the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    ///
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[inline]
    #[must_use]
    pub fn read_from_buffered(read: impl Read + Seek + Send, options: ReadOptions<impl OnReadProgress>) -> Result<Self> {
        crate::image::read_filtered_lines_from_buffered(
            read,

            Self::extract,

            // only keep the one header we selected earlier
            |image, header, tile| {
                tile.location.is_largest_resolution_level() // also skip multiresolution shenanigans
                    && header.own_attributes.name == image.layer_attributes.name // header names were checked to be unique earlier
            },

            |image, meta, line| {
                debug_assert_eq!(meta[line.location.layer].own_attributes.name, image.layer_attributes.name, "irrelevant header should be filtered out"); // TODO this should be an error right?

                let channel_count = image.channel_count();
                let channel_index = channel_count - 1 - line.location.channel; // convert ABGR index to RGBA index
                let line_position = line.location.position;
                let Vec2(width, height) = image.resolution;

                println!("channel_index: {}", channel_index);
                println!("channel_count: {}", channel_count);
                println!("self res: {:?}", image.resolution);
                println!("line index: {:?}", line);

                let get_index_of_sample = move |sample_index| {
                    let location = line_position + Vec2(sample_index, 0);
                    debug_assert!(location.0 < width && location.1 < height, "coordinate out of range: {:?}", location);

                    let flat = location.1 * width + location.0;
                    let r_index = flat * channel_count;
                    r_index + channel_index
                };

                match &mut image.data {
                    Pixels::F16(vec) => for (sample_index, sample) in line.read_samples().enumerate() { // TODO any pixel_type?
                        *vec.get_mut(get_index_of_sample(sample_index)).expect("rgba sample index calculation bug") = sample?;
                    },

                    Pixels::F32(vec) => for (sample_index, sample) in line.read_samples().enumerate() { // TODO any pixel_type?
                        *vec.get_mut(get_index_of_sample(sample_index)).expect("rgba sample index calculation bug") = sample?;
                    },

                    Pixels::U32(vec) => for (sample_index, sample) in line.read_samples().enumerate() { // TODO any pixel_type?
                        *vec.get_mut(get_index_of_sample(sample_index)).expect("rgba sample index calculation bug") = sample?;
                    },
                };

                Ok(())
            },

            options
        )
    }


    /// Allocate the memory for an image that could contain the described data.
    fn allocate(header: &Header, linear: bool, alpha: bool, pixel_type: PixelType) -> Self {
        let components = if alpha { 4 } else { 3 };
        let samples = components * header.data_size.area();

        Self {
            resolution: header.data_size,
            has_alpha_channel: alpha,

            data: match pixel_type {
                PixelType::F16 => Pixels::F16(vec![f16::from_f32(0.0); samples]),
                PixelType::F32 => Pixels::F32(vec![0.0; samples]),
                PixelType::U32 => Pixels::U32(vec![0; samples]),
            },

            is_linear: linear,

            layer_attributes: header.own_attributes.clone(),
            image_attributes: header.shared_attributes.clone(),

            encoding: Encoding {
                compression: header.compression,
                line_order: header.line_order,
                tile_size: match header.blocks {
                    Blocks::Tiles(tiles) => Some(tiles.tile_size),
                    Blocks::ScanLines => None,
                },
            }
        }
    }

    /// Try to find a header matching the RGBA requirements.
    fn extract(headers: &[Header]) -> Result<Self> {
        println!("allocating for meta data {:#?}", headers);

        let first_header_name = headers.first()
            .and_then(|header| header.own_attributes.name.as_ref());

        for (header_index, header) in headers.iter().enumerate() {
            // the following check is required because filtering works by name in this RGBA implementation
            if header_index != 0 && header.own_attributes.name.as_ref() == first_header_name {
                return Err(Error::invalid("duplicate header name"))
            }

            let channels = &header.channels.list;

            // channels are always sorted alphabetically
            let is_rgba = channels.len() == 4
                && channels[0].name == "A".try_into().unwrap() // TODO case insensitivity
                && channels[1].name == "B".try_into().unwrap()
                && channels[2].name == "G".try_into().unwrap()
                && channels[3].name == "R".try_into().unwrap();

            // channels are always sorted alphabetically
            let is_rgb = channels.len() == 3
                && channels[0].name == "B".try_into().unwrap()
                && channels[1].name == "G".try_into().unwrap()
                && channels[2].name == "R".try_into().unwrap();

            if !is_rgba && !is_rgb { continue; }

            let first_channel: &Channel = &channels[0];
            let pixel_type_mismatch = channels[1..].iter()
                .any(|channel|
                    channel.pixel_type != first_channel.pixel_type
                        && channel.is_linear == first_channel.is_linear
                );

            if pixel_type_mismatch { continue; }

            return Ok(Self::allocate(header, first_channel.is_linear, is_rgba, first_channel.pixel_type))
        }

        Err(Error::invalid("no valid RGB or RGBA image part"))
    }

    /// Write the exr image to a file.
    /// Use `write_to_unbuffered` instead if you do not have a file.
    /// If an error occurs, attempts to delete the partially written file.
    #[must_use]
    pub fn write_to_file(&self, path: impl AsRef<Path>, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        crate::io::attempt_delete_file_on_write_error(path, |write|
            self.write_to_unbuffered(write, options)
        )
    }

    /// Buffer the writer and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    #[must_use]
    pub fn write_to_unbuffered(&self, write: impl Write + Seek, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        self.write_to_buffered(BufWriter::new(write), options)
    }

    /// Write the exr image to a writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    #[must_use]
    pub fn write_to_buffered(&self, write: impl Write + Seek, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        let pixel_type = match self.data {
            Pixels::F16(_) => PixelType::F16,
            Pixels::F32(_) => PixelType::F32,
            Pixels::U32(_) => PixelType::U32,
        };

        let header = Header::new(
            self.layer_attributes.name.clone().unwrap_or(Text::from("RGBA").unwrap()),
            self.resolution,
    if self.has_alpha_channel { smallvec![
                Channel::new("A".try_into().unwrap(), pixel_type, self.is_linear), // TODO make linear a parameter
                Channel::new("B".try_into().unwrap(), pixel_type, self.is_linear),
                Channel::new("G".try_into().unwrap(), pixel_type, self.is_linear),
                Channel::new("R".try_into().unwrap(), pixel_type, self.is_linear),
            ] }

            else { smallvec![
                Channel::new("B".try_into().unwrap(), pixel_type, self.is_linear),
                Channel::new("G".try_into().unwrap(), pixel_type, self.is_linear),
                Channel::new("R".try_into().unwrap(), pixel_type, self.is_linear),
            ] }
        );

        let header = header
            .with_shared_attributes(self.image_attributes.clone())
            .with_attributes(self.layer_attributes.clone())
            .with_encoding(
                self.encoding.compression,

                match self.encoding.tile_size {
                    None => Blocks::ScanLines,
                    Some(size) => Blocks::Tiles(TileDescription {
                        tile_size: size,
                        level_mode: LevelMode::Singular,
                        rounding_mode: RoundingMode::Down
                    })
                },

                self.encoding.line_order,
            );

        crate::image::write_all_lines_to_buffered(
            write,
            MetaData::new(smallvec![ header ]),

            |_meta, line| {
                let channel_count = self.channel_count();
                let channel_index = channel_count - 1 - line.location.channel; // convert ABGR index to RGBA index
                let line_position = line.location.position;
                let Vec2(width, height) = self.resolution;
                debug_assert!(line.location.channel < self.channel_count(), "channel count bug");

                let get_index_of_sample = move |sample_index| {
                    let location = line_position + Vec2(sample_index, 0);
                    debug_assert!(location.0 < width && location.1 < height, "coordinate out of range: {:?}", location);

                    let flat = location.1 * width + location.0;
                    let r_index = flat * channel_count;
                    r_index + channel_index
                };

                match &self.data {
                    Pixels::F16(vec) => line.write_samples(|sample_index|{
                        vec[get_index_of_sample(sample_index)]
                    })?,

                    Pixels::F32(vec) => line.write_samples(|sample_index|{
                        vec[get_index_of_sample(sample_index)]
                    })?,

                    Pixels::U32(vec) => line.write_samples(|sample_index|{
                        vec[get_index_of_sample(sample_index)]
                    })?,
                };

                Ok(())
            },

            options
        )
    }
}

impl Pixels {

    /// The number of samples, that is, the number of all r, g, b, and a samples in the image, summed.
    /// For example, an RGBA image with 6x5 pixels has `6 * 5 * 4 = 120` samples.
    pub fn len(&self) -> usize {
        match self {
            Pixels::F16(vec) => vec.len(),
            Pixels::F32(vec) => vec.len(),
            Pixels::U32(vec) => vec.len(),
        }
    }
}


// Do not print the actual pixel contents into the console.
impl std::fmt::Debug for Pixels {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pixels::F16(ref vec) => write!(formatter, "[F16; {}]", vec.len()),
            Pixels::F32(ref vec) => write!(formatter, "[F32; {}]", vec.len()),
            Pixels::U32(ref vec) => write!(formatter, "[U32; {}]", vec.len()),
        }
    }
}