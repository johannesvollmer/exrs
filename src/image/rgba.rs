
//! Read and write a simple RGBA image.
//! This module loads the RGBA channels of any layer that contains RGB or RGBA channels.
//! Returns `Error::Invalid` if none can be found in the file.
//!
//! This module should only be used if you are confident that your images are really RGBA.
//! Use `exr::image::simple` if you need custom channels or specialized error handling.


use std::path::Path;
use std::fs::File;
use std::io::{Read, Seek, BufReader, Write, BufWriter};
use crate::math::{Vec2, RoundingMode};
use crate::error::{Result, Error, UnitResult};
use crate::meta::attributes::{SampleType, Text, LineOrder, TileDescription, LevelMode};
use std::convert::TryInto;
use crate::meta::{Header, ImageAttributes, LayerAttributes, MetaData, Blocks};
use half::f16;
use crate::image::{ReadOptions, OnReadProgress, WriteOptions, OnWriteProgress};
use crate::compression::Compression;


/// An image with a custom pixel storage.
/// Use `Image::read_from_file` to actually load an image.
///
/// See the `exr::image::rgba::pixels` module
/// if you do not want to implement your own pixel storage.
#[derive(Debug, Clone, PartialEq)]
pub struct Image<Storage> {

    /// The user-specified pixel storage containing the actual pixel data.
    /// This is a type parameter which should implement either `ExposePixels` or `ConsumePixels`.
    pub data: Storage,

    /// The channel types of the written file.
    /// For each channel, the appropriate method is called on `Image.data`.
    ///
    /// Careful: Not all applications may support
    /// RGBA images with arbitrary sample types.
    pub channels: Channels,

    /// The dimensions of this image, width and height.
    pub resolution: Vec2<usize>,

    /// The attributes of the exr image.
    pub image_attributes: ImageAttributes,

    /// The attributes of the exr layer.
    pub layer_attributes: LayerAttributes,

    /// Specifies how the pixel data is formatted inside the file,
    /// for example, compression and tiling.
    pub encoding: Encoding,
}

/// The RGBA channels of an image. The alpha channel is optional.
/// The first channel is red, the second blue, the third green, and the fourth alpha.
pub type Channels = (Channel, Channel, Channel, Option<Channel>);

/// Describes a single channel of red, green, blue, or alpha samples.
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub struct Channel {

    /// Are the samples stored in a linear color space?
    is_linear: bool,

    /// The type of the samples in this channel.
    sample_type: SampleType,
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


/// Expose the pixels of an image. Implement this on your own image type to write your image to a file.
///
/// Contains a separate method for each of the three possible sample types.
/// The actual sample type of the file is specified within `Image.channels`.
/// Implementing only the `f32` method will automatically convert all samples to that type, if necessary.
pub trait GetPixels: Sized + Sync { // TODO does not actually always need sync

    /// Extract a single `f32` value out of your image. Should panic for invalid sample indices.
    fn get_sample_f32(image: &Image<Self>, index: SampleIndex) -> f32;

    /// Extract a single `u32` value out of your image. Should panic for invalid sample indices.
    #[inline] fn get_sample_u32(image: &Image<Self>, index: SampleIndex) -> u32 { Self::get_sample_f32(image, index) as u32 }

    /// Extract a single `f16` value out of your image. Should panic for invalid sample indices.
    #[inline] fn get_sample_f16(image: &Image<Self>, index: SampleIndex) -> f16 { f16::from_f32(Self::get_sample_f32(image, index)) }
}

/// Consume the pixels of an image file. Implement this on your own image type to read a file into your image.
///
/// Contains a separate method for each of the three possible sample types.
/// Implementing only the `f32` method will automatically convert all samples to that type, if necessary.
pub trait CreatePixels: Sized {

    /// Create a new pixel storage for the supplied image.
    /// The returned value will be put into the `data` field of the supplied image.
    fn new(image: &Image<()>) -> Self;

    /// Set the value of a single `f32`. Should panic on invalid sample indices.
    fn set_sample_f32(image: &mut Image<Self>, index: SampleIndex, sample: f32);

    /// Set the value of a single `u32`. Should panic on invalid sample indices.
    #[inline] fn set_sample_u32(image: &mut Image<Self>, index: SampleIndex, sample: u32) { Self::set_sample_f32(image, index, sample as f32) }

    /// Set the value of a single `f16`. Should panic on invalid sample indices.
    #[inline] fn set_sample_f16(image: &mut Image<Self>, index: SampleIndex, sample: f16) { Self::set_sample_f32(image, index, sample.to_f32()) }
}

/// An index that uniquely identifies each `f16`, `f32`, or `u32` in an RGBA image.
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash)]
pub struct SampleIndex {

    /// The x and y index of the pixel.
    pub position: Vec2<usize>,

    /// The index of the channel.
    /// Red is zero, green is one, blue is two, and alpha is three.
    pub channel: usize,
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


impl<S> Image<S> {

    /// Create an image with the resolution, channels, and actual pixel data.
    pub fn new(resolution: Vec2<usize>, channels: Channels, data: S) -> Self {
        Self {
            data, resolution, channels,
            image_attributes: ImageAttributes::new(resolution),
            layer_attributes: LayerAttributes::new(Text::from("RGBA").expect("ascii bug")),
            encoding: Encoding::fast()
        }
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

    /// Is 4 if this is an RGBA image, 3 for an RGB image.
    #[inline]
    pub fn channel_count(&self) -> usize {
        if self.channels.3.is_some() { 4 } else { 3 }
    }

    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    // TODO add read option: skip alpha channel even if present.
    ///
    /// Example:
    /// ```
    /// use exr::prelude::*;
    /// let image = rgba::Image::<rgba::pixels::Flattened<f16>>::read_from_file("file.exr", read_options::high());
    /// ```
    ///
    /// You should rather implement `rgba::ConsumePixels` on your own image type
    /// instead of using `pixels::Flattened<f16>`.
    #[inline]
    #[must_use]
    pub fn read_from_file(
        path: impl AsRef<Path>,
        options: ReadOptions<impl OnReadProgress>
    ) -> Result<Self> where S: CreatePixels
    {
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
    pub fn read_from_unbuffered(
        read: impl Read + Seek + Send,
        options: ReadOptions<impl OnReadProgress>
    ) -> Result<Self> where S: CreatePixels
    {
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
    pub fn read_from_buffered(
        read: impl Read + Seek + Send,
        options: ReadOptions<impl OnReadProgress>
    ) -> Result<Self> where S: CreatePixels
    {
        crate::image::read_filtered_lines_from_buffered(
            read,

            Self::extract,

            // only keep the one header we selected earlier
            |image, header, tile| {
                tile.location.is_largest_resolution_level() // also skip multiresolution shenanigans
                    && header.own_attributes.name == image.layer_attributes.name // header names were checked to be unique earlier
            },

            |image, meta, line| {
                let header = &meta[line.location.layer];
                debug_assert_eq!(header.own_attributes.name, image.layer_attributes.name, "irrelevant header should be filtered out"); // TODO this should be an error right?
                let channel = &header.channels.list[line.location.channel];

                let channel_index = {
                    if      channel.name.eq_case_insensitive("a") { 3 }
                    else if channel.name.eq_case_insensitive("b") { 2 }
                    else if channel.name.eq_case_insensitive("g") { 1 }
                    else if channel.name.eq_case_insensitive("r") { 0 }
                    else { return Ok(()); } // ignore non-rgba channels
                };

                let line_position = line.location.position;
                let Vec2(width, height) = image.resolution;

                let get_index_of_sample = move |sample_index| {
                    let location = line_position + Vec2(sample_index, 0);
                    debug_assert!(location.0 < width && location.1 < height, "coordinate out of range: {:?}", location);
                    SampleIndex { position: location, channel: channel_index }
                };

                let channel = match channel_index {
                    0 => image.channels.0, 1 => image.channels.1, 2 => image.channels.2,
                    3 => image.channels.3.expect("invalid alpha channel index"),
                    _ => panic!("invalid channel index"),
                };

                match channel.sample_type {
                    SampleType::F16 => for (sample_index, sample) in line.read_samples().enumerate() {
                        S::set_sample_f16(image, get_index_of_sample(sample_index), sample?);
                    },

                    SampleType::F32 => for (sample_index, sample) in line.read_samples().enumerate() {
                        S::set_sample_f32(image, get_index_of_sample(sample_index), sample?);
                    },

                    SampleType::U32 => for (sample_index, sample) in line.read_samples().enumerate() {
                        S::set_sample_u32(image, get_index_of_sample(sample_index), sample?);
                    },
                };

                Ok(())
            },

            options
        )
    }

    /// Allocate the memory for an image that could contain the described data.
    fn allocate(header: &Header, channels: Channels) -> Self where S: CreatePixels {
        let meta = Image {
            resolution: header.data_size,
            channels,

            data: (),

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
        };

        let data = S::new(&meta);

        Image {
            data,

            // .. meta
            resolution: meta.resolution,
            channels: meta.channels,
            image_attributes: meta.image_attributes,
            layer_attributes: meta.layer_attributes,
            encoding: meta.encoding
        }
    }

    /// Try to find a header matching the RGBA requirements.
    fn extract(headers: &[Header]) -> Result<Self> where S: CreatePixels {
        let first_header_name = headers.first()
            .and_then(|header| header.own_attributes.name.as_ref());

        for (header_index, header) in headers.iter().enumerate() {
            // the following check is required because filtering works by name in this RGBA implementation
            if header_index != 0 && header.own_attributes.name.as_ref() == first_header_name {
                return Err(Error::invalid("duplicate header name"))
            }

            let mut rgba = [None; 4];

            for channel in &header.channels.list {
                let rgba_channel = Some(Channel {
                    is_linear: channel.is_linear,
                    sample_type: channel.sample_type,
                });

                if      channel.name.eq_case_insensitive("a") { rgba[3] = rgba_channel; }
                else if channel.name.eq_case_insensitive("b") { rgba[2] = rgba_channel; }
                else if channel.name.eq_case_insensitive("g") { rgba[1] = rgba_channel; }
                else if channel.name.eq_case_insensitive("r") { rgba[0] = rgba_channel; }
            }

            if let [Some(r), Some(g), Some(b), a] = rgba {
                return Ok(Self::allocate(header, (r,g,b,a)))
            }
        }

        Err(Error::invalid("no valid RGB or RGBA image layer"))
    }

    /// Write the exr image to a file.
    /// Use `write_to_unbuffered` instead if you do not have a file.
    /// If an error occurs, attempts to delete the partially written file.
    #[must_use]
    pub fn write_to_file(
        &self, path: impl AsRef<Path>,
        options: WriteOptions<impl OnWriteProgress>
    ) -> UnitResult where S: GetPixels
    {
        crate::io::attempt_delete_file_on_write_error(path, |write|
            self.write_to_unbuffered(write, options)
        )
    }

    /// Buffer the writer and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    #[must_use]
    pub fn write_to_unbuffered(
        &self, write: impl Write + Seek,
        options: WriteOptions<impl OnWriteProgress>
    ) -> UnitResult where S: GetPixels
    {
        self.write_to_buffered(BufWriter::new(write), options)
    }

    /// Write the exr image to a writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    #[must_use]
    pub fn write_to_buffered(
        &self, write: impl Write + Seek,
        options: WriteOptions<impl OnWriteProgress>
    ) -> UnitResult where S: GetPixels
    {
        use crate::meta::attributes as meta;

        let header = Header::new(
            self.layer_attributes.name.clone().unwrap_or(Text::from("RGBA").unwrap()),
            self.resolution,
    if let Some(alpha) = self.channels.3 { smallvec![
                meta::Channel::new("A".try_into().unwrap(), alpha.sample_type, alpha.is_linear),
                meta::Channel::new("B".try_into().unwrap(), self.channels.2.sample_type, self.channels.2.is_linear),
                meta::Channel::new("G".try_into().unwrap(), self.channels.1.sample_type, self.channels.1.is_linear),
                meta::Channel::new("R".try_into().unwrap(), self.channels.0.sample_type, self.channels.0.is_linear),
            ] }

            else { smallvec![
                meta::Channel::new("B".try_into().unwrap(), self.channels.2.sample_type, self.channels.2.is_linear),
                meta::Channel::new("G".try_into().unwrap(), self.channels.1.sample_type, self.channels.1.is_linear),
                meta::Channel::new("R".try_into().unwrap(), self.channels.0.sample_type, self.channels.0.is_linear),
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
                    SampleIndex { position: location, channel: channel_index }
                };

                let channel = match channel_index {
                    0 => self.channels.0,
                    1 => self.channels.1,
                    2 => self.channels.2,
                    3 => self.channels.3.expect("invalid alpha channel index"),
                    _ => panic!("invalid channel index"),
                };

                match channel.sample_type {
                    SampleType::F16 => line.write_samples(|sample_index|{
                        S::get_sample_f16(self, get_index_of_sample(sample_index))
                    }).expect("rgba line write error"),

                    SampleType::F32 => line.write_samples(|sample_index|{
                        S::get_sample_f32(self, get_index_of_sample(sample_index))
                    }).expect("rgba line write error"),

                    SampleType::U32 => line.write_samples(|sample_index|{
                        S::get_sample_u32(self, get_index_of_sample(sample_index))
                    }).expect("rgba line write error"),
                };

                Ok(())
            },

            options
        )
    }
}

/// Contains some predefined pixel storages to put into the `rgba::Image<T>` type parameter.
/// Example:
/// ```
/// # use exr::prelude::*;
/// use exr::image::rgba::{ Image, pixels::Flattened as FlatPixels };
///
/// let image = Image::<FlatPixels<f16>>::read_from_file("file.exr", read_options::high());
/// ```
pub mod pixels {
    use super::*;

    /// Store all samples in a single array.
    /// All samples will be converted to the type `T`.
    /// This currently supports the sample types `f16`, `f32`, and `u32`.
    ///
    #[derive(PartialEq, Clone)]
    pub struct Flattened<T> {

        /// The flattened vector contains all rows one after another.
        /// In each row, for each pixel, its red, green, blue, and then alpha
        /// samples are stored one after another.
        ///
        /// Use `Flattened::flatten_sample_index(image, sample_index)`
        /// to compute the flat index of a specific sample.
        samples: Vec<T>,
    }

    impl<T> Flattened<T> {

        /// Compute the flat index of a specific sample. The computed index can be used with `Flattened.samples[index]`.
        /// Panics for invalid sample coordinates.
        #[inline]
        pub fn flatten_sample_index(image: &Image<Self>, index: SampleIndex) -> usize {
            debug_assert!(index.position.0 < image.resolution.0 && index.position.1 < image.resolution.1, "invalid pixel position");
            debug_assert!(index.channel < image.channel_count(), "invalid channel index");

            let pixel_index = index.position.1 * image.resolution.0 + index.position.0;
            pixel_index * image.channel_count() + index.channel
        }
    }

    impl GetPixels for Flattened<f16> {
        #[inline]
        fn get_sample_f32(image: &Image<Self>, index: SampleIndex) -> f32 {
            image.data.samples[Flattened::flatten_sample_index(image, index)].to_f32()
        }
    }

    impl CreatePixels for Flattened<f16> {
        #[inline]
        fn new(image: &Image<()>) -> Self {
            Flattened { samples: vec![f16::ZERO; image.resolution.area() * image.channel_count()] }
        }

        #[inline]
        fn set_sample_f32(image: &mut Image<Self>, index: SampleIndex, sample: f32) {
            let index = Self::flatten_sample_index(image, index);
            image.data.samples[index] = f16::from_f32(sample)
        }
    }

    impl GetPixels for Flattened<f32> {
        #[inline]
        fn get_sample_f32(image: &Image<Self>, index: SampleIndex) -> f32 {
            image.data.samples[Flattened::flatten_sample_index(image, index)]
        }
    }

    impl CreatePixels for Flattened<f32> {
        #[inline]
        fn new(image: &Image<()>) -> Self {
            Flattened { samples: vec![0.0; image.resolution.area() * image.channel_count()] }
        }

        #[inline]
        fn set_sample_f32(image: &mut Image<Self>, index: SampleIndex, sample: f32) {
            let index = Self::flatten_sample_index(image, index);
            image.data.samples[index] = sample
        }
    }

    impl GetPixels for Flattened<u32> {
        #[inline]
        fn get_sample_f32(image: &Image<Self>, index: SampleIndex) -> f32 {
            Self::get_sample_u32(image, index) as f32
        }

        #[inline]
        fn get_sample_u32(image: &Image<Self>, index: SampleIndex) -> u32 {
            image.data.samples[Flattened::flatten_sample_index(image, index)]
        }
    }

    impl CreatePixels for Flattened<u32> {
        #[inline]
        fn new(image: &Image<()>) -> Self {
            Flattened { samples: vec![0; image.resolution.area() * image.channel_count()] }
        }

        #[inline]
        fn set_sample_f32(image: &mut Image<Self>, index: SampleIndex, sample: f32) {
            Self::set_sample_u32(image, index, sample as u32)
        }

        #[inline]
        fn set_sample_u32(image: &mut Image<Self>, index: SampleIndex, sample: u32) {
            let index = Self::flatten_sample_index(image, index);
            image.data.samples[index] = sample
        }
    }

    use std::fmt::*;
    impl<T> Debug for Flattened<T> {
        #[inline]
        fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "[{}; {}]", std::any::type_name::<T>(), self.samples.len())
        }
    }
}
