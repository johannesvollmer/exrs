
//! Read and write a simple RGBA image.
//! This module loads the RGBA channels of any layer that contains RGB or RGBA channels.
//! Returns `Error::Invalid` if none can be found in the file.
//!
//! This module should only be used if you are confident that your images are really RGBA.
//! Use `exr::image::simple` if you need custom channels or specialized error handling.
//!
//! Also, the luxury of automatic conversion comes with a cost.
//! Using `image::simple` might be faster in special cases.

use crate::prelude::common::*;

use std::path::Path;
use std::fs::File;
use std::io::{Read, Seek, BufReader, Write, BufWriter, Cursor};
use crate::math::{RoundingMode};
use crate::error::{Result, Error, UnitResult};
use crate::meta::attribute::{SampleType, Text, LineOrder, TileDescription, LevelMode};
use std::convert::TryInto;
use crate::meta::{Blocks};
use half::f16;
use crate::image::{ReadOptions, OnReadProgress, WriteOptions, OnWriteProgress};
use crate::compression::Compression;
use crate::block::samples::Sample;
use crate::meta::header::Header;


/// A summary of an image file.
/// Does not contain any actual pixel data.
///
/// The given pixel values will be automatically converted to the type found in `Image.channels`.
///
/// To load an image, use `Image::load_pixels_from_file` or similar.
/// To store an image, use `image.write_pixels_to_file` or similar.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageInfo {

    /// The channel types of the written file.
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

/// The sample type of the image's RGBA channels. The alpha channel is optional.
/// The first channel is red, the second blue, the third green, and the fourth alpha.
pub type Channels = (SampleType, SampleType, SampleType, Option<SampleType>);

/// Specifies how the pixel data is formatted inside the file.
/// Does not affect any visual aspect, like positioning or orientation.
// TODO alsop nest encoding like this for meta::Header and simple::Image or even reuse this in image::simple
#[derive(Copy, Clone, Debug, PartialEq)]
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

/// This is the closure alias to `pub type CreatePixels<T> = impl FnOnce(&ImageInfo) -> T;`
///
/// The created value will later be filled with pixels.
///
/// This is a macro because type impl aliases are still unstable,
/// see https://github.com/rust-lang/rust/issues/63063
#[allow(non_snake_case)]
macro_rules! CreatePixels { ($T: ty) => { impl (FnOnce(&crate::image::rgba::ImageInfo) -> $T) }; }

/// This is the closure alias to `pub type SetPixels<T> = impl FnMut(&T, Vec2<usize>, Pixel);`
///
/// This is a macro because type impl aliases are still unstable,
/// see https://github.com/rust-lang/rust/issues/63063
#[allow(non_snake_case)]
macro_rules! SetPixels { ($T: ty) => { impl (FnMut(&mut $T, Vec2<usize>, Pixel)) }; }

/// This is the closure alias to `pub type GetPixels<'t> = impl Sync + Fn(Vec2<usize>) -> Pixel + 't;`
///
/// Extract a single RGBA pixel out of your image.
/// May return any variant of samples, and any alpha channel.
/// The samples will be converted to the type specified in the `ImageInfo::channels`.
/// The alpha value may be ignored by the image.
/// If the image has an alpha channel but no alpha value is provided,
/// a value of `1.0` is used as default alpha.
///
/// This is a macro because type impl aliases are still unstable,
/// see https://github.com/rust-lang/rust/issues/63063
#[allow(non_snake_case)]
macro_rules! GetPixels {
    () => { impl Sync + Fn(Vec2<usize>) -> Pixel };
    ($time: lifetime) => { impl Sync + Fn(Vec2<usize>) -> Pixel + $time };
}


/// A single pixel with red, green, blue, and alpha values.
/// Each channel may have a different sample type.
///
/// A Pixel can be created using `Pixel::rgb(0_f32, 0_u32, f16::ONE)` or `Pixel::rgba(0_f32, 0_u32, 0_f32, f16::ONE)`.
/// Additionally, a pixel can be converted from a tuple or array with either three or four components using `Pixel::from((0_u32, 0_f32, f16::ONE))`.
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

    /// Returns this pixel's alpha value, or the default value of `1.0` if no alpha is present.
    #[inline] pub fn alpha_or_default(&self) -> Sample {
        self.alpha.unwrap_or(Sample::default_alpha())
    }
}



impl Encoding {

    /// Chooses an adequate block size and line order for the specified compression.
    #[inline]
    pub fn for_compression(compression: Compression) -> Self {
        match compression {
            Compression::Uncompressed => Self {
                tile_size: None, // scan lines have maximum width, which is best for efficient line memcpy
                line_order: LineOrder::Increasing, // order does not really matter, no compression to be parallelized
                compression,
            },

            Compression::RLE => Self {
                tile_size: Some(Vec2(128, 128)), // favor tiles with one solid color
                line_order: LineOrder::Unspecified, // tiles can be compressed in parallel without sorting
                compression,
            },

            Compression::ZIP16 | Compression::ZIP1 => Self {
                tile_size: None, // maximum data size for zip compression
                line_order: LineOrder::Increasing, // cannot be unspecified with scan line blocks!
                compression,
            },

            _ => Self {
                compression,
                tile_size: Some(Vec2(256, 256)), // use tiles to enable unspecified line order
                line_order: LineOrder::Unspecified
            }
        }
    }

    /// Uses RLE compression with tiled 128x128 blocks.
    #[inline]
    pub fn fast() -> Self {
        Self::for_compression(Compression::RLE)
    }

    /// Uses ZIP16 compression with scan line blocks.
    #[inline]
    pub fn small() -> Self {
        Self::for_compression(Compression::ZIP16)
    }
}


/// Used to remember which header and which channels should be extracted from an image
struct ExtractionInfo {
    header_index: usize,
    channel_indices: (usize, usize, usize, Option<usize>),
}


impl ImageInfo {

    /// Create an Image with an alpha channel.
    /// All channels will have the specified sample type.
    /// Data is automatically converted to that type.
    /// Use `ImageInfo::new` where each channel should have a different sample type.
    pub fn rgba(resolution: impl Into<Vec2<usize>>, sample_type: SampleType) -> Self {
        Self::new(resolution, (sample_type, sample_type, sample_type, Some(sample_type)))
    }

    /// Create an Image without an alpha channel.
    /// All channels will have the specified sample type.
    /// Data is automatically converted to that type.
    /// Use `ImageInfo::new` where each channel should have a different sample type.
    pub fn rgb(resolution: impl Into<Vec2<usize>>, sample_type: SampleType) -> Self {
        Self::new(resolution, (sample_type, sample_type, sample_type, None))
    }

    /// Create an image with the resolution and channels.
    pub fn new(resolution: impl Into<Vec2<usize>>, channels: Channels) -> Self {
        let resolution = resolution.into();

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
    pub fn rgb_channels(&self) -> [SampleType; 3] {
        [self.channels.0, self.channels.1, self.channels.2]
    }

    /// Read the exr image from a file.
    /// Use `read_pixels_from_unbuffered` instead, if you do not have a file.
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// The `create_pixels` parameter is a closure of type `FnOnce(&Image) -> T`.
    /// The `set_pixels` parameter is a closure of type `FnMut(&mut T, Vec2<usize>, Pixel)`.
    #[inline]
    #[must_use]
    pub fn read_pixels_from_file<T>(
        path: impl AsRef<Path>,
        options: ReadOptions<impl OnReadProgress>,
        create_pixels: CreatePixels!(T),
        set_pixels: SetPixels!(T),
    ) -> Result<(Self, T)>
    {
        Self::read_pixels_from_unbuffered(File::open(path)?, options, create_pixels, set_pixels)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_pixels_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_pixels_from_file` instead, if you have a file path.
    ///
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// The `create_pixels` parameter is a closure of type `FnOnce(&Image) -> T`.
    /// The `set_pixels` parameter is a closure of type `FnMut(&mut T, Vec2<usize>, Pixel)`.
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[inline]
    #[must_use]
    pub fn read_pixels_from_unbuffered<T>(
        read: impl Read + Seek + Send,
        options: ReadOptions<impl OnReadProgress>,
        create_pixels: CreatePixels!(T),
        set_pixels: SetPixels!(T),
    ) -> Result<(Self, T)>
    {
        Self::read_pixels_from_buffered(BufReader::new(read), options, create_pixels, set_pixels)
    }

    /// Read the exr image from a reader.
    /// Use `read_pixels_from_file` instead, if you have a file path.
    /// Use `read_pixels_from_unbuffered` instead, if this is not an in-memory reader.
    ///
    /// Returns `Error::Invalid` if not at least one image part with RGB channels can be found in the file.
    ///
    /// The `create_pixels` parameter is a closure of type `FnOnce(&Image) -> T`.
    /// The `set_pixels` parameter is a closure of type `FnMut(&mut T, Vec2<usize>, Pixel)`.
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[inline]
    #[must_use]
    pub fn read_pixels_from_buffered<T>(
        read: impl Read + Seek + Send,
        options: ReadOptions<impl OnReadProgress>,
        create_pixels: CreatePixels!(T),
        mut set_pixels: SetPixels!(T),
    ) -> Result<(Self, T)>
    {
        let (_extraction, info, pixels) = crate::block::read_filtered_blocks_from_buffered(
            read,

            move |meta| {
                let (extraction, image) = Self::extract(meta)?;
                let pixels = create_pixels(&image);
                Ok((extraction, image, pixels))
            },

            // only keep the one header we selected earlier
            |(extraction, _image, _pixels), (header_index, _header), (_tile_index, tile)| {
                tile.location.is_largest_resolution_level() // also skip multi-resolution shenanigans
                    && header_index == extraction.header_index
            },

            |(extraction, image, pixels), meta, block| {
                let (r_type, g_type, b_type, a_type) = image.channels;

                let header: &Header = &meta[block.index.layer];
                debug_assert_eq!(header.own_attributes.name, image.layer_attributes.name, "irrelevant header should be filtered out"); // TODO this should be an error right?
                let line_bytes = block.index.pixel_size.0 * header.channels.bytes_per_pixel;

                // TODO compute this once per image, not per block
                let (mut r_range, mut g_range, mut b_range, mut a_range) = (0..0, 0..0, 0..0, 0..0);
                let mut byte_index = 0;

                for (channel_index, channel) in header.channels.list.iter().enumerate() {
                    let sample_bytes = channel.sample_type.bytes_per_sample();
                    let channel_bytes = block.index.pixel_size.0 * sample_bytes;
                    let byte_range = byte_index .. byte_index + channel_bytes;
                    byte_index = byte_range.end;

                    if      Some(channel_index) == extraction.channel_indices.3 { a_range = byte_range }
                    else if channel_index == extraction.channel_indices.2 { b_range = byte_range }
                    else if channel_index == extraction.channel_indices.1 { g_range = byte_range }
                    else if channel_index == extraction.channel_indices.0 { r_range = byte_range }
                    else { continue; } // ignore non-rgba channels
                };

                let byte_lines = block.data.chunks_exact(line_bytes);
                let y_coords = 0 .. block.index.pixel_size.height();
                for (y, byte_line) in y_coords.zip(byte_lines) {

                    let mut next_r = sample_reader(r_type, &byte_line[r_range.clone()]);
                    let mut next_g = sample_reader(g_type, &byte_line[g_range.clone()]);
                    let mut next_b = sample_reader(b_type, &byte_line[b_range.clone()]);
                    let mut next_a = a_type
                        .map(|a_type| sample_reader(a_type, &block.data[a_range.clone()]));

                    fn sample_reader<'a, R: Read + 'a>(sample_type: SampleType, mut read: R) -> Box<dyn 'a + FnMut() -> Result<Sample>> {
                        use crate::io::Data;

                        // WITH ENUM MATCHING EACH SAMPLE:
                        // test read_full   ... bench:  31,670,900 ns/iter (+/- 2,653,097)
                        // test read_rgba   ... bench: 120,208,940 ns/iter (+/- 2,972,441)

                        // WITH DYNAMIC DISPATCH:
                        // test read_full   ... bench:  31,387,880 ns/iter (+/- 1,100,514)
                        // test read_rgba   ... bench: 111,231,040 ns/iter (+/- 2,872,627)
                        match sample_type {
                            SampleType::F16 => Box::new(move || Ok(Sample::from(f16::read(&mut read)?))),
                            SampleType::F32 => Box::new(move || Ok(Sample::from(f32::read(&mut read)?))),
                            SampleType::U32 => Box::new(move || Ok(Sample::from(u32::read(&mut read)?))),
                        }
                    }

                    for x in 0..block.index.pixel_size.0 {
                        let pixel = Pixel::new(
                            next_r()?, next_g()?, next_b()?,
                            if let Some(a) = &mut next_a { Some(a()?) } else { None }
                        );

                        let position = block.index.pixel_position + Vec2(x,y);
                        set_pixels(pixels, position, pixel);
                    }
                }

                Ok(())
            },

            options
        )?;

        Ok((info, pixels))
    }

    /// Allocate the memory for an image that could contain the described data.
    fn allocate(header: &Header, channels: Channels) -> Self {
        ImageInfo {
            channels,
            resolution: header.data_size,

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
    fn extract(headers: &[Header]) -> Result<(ExtractionInfo, Self)> {
        for (header_index, header) in headers.iter().enumerate() {
            let mut rgba_types  = [None; 4];

            for (channel_index, channel) in header.channels.list.iter().enumerate() {
                let channel_type = Some((channel_index, channel.sample_type));

                if      channel.name.eq_case_insensitive("a") { rgba_types[3] = channel_type; }
                else if channel.name.eq_case_insensitive("b") { rgba_types[2] = channel_type; }
                else if channel.name.eq_case_insensitive("g") { rgba_types[1] = channel_type; }
                else if channel.name.eq_case_insensitive("r") { rgba_types[0] = channel_type; }
            }

            if let [Some(r), Some(g), Some(b), a] = rgba_types {
                return Ok((
                    ExtractionInfo { header_index, channel_indices: (r.0, g.0, b.0, a.map(|a| a.0)) },
                    Self::allocate(header, (r.1, g.1, b.1, a.map(|a| a.1)))
                ))
            }
        }

        Err(Error::invalid("no valid RGB or RGBA image layer"))
    }

    /// Write the exr image to a file.
    /// Use `write_pixels_to_unbuffered` instead if you do not have a file.
    /// If an error occurs, attempts to delete the partially written file.
    ///
    /// The `pixels` parameter is a closure of type `Fn(&Image, Vec2<usize>) -> Pixel`.
    #[must_use]
    pub fn write_pixels_to_file(
        &self, path: impl AsRef<Path>,
        options: WriteOptions<impl OnWriteProgress>,
        pixels: GetPixels!(),
    ) -> UnitResult
    {
        crate::io::attempt_delete_file_on_write_error(path, |write|
            self.write_pixels_to_unbuffered(write, options, pixels)
        )
    }

    /// Buffer the writer and then write the exr image to it.
    /// Use `write_pixels_to_unbuffered` instead, if your reader is an in-memory writer.
    /// Use `write_pixels_to_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    ///
    /// The `pixels` parameter is a closure of type `Fn(&Image, Vec2<usize>) -> Pixel`.
    #[must_use]
    pub fn write_pixels_to_unbuffered(
        &self, write: impl Write + Seek,
        options: WriteOptions<impl OnWriteProgress>,
        pixels: GetPixels!(),
    ) -> UnitResult
    {
        self.write_pixels_to_buffered(BufWriter::new(write), options, pixels)
    }

    /// Write the exr image to a writer.
    /// Use `write_pixels_to_file` instead, if you have a file path.
    /// Use `write_pixels_to_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    ///
    /// The `pixels` parameter is a closure of type `Fn(&Image, Vec2<usize>) -> Pixel`.
    #[must_use]
    pub fn write_pixels_to_buffered(
        &self, write: impl Write + Seek,
        options: WriteOptions<impl OnWriteProgress>,
        pixels: GetPixels!(),
    ) -> UnitResult
    {
        use crate::meta::attribute as meta;

        let header = Header::new(
            self.layer_attributes.name.clone().unwrap_or(Text::from("RGBA").unwrap()),
            self.resolution,
            {
                if let Some(alpha) = self.channels.3 { smallvec![
                    meta::ChannelInfo::new("A".try_into().unwrap(), alpha, true), // store as linear data
                    meta::ChannelInfo::new("B".try_into().unwrap(), self.channels.2, false),
                    meta::ChannelInfo::new("G".try_into().unwrap(), self.channels.1, false),
                    meta::ChannelInfo::new("R".try_into().unwrap(), self.channels.0, false),
                ] }

                else { smallvec![
                    meta::ChannelInfo::new("B".try_into().unwrap(), self.channels.2, false),
                    meta::ChannelInfo::new("G".try_into().unwrap(), self.channels.1, false),
                    meta::ChannelInfo::new("R".try_into().unwrap(), self.channels.0, false),
                ] }
            }
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
            smallvec![ header ],

            |meta, block_index| {
                let header = &meta.get(block_index.layer).expect("invalid block index");
                let block_bytes = block_index.pixel_size.area() * header.channels.bytes_per_pixel;

                let width = block_index.pixel_size.0;
                let line_bytes = width * header.channels.bytes_per_pixel;

                // alpha would always start at 0, then comes b, g, r.
                let (r_type, g_type, b_type, a_type) = self.channels;
                let r_line_bytes = width * r_type.bytes_per_sample();
                let g_line_bytes = width * g_type.bytes_per_sample();
                let b_line_bytes = width * b_type.bytes_per_sample();
                let a_line_bytes = a_type
                    .map(|a_type| width * a_type.bytes_per_sample())
                    .unwrap_or(0);

                let mut block_bytes = vec![0_u8; block_bytes];

                let y_coordinates = 0..block_index.pixel_size.height();
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
                        let pixel = pixels(position);

                        write_r(pixel.red);
                        write_g(pixel.green);
                        write_b(pixel.blue);

                        if let Some(write_a) = &mut write_a {
                            write_a(pixel.alpha_or_default()); // no alpha channel provided = not transparent
                        }
                    }
                }

                block_bytes
            },

            options
        )
    }
}


/// Provides some predefined pixel containers for RGBA images.
/// Currently contains a homogeneous flattened vector storage.
pub mod pixels {
    use super::*;


    /// Store all samples in a single array.
    /// All samples will be converted to the type `T`.
    /// This currently supports the sample types `f16`, `f32`, and `u32`.
    #[derive(PartialEq, Clone)]
    pub struct Flattened<T> {

        channels: usize,
        width: usize,

        /// The flattened vector contains all rows one after another.
        /// In each row, for each pixel, its red, green, blue, and then alpha
        /// samples are stored one after another.
        ///
        /// Use `Flattened::compute_pixel_index(image, position)`
        /// to compute the flat index of a specific pixel.
        pub samples: Vec<T>,
    }

    impl<T> Flattened<T> {

        /// Compute the flat index of a specific pixel. Returns a range of either 3 or 4 samples.
        /// The computed index can be used with `Flattened.samples[index]`.
        /// Panics for invalid sample coordinates.
        #[inline]
        pub fn compute_pixel_index(&self, position: Vec2<usize>) -> std::ops::Range<usize> {
            let pixel_index = position.y() * self.width + position.x();
            let red_index = pixel_index * self.channels;
            red_index .. red_index + self.channels
        }
    }

    /// Constructor for a flattened f16 pixel storage.
    /// This function an directly be passed to `rgba::ImageInfo::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<f16>` image.
    #[inline] pub fn create_flattened_f16(image: &ImageInfo) -> Flattened<f16> {
        Flattened {
            width: image.resolution.0,
            channels: image.channel_count(),
            samples: vec![f16::ZERO; image.resolution.area() * image.channel_count()]
        }
    }

    /// Constructor for a flattened f32 pixel storage.
    /// This function an directly be passed to `rgba::ImageInfo::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<f32>` image.
    #[inline] pub fn create_flattened_f32(image: &ImageInfo) -> Flattened<f32> {
        Flattened {
            width: image.resolution.0,
            channels: image.channel_count(),
            samples: vec![0.0; image.resolution.area() * image.channel_count()]
        }
    }

    /// Constructor for a flattened u32 pixel storage.
    /// This function an directly be passed to `rgba::ImageInfo::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<u32>` image.
    #[inline] pub fn create_flattened_u32(image: &ImageInfo) -> Flattened<u32> {
        Flattened {
            width: image.resolution.0,
            channels: image.channel_count(),
            samples: vec![0; image.resolution.area() * image.channel_count()]
        }
    }

    /// Create an object that can examine the pixels of a `Flattened<T>` image.
    #[inline]
    pub fn flattened_pixel_getter<T>(image: &Flattened<T>) -> GetPixels!('_)
        where T: Sync + Copy + Into<Sample>
    {
        move |position: Vec2<usize>| {
            let pixel = &image.samples[image.compute_pixel_index(position)];
            Pixel::new(pixel[0], pixel[1], pixel[2], pixel.get(3).cloned())
        }
    }

    /// Create an object that can update the pixels of a `Flattened<T>` image.
    #[inline]
    pub fn flattened_pixel_setter<T>() -> SetPixels!(Flattened<T>) where T: Copy + From<Sample> {
        |image: &mut Flattened<T>, position: Vec2<usize>, pixel: Pixel| {
            let index = image.compute_pixel_index(position);
            let samples = &mut image.samples[index];

            samples[0] = pixel.red.into();
            samples[1] = pixel.green.into();
            samples[2] = pixel.blue.into();

            if samples.len() == 4 {
                samples[3] = pixel.alpha_or_default().into();
            }
        }
    }


    use std::fmt::*;
    impl<T> Debug for Flattened<T> {
        #[inline] fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "[{}; {}]", std::any::type_name::<T>(), self.samples.len())
        }
    }
}




impl<R, G, B> From<(R, G, B)> for Pixel where R: Into<Sample>, G: Into<Sample>, B: Into<Sample> {
    #[inline] fn from((r,g,b): (R, G, B)) -> Self { Self::rgb(r,g,b) }
}

impl<R, G, B, A> From<(R, G, B, A)> for Pixel where R: Into<Sample>, G: Into<Sample>, B: Into<Sample>, A: Into<Sample> {
    #[inline] fn from((r,g,b,a): (R, G, B, A)) -> Self { Self::rgba(r,g,b, a) }
}

impl<R, G, B> From<Pixel> for (R, G, B) where R: From<Sample>, G: From<Sample>, B: From<Sample> {
    #[inline] fn from(pixel: Pixel) -> Self { (R::from(pixel.red), G::from(pixel.green), B::from(pixel.blue)) }
}

impl<R, G, B, A> From<Pixel> for (R, G, B, A) where R: From<Sample>, G: From<Sample>, B: From<Sample>, A: From<Sample> {
    #[inline] fn from(pixel: Pixel) -> Self { (
        R::from(pixel.red), G::from(pixel.green), B::from(pixel.blue),
        A::from(pixel.alpha_or_default())
    ) }
}

impl<S> From<[S; 3]> for Pixel where S: Into<Sample> {
    #[inline] fn from([r,g,b]: [S; 3]) -> Self { Self::rgb(r,g,b) }
}

impl<S> From<[S; 4]> for Pixel where S: Into<Sample> {
    #[inline] fn from([r,g,b, a]: [S; 4]) -> Self { Self::rgba(r,g,b, a) }
}

impl<S> From<Pixel> for [S; 3] where S: From<Sample> {
    #[inline] fn from(pixel: Pixel) -> Self { [S::from(pixel.red), S::from(pixel.green), S::from(pixel.blue)] }
}

impl<S> From<Pixel> for [S; 4] where S: From<Sample> {
    #[inline] fn from(pixel: Pixel) -> Self { [
        S::from(pixel.red), S::from(pixel.green), S::from(pixel.blue),
        S::from(pixel.alpha_or_default())
    ] }
}