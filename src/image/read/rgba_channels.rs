//! How to read rgba channels.
//! This is not a zero-cost abstraction.

use crate::image::*;
use crate::io::Read;
use crate::meta::header::{Header};
use crate::error::{Result, UnitResult, Error};
use crate::block::UncompressedBlock;
use crate::math::Vec2;
use crate::image::read::layers::{ChannelsReader, ReadChannels};
use crate::block::samples::Sample;
use crate::block::chunk::TileCoordinates;

/// Specify to load only rgb channels and how to store the result.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadRgbaChannels<CreatePixelStorage, SetPixel> {

    /// A function used to create one rgba pixel storage per layer
    pub create: CreatePixelStorage,

    /// A function used to write the rgba pixels from the file to your image storage
    pub set_pixel: SetPixel
}

/// Define how to store an rgba pixel in your custom pixel storage.
/// Can be a closure of type [`Fn(&RgbaChannelsInfo) -> YourPixelStorage`].
pub trait SetRgbaPixel<PixelStorage> {

    /// Will be called for all pixels in the file, resulting in a complete image.
    fn set_pixel(&self, pixels: &mut PixelStorage, position: Vec2<usize>, pixel: RgbaPixel);
}

/// Define how to create your custom pixel storage for a given layer.
/// Can be a closure of type [`Fn(&mut YourPixelStorage, Vec2<usize>, RgbaPixel)`].
pub trait CreateRgbaPixels {

    /// Your custom pixel storage.
    type Pixels;

    /// Called once per rgba layer.
    fn create(&self, info: &RgbaChannelsInfo) -> Self::Pixels;
}

impl<P, F> SetRgbaPixel<P> for F where F: Fn(&mut P, Vec2<usize>, RgbaPixel) {
    fn set_pixel(&self, pixels: &mut P, position: Vec2<usize>, pixel: RgbaPixel) { self(pixels, position, pixel) }
}

impl<F, P> CreateRgbaPixels for F where F: Fn(&RgbaChannelsInfo) -> P {
    type Pixels = P;
    fn create(&self, info: &RgbaChannelsInfo) -> Self::Pixels { self(info) }
}


/// Processes pixel blocks from a file and accumulates them into the rgba channels.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RgbaChannelsReader<'s, Set, Image> {
    storage: Image,
    set_pixel: &'s Set,
    channel_indices: (usize, usize, usize, Option<usize>),
    info: RgbaChannelsInfo
}


/// A summary of the channels of a given layer.
/// Does not contain any actual pixel data.
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub struct RgbaChannelsInfo { // TODO remove this struct?

    /// The type of each channel in the rgba or rgb image.
    pub channels: RgbaSampleTypes,

    /// The dimensions of this image, width and height.
    pub resolution: Vec2<usize>,
}



impl<S> ContainsNaN for RgbaChannels<S> where S: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.storage.contains_nan_pixels()
    }
}


impl RgbaSampleTypes {

    /// Is 4 if this is an rgba image, 3 for an rgb image.
    #[inline]
    pub fn count(&self) -> usize {
        if self.3.is_some() { 4 } else { 3 }
    }

    /// Return the red green and blue channels as an indexable array. Does not include the alpha channel.
    #[inline]
    pub fn color_types_as_array(&self) -> [SampleType; 3] {
        [self.0, self.1, self.2]
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
        if header.deep { return Err(Error::invalid("layer has deep data, no flat rgba data")) }

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
            println!("found channels {:#?}", header.channels);
            Err(Error::invalid("layer has no rgba channels"))
        }
    }
}

impl<Setter, Storage>
    ChannelsReader for RgbaChannelsReader<'_, Setter, Storage>
    where Setter: SetRgbaPixel<Storage>
{
    type Channels = RgbaChannels<Storage>;

    // TODO levels?
    fn filter_block(&self, (_, tile): (usize, &TileCoordinates)) -> bool {
        tile.is_largest_resolution_level()
    }

    fn read_block(&mut self, header: &Header, block: UncompressedBlock) -> UnitResult {
        // TODO use decompressed.for_lines(header, &decompressed, |line| {   self.sample_channels_reader[line.location.channel].samples.read_line(line) })

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

    fn into_channels(self) -> Self::Channels {
        RgbaChannels {
            sample_types: self.info.channels,
            storage: self.storage
        }
    }
}



/// Provides a predefined pixel storage for rgba images.
/// Currently contains a homogeneous flattened vector storage.
pub mod pixels {
    use super::*;

    /// Store all samples in a single array.
    /// All samples will be converted to the type `T`.
    /// This supports all the sample types, `f16`, `f32`, and `u32`.
    ///
    /// The flattened vector contains all rows one after another.
    /// In each row, for each pixel, its red, green, blue, and then alpha
    /// samples are stored one after another.
    ///
    /// Use `Flattened::compute_pixel_index(image, position)`
    /// to compute the flat index of a specific pixel.
    #[derive(PartialEq, Clone)]
    pub struct Flattened<T> {


        /// The number of channels in this layer, either 3 or 4.
        pub channels: usize,

        /// The resolution of this layer.
        pub size: Vec2<usize>,

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
            let pixel_index = position.flat_index_for_size(self.size);
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
        type Pixel = RgbaPixel;
        fn get_pixel(&self, position: Vec2<usize>) -> RgbaPixel {
            get_flattened_pixel(self, position)
        }
    }

    /// Constructor for a flattened f16 pixel storage.
    /// Can usually be used as a reference instead of calling it manually.
    #[inline] pub fn create_flattened_f16(image: &RgbaChannelsInfo) -> Flattened<f16> {
        Flattened {
            size: image.resolution,
            channels: image.channels.count(),
            samples: vec![f16::ZERO; image.resolution.area() * image.channels.count()]
        }
    }

    /// Constructor for a flattened f32 pixel storage.
    /// Can usually be used as a reference instead of calling it manually.
    #[inline] pub fn create_flattened_f32(image: &RgbaChannelsInfo) -> Flattened<f32> {
        Flattened {
            size: image.resolution,
            channels: image.channels.count(),
            samples: vec![0.0; image.resolution.area() * image.channels.count()]
        }
    }

    /// Constructor for a flattened u32 pixel storage.
    /// Can usually be used as a reference instead of calling it manually.
    #[inline] pub fn create_flattened_u32(image: &RgbaChannelsInfo) -> Flattened<u32> {
        Flattened {
            size: image.resolution,
            channels: image.channels.count(),
            samples: vec![0; image.resolution.area() * image.channels.count()]
        }
    }

    /// Examine a pixel of a `Flattened<T>` image.
    /// Can usually be used as a reference instead of calling it manually.
    #[inline]
    pub fn get_flattened_pixel<T>(image: &Flattened<T>, position: Vec2<usize>) -> RgbaPixel
        where T: Sync + Copy + Into<Sample>
    {
        let pixel = &image.samples[image.compute_pixel_index(position)];
        RgbaPixel::new(pixel[0], pixel[1], pixel[2], pixel.get(3).cloned())
    }

    /// Update a pixel of a `Flattened<T>` image.
    /// Can usually be used as a reference instead of calling it manually.
    #[inline]
    pub fn set_flattened_pixel<T> (image: &mut Flattened<T>, position: Vec2<usize>, pixel: RgbaPixel) where T: Copy + From<Sample> {
        let index = image.compute_pixel_index(position);
        let samples = &mut image.samples[index];

        samples[0] = pixel.red.into();
        samples[1] = pixel.green.into();
        samples[2] = pixel.blue.into();

        if samples.len() == 4 {
            samples[3] = pixel.alpha_or_1().into();
        }
    }

    use std::fmt::*;
    impl<T> Debug for Flattened<T> {
        #[inline] fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "[{}; {}]", std::any::type_name::<T>(), self.samples.len())
        }
    }
}


