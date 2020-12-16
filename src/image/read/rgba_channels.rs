
//! Read and write a simple RGBA image.
//! This module loads the RGBA channels of any layer that contains RGB or RGBA channels.
//! Returns `Error::Invalid` if none can be found in the file.
//!
//! This module should only be used if you are confident that your images are really RGBA.
//!
//! Furthermore, this is not a zero-cost abstraction.
//! Use `read().all_channels()` with a filter instead, if performance is critical.

use crate::image::*;
use crate::io::Read;
use crate::meta::header::{Header};
use crate::error::{Result, UnitResult, Error};
use crate::block::UncompressedBlock;
use crate::math::Vec2;
use crate::image::read::layers::{ChannelsReader, ReadChannels, ReadFirstValidLayer, ReadAllLayers};
use crate::block::samples::Sample;
use crate::block::chunk::TileCoordinates;


#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadRgbaChannels<Create, Set> {
    pub create: Create,
    pub set_pixel: Set
}

impl<'s, C, S> ReadRgbaChannels<C, S> where Self: ReadChannels<'s> {
    pub fn first_valid_layer(self) -> ReadFirstValidLayer<Self> { ReadFirstValidLayer { read_channels: self } }
    pub fn all_layers(self) -> ReadAllLayers<Self> { ReadAllLayers { read_channels: self } }
}


pub trait SetRgbaPixel<P> {
    fn set_pixel(&self, pixels: &mut P, position: Vec2<usize>, pixel: RgbaPixel);
}

pub trait CreateRgbaPixels {
    type Pixels;
    fn create(&self, info: &RgbaChannelsInfo) -> Self::Pixels;
}

impl<P, F> SetRgbaPixel<P> for F where F: Fn(&mut P, Vec2<usize>, RgbaPixel) {
    fn set_pixel(&self, pixels: &mut P, position: Vec2<usize>, pixel: RgbaPixel) { self(pixels, position, pixel) }
}

impl<F, P> CreateRgbaPixels for F where F: Fn(&RgbaChannelsInfo) -> P {
    type Pixels = P;
    fn create(&self, info: &RgbaChannelsInfo) -> Self::Pixels { self(info) }
}



#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RgbaChannelsReader<'s, Set, Image> {
    storage: Image,
    set_pixel: &'s Set,
    channel_indices: (usize, usize, usize, Option<usize>),
    info: RgbaChannelsInfo
}


/// A summary of the channels of a given layer.
/// Does not contain any actual pixel data.
///
/// Any given pixel values will be automatically converted to the type found in `Image.channels`.
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub struct RgbaChannelsInfo { // TODO remove this struct?

    /// The channel types of the written file.
    ///
    /// Careful: Not all applications may support
    /// RGBA images with arbitrary sample types.
    pub channels: RgbaSampleTypes,

    /// The dimensions of this image, width and height.
    pub resolution: Vec2<usize>,

    /*/// The attributes of the exr image.
    pub image_attributes: ImageAttributes,

    /// The attributes of the exr layer.
    pub layer_attributes: LayerAttributes,

    /// Specifies how the pixel data is formatted inside the file,
    /// for example, compression and tiling.
    pub encoding: Encoding,*/
}


/*/// Specifies how the pixel data is formatted inside the file.
/// Does not affect any visual aspect, like positioning or orientation.
// TODO alsop nest encoding like this for meta::Header and simple::Image or even reuse this in image::simple
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct RgbaEncoding {

    /// What type of compression the pixel data in the file is compressed with.
    pub compression: Compression,

    /// If this is some pair of numbers, the image is divided into tiles of that size.
    /// If this is none, the image is divided into scan line blocks, depending on the compression method.
    pub tile_size: Option<Vec2<usize>>,

    /// In what order the tiles of this header occur in the file.
    /// Does not change any actual image orientation.
    pub line_order: LineOrder,
}*/


impl<S> ContainsNaN for RgbaChannels<S> where S: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.storage.contains_nan_pixels()
    }
}

/*impl RgbaEncoding {

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
}*/


impl RgbaChannelsInfo {

    /*/// Create an Image with an alpha channel.
    /// All channels will have the specified sample type.
    /// Data is automatically converted to that type.
    /// Use `RgbaInfo::new` where each channel should have a different sample type.
    pub fn rgba(resolution: impl Into<Vec2<usize>>, sample_type: SampleType) -> Self {
        Self::new(resolution, (sample_type, sample_type, sample_type, Some(sample_type)))
    }

    /// Create an Image without an alpha channel.
    /// All channels will have the specified sample type.
    /// Data is automatically converted to that type.
    /// Use `RgbaInfo::new` where each channel should have a different sample type.
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
        self.layer_attributes.layer_position = position;
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
    }*/

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
}


// TODO what about subsampling?

// Reminder: This is implemented for references, because it needs to borrow
// and this is the simplest way to specify the lifetime without requiring a lifetime inside the trait
impl<'s, Setter: 's, Constructor: 's>
    ReadChannels<'s> for ReadRgbaChannels<Constructor, Setter>
where
    Constructor: CreateRgbaPixels,
    Setter: SetRgbaPixel<Constructor::Pixels>
{
    type Reader = RgbaChannelsReader<'s, Setter, Constructor::Pixels>;

    fn create_channels_reader(&'s self, header: &Header) -> Result<Self::Reader> {
        if header.deep { return Err(Error::invalid("layer has deep data, no flat RGB data")) }

        let mut rgba_types  = [None; 4];
        for (channel_index, channel) in header.channels.list.iter().enumerate() {
            let channel_type = Some((channel_index, channel.sample_type));

            if      channel.name.eq_case_insensitive("a") { rgba_types[3] = channel_type; }
            else if channel.name.eq_case_insensitive("b") { rgba_types[2] = channel_type; }
            else if channel.name.eq_case_insensitive("g") { rgba_types[1] = channel_type; }
            else if channel.name.eq_case_insensitive("r") { rgba_types[0] = channel_type; }
        }

        if let [Some(r), Some(g), Some(b), a] = rgba_types {
            let channel_indices = (r.0, g.0, b.0, a.map(|a| a.0));
            let channels = RgbaSampleTypes(r.1, g.1, b.1, a.map(|a| a.1));
            let info = RgbaChannelsInfo { channels, resolution: header.layer_size, };

            Ok(RgbaChannelsReader {
                storage: self.create.create(&info),
                set_pixel: &self.set_pixel,
                channel_indices,
                info
            })
        }

        else {
            Err(Error::invalid("layer has no RGB channels"))
        }
    }
}

impl<Setter, Storage>
    ChannelsReader for RgbaChannelsReader<'_, Setter, Storage>
    where Setter: SetRgbaPixel<Storage>
{
    type Channels = RgbaChannels<Storage>;

    fn read_block(&mut self, header: &Header, block: UncompressedBlock) -> UnitResult {
        let RgbaSampleTypes(r_type, g_type, b_type, a_type) = self.info.channels;
        let line_bytes = block.index.pixel_size.0 * header.channels.bytes_per_pixel;

        // TODO compute this once per image, not per block
        let (mut r_range, mut g_range, mut b_range, mut a_range) = (0..0, 0..0, 0..0, 0..0);
        let mut byte_index = 0;

        for (channel_index, channel) in header.channels.list.iter().enumerate() {
            let sample_bytes = channel.sample_type.bytes_per_sample();
            let channel_bytes = block.index.pixel_size.0 * sample_bytes;
            let byte_range = byte_index .. byte_index + channel_bytes;
            byte_index = byte_range.end;

            if      Some(channel_index) == self.channel_indices.3 { a_range = byte_range }
            else if channel_index == self.channel_indices.2 { b_range = byte_range }
            else if channel_index == self.channel_indices.1 { g_range = byte_range }
            else if channel_index == self.channel_indices.0 { r_range = byte_range }
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
                let pixel = RgbaPixel::new(
                    next_r()?, next_g()?, next_b()?,
                    if let Some(a) = &mut next_a { Some(a()?) } else { None }
                );

                let position = block.index.pixel_position + Vec2(x,y);
                self.set_pixel.set_pixel(&mut self.storage, position, pixel)
            }
        }

        Ok(())
    }

    // TODO levels?
    fn filter_block(&self, (_, tile): (usize, &TileCoordinates)) -> bool {
        tile.is_largest_resolution_level()
    }

    fn into_channels(self) -> Self::Channels {
        RgbaChannels {
            sample_types: self.info.channels,
            storage: self.storage
        }
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

    impl<T> ContainsNaN for Flattened<T> where T: ContainsNaN {
        fn contains_nan_pixels(&self) -> bool {
           self.samples.as_slice().contains_nan_pixels()
        }
    }

    impl<T> GetRgbaPixel for Flattened<T> where T: Sync + Copy + Into<Sample> {
        fn get_pixel(&self, position: Vec2<usize>) -> RgbaPixel {
            get_flattened_pixel(self, position)
        }
    }

    /// Constructor for a flattened f16 pixel storage.
    /// This function an directly be passed to `rgba::RgbaInfo::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<f16>` image.
    #[inline] pub fn create_flattened_f16(image: &RgbaChannelsInfo) -> Flattened<f16> {
        Flattened {
            width: image.resolution.0,
            channels: image.channel_count(),
            samples: vec![f16::ZERO; image.resolution.area() * image.channel_count()]
        }
    }

    /// Constructor for a flattened f32 pixel storage.
    /// This function an directly be passed to `rgba::RgbaInfo::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<f32>` image.
    #[inline] pub fn create_flattened_f32(image: &RgbaChannelsInfo) -> Flattened<f32> {
        Flattened {
            width: image.resolution.0,
            channels: image.channel_count(),
            samples: vec![0.0; image.resolution.area() * image.channel_count()]
        }
    }

    /// Constructor for a flattened u32 pixel storage.
    /// This function an directly be passed to `rgba::RgbaInfo::load_from_file` and friends.
    /// It will construct a `rgba::pixels::Flattened<u32>` image.
    #[inline] pub fn create_flattened_u32(image: &RgbaChannelsInfo) -> Flattened<u32> {
        Flattened {
            width: image.resolution.0,
            channels: image.channel_count(),
            samples: vec![0; image.resolution.area() * image.channel_count()]
        }
    }

    /// Create an object that can examine the pixels of a `Flattened<T>` image.
    #[inline]
    pub fn get_flattened_pixel<T>(image: &Flattened<T>, position: Vec2<usize>) -> RgbaPixel
        where T: Sync + Copy + Into<Sample>
    {
        let pixel = &image.samples[image.compute_pixel_index(position)];
        RgbaPixel::new(pixel[0], pixel[1], pixel[2], pixel.get(3).cloned())
    }

    /// Create an object that can update the pixels of a `Flattened<T>` image.
    #[inline]
    pub fn set_flattened_pixel<T> (image: &mut Flattened<T>, position: Vec2<usize>, pixel: RgbaPixel) where T: Copy + From<Sample> {
        let index = image.compute_pixel_index(position);
        let samples = &mut image.samples[index];

        samples[0] = pixel.red.into();
        samples[1] = pixel.green.into();
        samples[2] = pixel.blue.into();

        if samples.len() == 4 {
            samples[3] = pixel.alpha_or_default().into();
        }
    }


    use std::fmt::*;
    impl<T> Debug for Flattened<T> {
        #[inline] fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "[{}; {}]", std::any::type_name::<T>(), self.samples.len())
        }
    }
}


