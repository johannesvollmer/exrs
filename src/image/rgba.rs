
//! Read and write a simple RGBA image.
//! This module loads the RGBA channels of any layer that contains RGB or RGBA channels.
//! Returns `Error::Invalid` if none can be found in the file.
//!
//! This module should only be used if you are confident that your images are really RGBA.
//! Use `exr::image::simple` if you need custom channels or specialized error handling.
//!
//! Also, the luxury of automatic conversion comes with a cost.
//! Using `image::simple` might be slightly faster in special cases.


use std::path::Path;
use std::fs::File;
use std::io::{Read, Seek, BufReader, Write, BufWriter, Cursor};
use crate::math::{Vec2, RoundingMode};
use crate::error::{Result, Error, UnitResult};
use crate::meta::attributes::{SampleType, Text, LineOrder, TileDescription, LevelMode};
use std::convert::TryInto;
use crate::meta::{Header, ImageAttributes, LayerAttributes, MetaData, Blocks};
use half::f16;
use crate::image::{ReadOptions, OnReadProgress, WriteOptions, OnWriteProgress};
use crate::compression::Compression;
use std::collections::HashSet;
use crate::block::samples::Sample;

/// A summary of an image file.
/// Does not contain any actual pixel data.
///
/// The given pixel values will be automatically converted to the type found in `Image.channels`.
///
/// To load an image, use `Image::load_from_file` or similar.
/// To store an image, use `image.write_to_file` or similar.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {

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
    pub is_linear: bool,

    /// The type of the samples in this channel. Either f32, f16, or u32.
    pub sample_type: SampleType,
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
/// The actual sample type of the file is specified within `Image.channels`.
/// All provided samples will automatically be converted to the desired channel type.
pub trait GetPixels: Sync { // TODO does not actually always need sync

    /// Extract a single RGBA pixel out of your image.
    /// May return any variant of samples, and any alpha channel.
    /// The samples will be converted to the type specified in the image.
    /// The alpha value may be ignored by the image.
    /// If the image has an alpha channel but no alpha value is provided,
    /// a value of `1.0` is used as default alpha.
    fn get_pixel(&self, image: &Image, position: Vec2<usize>) -> Pixel;
}

/// Create the pixels of an image file. Implement this for your own image type to read a file into your image.
pub trait CreatePixels {

    /// The type of Pixels created by this object.
    /// The created value will later be filled with pixels.
    type Pixels: SetPixels;

    /// Create a new pixel storage for the supplied image.
    fn new(self, image: &Image) -> Self::Pixels;
}

/// Consume the pixels of an image file. Implement this on your own image type to read a file into your image.
pub trait SetPixels {

    /// Set the value of a single pixel.
    fn set_pixel(&mut self, image: &Image, position: Vec2<usize>, pixel: Pixel);
}

/// A single pixel with red, green, blue, and alpha samples.
/// Each channel may have a different sample type.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Pixel {

    /// The red component of this pixel.
    pub red: Sample,

    /// The red component of this pixel.
    pub green: Sample,

    /// The red component of this pixel.
    pub blue: Sample,

    /// The alpha component of this pixel.
    /// Most images will keep this number between zero and one.
    pub alpha: Option<Sample>,
}

impl Pixel {

    /// Create a new pixel without the specified samples. Accepts f32, u32, and f16 values for each sample.
    #[inline] pub fn new(red: impl Into<Sample>, green: impl Into<Sample>, blue: impl Into<Sample>, alpha: Option<impl Into<Sample>>) -> Self {
        Self { red: red.into(), green: green.into(), blue: blue.into(), alpha: alpha.map(Into::into) }
    }

    /// Create a new pixel without an alpha sample. Accepts f32, u32, and f16 values for each sample.
    #[inline] pub fn rgb(red: impl Into<Sample>, green: impl Into<Sample>, blue: impl Into<Sample>) -> Self {
        Self::new(red, green, blue, Option::<f32>::None)
    }

    /// Create a new pixel with an alpha sample. Accepts f32, u32, and f16 values for each sample.
    #[inline] pub fn rgba(red: impl Into<Sample>, green: impl Into<Sample>, blue: impl Into<Sample>, alpha: impl Into<Sample>) -> Self {
        Self::new(red, green, blue, Some(alpha))
    }
}

impl Channel {

    /// A new channel in linear color space.
    pub fn linear(sample_type: SampleType) -> Self {
        Self { is_linear: true, sample_type }
    }

    /// A new channel in non-linear color space.
    pub fn non_linear(sample_type: SampleType) -> Self {
        Self { is_linear: false, sample_type }
    }
}


impl<F> GetPixels for F where F: Sync + Fn(&Image, Vec2<usize>) -> Pixel {
    #[inline] fn get_pixel(&self, image: &Image, position: Vec2<usize>) -> Pixel { self(image, position) }
}

impl<F, T> CreatePixels for F where F: FnOnce(&Image) -> T, T: SetPixels {
    type Pixels = T;
    #[inline] fn new(self, image: &Image) -> Self::Pixels { self(image) }
}

impl<F> SetPixels for F where F: FnMut(&Image, Vec2<usize>, Pixel) {
    #[inline] fn set_pixel(&mut self, image: &Image, position: Vec2<usize>, pixel: Pixel) { self(image, position, pixel) }
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
                tile_size: Some(Vec2(128, 128)), // favor tiles with one solid color
                line_order: LineOrder::Unspecified, // tiles can be compressed in parallel
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
                line_order: LineOrder::Increasing // basically free
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

    /// Create an Image with an alpha channel. Each channel will be the same as the specified channel.
    pub fn with_alpha(resolution: Vec2<usize>, channel: Channel) -> Self {
        Self::new(resolution, (channel, channel, channel, Some(channel)))
    }

    /// Create an Image without an alpha channel. Each channel will be the same as the specified channel.
    pub fn without_alpha(resolution: Vec2<usize>, channel: Channel) -> Self {
        Self::new(resolution, (channel, channel, channel, None))
    }

    /// Create an image with the resolution and channels.
    pub fn new(resolution: Vec2<usize>, channels: Channels) -> Self {
        Self {
            resolution, channels,
            image_attributes: ImageAttributes::new(resolution),
            layer_attributes: LayerAttributes::new(Text::from("RGBA").expect("ascii bug")),
            encoding: Encoding::fast()
        }
    }

    /// Set the display window and data window position of this image.
    pub fn with_position(mut self, position: impl Into<Vec2<i32>>) -> Self {
        let position: Vec2<i32> = position.into();
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

    /// Return the red green and blue channels as an indexable array.
    #[inline]
    pub fn rgb_channels(&self) -> [Channel; 3] {
        [self.channels.0, self.channels.1, self.channels.2]
    }

    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// The `create_pixels` parameter can be a closure of type `Fn(&Image) -> impl SetPixels`.
    #[inline]
    #[must_use]
    pub fn read_from_file<P: CreatePixels>(
        path: impl AsRef<Path>,
        options: ReadOptions<impl OnReadProgress>,
        create_pixels: P,
    ) -> Result<(Self, P::Pixels)>
    {
        Self::read_from_unbuffered(File::open(path)?, options, create_pixels)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    ///
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// The `create_pixels` parameter can be a closure of type `Fn(&Image) -> impl SetPixels`.
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[inline]
    #[must_use]
    pub fn read_from_unbuffered<P: CreatePixels>(
        read: impl Read + Seek + Send,
        options: ReadOptions<impl OnReadProgress>,
        create_pixels: P,
    ) -> Result<(Self, P::Pixels)>
    {
        Self::read_from_buffered(BufReader::new(read), options, create_pixels)
    }

    /// Read the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    ///
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// The `create_pixels` parameter can be a closure of type `Fn(&Image) -> impl SetPixels`.
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[inline]
    #[must_use]
    pub fn read_from_buffered<P: CreatePixels>(
        read: impl Read + Seek + Send,
        options: ReadOptions<impl OnReadProgress>,
        create_pixels: P,
    ) -> Result<(Self, P::Pixels)>
    {
        crate::image::read_filtered_blocks_from_buffered(
            read,

            move |meta| {
                let image = Self::extract(meta)?;
                let pixels = create_pixels.new(&image);
                Ok((image, pixels))
            },

            // only keep the one header we selected earlier
            |(image, _pixels), header, tile| {
                tile.location.is_largest_resolution_level() // also skip multi-resolution shenanigans
                    && header.own_attributes.name == image.layer_attributes.name // header names were checked to be unique earlier
            },

            |(image, pixels), meta, block| {
                let r_type = image.channels.0.sample_type;
                let g_type = image.channels.1.sample_type;
                let b_type = image.channels.2.sample_type;
                let a_type = image.channels.3.map(|a| a.sample_type);

                let header: &Header = &meta[block.index.layer];
                debug_assert_eq!(header.own_attributes.name, image.layer_attributes.name, "irrelevant header should be filtered out"); // TODO this should be an error right?
                let line_bytes = block.index.pixel_size.0 * header.channels.bytes_per_pixel;

                // TODO compute this once per image, not per block
                let (mut r_range, mut g_range, mut b_range, mut a_range) = (0..0, 0..0, 0..0, 0..0);
                let mut byte_index = 0;

                for channel in &header.channels.list {
                    let sample_bytes = channel.sample_type.bytes_per_sample();
                    let channel_bytes = block.index.pixel_size.0 * sample_bytes;
                    let byte_range = byte_index .. byte_index + channel_bytes;
                    byte_index = byte_range.end;

                    if      channel.name.eq_case_insensitive("a") { a_range = byte_range }
                    else if channel.name.eq_case_insensitive("b") { b_range = byte_range }
                    else if channel.name.eq_case_insensitive("g") { g_range = byte_range }
                    else if channel.name.eq_case_insensitive("r") { r_range = byte_range }
                    else { continue; } // ignore non-rgba channels
                };

                let byte_lines = block.data.chunks_exact(line_bytes);
                let y_coords = 0 .. block.index.pixel_size.1;
                for (y, byte_line) in y_coords.zip(byte_lines) {

                    let mut next_r = sample_reader(r_type, &byte_line[r_range.clone()]);
                    let mut next_g = sample_reader(g_type, &byte_line[g_range.clone()]);
                    let mut next_b = sample_reader(b_type, &byte_line[b_range.clone()]);
                    let mut next_a = a_type
                        .map(|a_type| sample_reader(a_type, &block.data[a_range.clone()]));

                    fn sample_reader(sample_type: SampleType, mut read: impl Read) -> impl (FnMut() -> Result<Sample>) {
                        use crate::io::Data;

                        move || Ok(match sample_type { // TODO this is the hot path
                            SampleType::F16 => Sample::F16(f16::read(&mut read)?),
                            SampleType::F32 => Sample::F32(f32::read(&mut read)?),
                            SampleType::U32 => Sample::U32(u32::read(&mut read)?),
                        })
                    }

                    for x in 0..block.index.pixel_size.0 {
                        let pixel = Pixel::new(
                            next_r()?, next_g()?, next_b()?,
                            if let Some(a) = &mut next_a { Some(a()?) } else { None }
                        );

                        let position = block.index.pixel_position + Vec2(x,y);
                        pixels.set_pixel(image, position, pixel);
                    }
                }

                Ok(())
            },

            options
        )
    }

    /// Allocate the memory for an image that could contain the described data.
    fn allocate(header: &Header, channels: Channels) -> Self {
        Image {
            resolution: header.data_size,
            channels,

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
        let mut header_names = HashSet::with_capacity(headers.len());

        for header in headers {
            // the following check is required because filtering works by name in this RGBA implementation
            if !header_names.insert(&header.own_attributes.name) { // none twice is also catched
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
    ///
    /// The `pixels` parameter can be a closure of type `Fn(&Image, Vec2<usize>) -> Pixel`.
    #[must_use]
    pub fn write_to_file(
        &self, path: impl AsRef<Path>,
        options: WriteOptions<impl OnWriteProgress>,
        pixels: &impl GetPixels,
    ) -> UnitResult
    {
        crate::io::attempt_delete_file_on_write_error(path, |write|
            self.write_to_unbuffered(write, options, pixels)
        )
    }

    /// Buffer the writer and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    ///
    /// The `pixels` parameter can be a closure of type `Fn(&Image, Vec2<usize>) -> Pixel`.
    #[must_use]
    pub fn write_to_unbuffered(
        &self, write: impl Write + Seek,
        options: WriteOptions<impl OnWriteProgress>,
        pixels: &impl GetPixels,
    ) -> UnitResult
    {
        self.write_to_buffered(BufWriter::new(write), options, pixels)
    }

    /// Write the exr image to a writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    ///
    /// The `pixels` parameter can be a closure of type `Fn(&Image, Vec2<usize>) -> Pixel`.
    #[must_use]
    pub fn write_to_buffered(
        &self, write: impl Write + Seek,
        options: WriteOptions<impl OnWriteProgress>,
        pixels: &impl GetPixels,
    ) -> UnitResult
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


        crate::block::lines::write_all_tiles_to_buffered(
            write,
            MetaData::new(smallvec![ header ]),

            |meta, block_index| {
                let header = &meta.get(block_index.layer).expect("invalid block index");
                let block_bytes = block_index.pixel_size.area() * header.channels.bytes_per_pixel;

                let width = block_index.pixel_size.0;
                let line_bytes = width * header.channels.bytes_per_pixel;

                // alpha would always start at 0, then comes b, g, r.
                // let a_byte_range = a_type.map(|a| b_byte_range.end .. b_byte_range.end + width * a.bytes_per_sample());
                let a_type = self.channels.3.map(|a| a.sample_type);
                let (r_type, g_type, b_type) = (
                    self.channels.0.sample_type, self.channels.1.sample_type, self.channels.2.sample_type
                );

                let r_line_bytes = width * r_type.bytes_per_sample();
                let g_line_bytes = width * g_type.bytes_per_sample();
                let b_line_bytes = width * b_type.bytes_per_sample();
                let a_line_bytes = a_type
                    .map(|a_type| width * a_type.bytes_per_sample())
                    .unwrap_or(0);

                let mut block_bytes = vec![0_u8; block_bytes];

                let y_coordinates = 0..block_index.pixel_size.1;
                let byte_lines = block_bytes.chunks_exact_mut(line_bytes);
                for (y, line_bytes) in y_coordinates.zip(byte_lines) {

                    let (a, line_bytes) = line_bytes.split_at_mut(a_line_bytes);
                    let (b, line_bytes) = line_bytes.split_at_mut(b_line_bytes);
                    let (g, line_bytes) = line_bytes.split_at_mut(g_line_bytes);
                    let (r, line_bytes) = line_bytes.split_at_mut(r_line_bytes);
                    debug_assert!(line_bytes.is_empty());

                    fn sample_writer(sample_type: SampleType, mut write: impl Write) -> impl FnMut(Sample) {
                        use crate::io::Data;

                        move |sample| {
                            match sample_type {
                                SampleType::F16 => sample.to_f16().write(&mut write).expect("write to buffer error"),
                                SampleType::F32 => sample.to_f32().write(&mut write).expect("write to buffer error"),
                                SampleType::U32 => sample.to_u32().write(&mut write).expect("write to buffer error"),
                            }
                        }
                    }

                    let mut write_r = sample_writer(r_type, Cursor::new(r));
                    let mut write_g = sample_writer(g_type, Cursor::new(g));
                    let mut write_b = sample_writer(b_type, Cursor::new(b));
                    let mut write_a = a_type.map(|a_type| sample_writer(a_type, Cursor::new(a)));

                    for x in 0..width {
                        let position = block_index.pixel_position + Vec2(x,y);
                        let pixel = pixels.get_pixel(self, position);

                        write_r(pixel.red);
                        write_g(pixel.green);
                        write_b(pixel.blue);

                        if let Some(write_a) = &mut write_a {
                            write_a(pixel.alpha.unwrap_or(Sample::F32(1.0))); // no alpha channel provided = not transparent
                        }
                    }
                }

                block_bytes
            },

            options
        )
    }
}


/// Provides some predefined pixel storages for RGBA images.
/// Currently contains a homogeneous flattened vector storage.
pub mod pixels {
    use super::*;

    /// Constructor for a flat f16 pixel storage.
    /// This function an directly be passed to `rgba::Image::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<f16>` image.
    #[inline] pub fn flat_f16(image: &Image) -> Flattened<f16> {
        Flattened { samples: vec![f16::ZERO; image.resolution.area() * image.channel_count()] }
    }

    /// Constructor for a flat f32 pixel storage.
    /// This function an directly be passed to `rgba::Image::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<f32>` image.
    #[inline] pub fn flat_f32(image: &Image) -> Flattened<f32> {
        Flattened { samples: vec![0.0; image.resolution.area() * image.channel_count()] }
    }

    /// Constructor for a flat u32 pixel storage.
    /// This function an directly be passed to `rgba::Image::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<u32>` image.
    #[inline] pub fn flat_u32(image: &Image) -> Flattened<u32> {
        Flattened { samples: vec![0; image.resolution.area() * image.channel_count()] }
    }

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
        pub fn flatten_sample_index(image: &Image, position: Vec2<usize>, channel: usize) -> usize {
            debug_assert!(position.0 < image.resolution.0 && position.1 < image.resolution.1, "invalid pixel position");
            debug_assert!(channel < image.channel_count(), "invalid channel index");

            let pixel_index = position.1 * image.resolution.0 + position.0;
            pixel_index * image.channel_count() + channel
        }
    }

    impl<T> GetPixels for Flattened<T> where T: Sync + Copy + Into<Sample> {
        #[inline] fn get_pixel(&self, image: &Image, position: Vec2<usize>) -> Pixel {
            Pixel::new(
                self.samples[Self::flatten_sample_index(image, position, 0)],
                self.samples[Self::flatten_sample_index(image, position, 1)],
                self.samples[Self::flatten_sample_index(image, position, 2)],
                image.channels.3.map(|_| self.samples[Self::flatten_sample_index(image, position, 3)]),
            )
        }
    }

    impl<T> SetPixels for Flattened<T> where T: From<Sample> {
        #[inline] fn set_pixel(&mut self, image: &Image, position: Vec2<usize>, pixel: Pixel) {
            self.samples[Self::flatten_sample_index(image, position, 0)] = T::from(pixel.red);
            self.samples[Self::flatten_sample_index(image, position, 1)] = T::from(pixel.green);
            self.samples[Self::flatten_sample_index(image, position, 2)] = T::from(pixel.blue);

            if let Some(a) = pixel.alpha {
                self.samples[Self::flatten_sample_index(image, position, 3)] = T::from(a);
            }
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

