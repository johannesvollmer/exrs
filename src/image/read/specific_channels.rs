//! How to read arbitrary but specific selection of arbitrary channels.
//! This is not a zero-cost abstraction.
// this module uses too many traits in order to abstract over many possible tuples of channels
// TODO this module can be simplified A LOT by using SmallVec<Sample> objects, which is anyways how it works,
// TODO as the internal sample type always differs from the user-specified concrete type


use crate::image::*;
use crate::meta::header::{Header};
use crate::error::{Result, UnitResult, Error};
use crate::block::UncompressedBlock;
use crate::math::Vec2;
use crate::image::read::layers::{ChannelsReader, ReadChannels};
use crate::block::samples::Sample;
use crate::block::chunk::TileCoordinates;
use std::marker::PhantomData;


/// Specify to load only specific channels and how to store the result.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadSpecificChannels<Px, Channels, CreatePixelStorage, SetPixel> {

    /// A tuple containing `String` or `&str` elements.
    /// Each tuple element queries the file for a channel with that name.
    pub channel_names: Channels, // impl ReadChannelNames

    /// Creates a new pixel storage per layer
    pub create: CreatePixelStorage,

    /// Writes the pixels from the file to your image storage
    pub set_pixel: SetPixel,

    // TODO private
    /// Required to avoid `unconstrained type parameter` problems.
    pub px: PhantomData<Px>,
}
// TODO rename all "info" to "description" or something else
// pub type RgbaChannelsInfo = ChannelsInfo<RgbaSampleTypes>; // TODO rename to specific_channels_layout or description, global search for "info"

pub trait ReadFilteredChannels<Pixel> {
    type Filter: ChannelsFilter<Pixel>;
    fn filter(&self) -> Self::Filter;
}

pub trait ChannelsFilter<Pixel> {
    type PixelReader: PixelReader<Pixel>;
    type ChannelsInfo;

    fn visit_channel(&mut self, channel: ChannelIndexInfo);

    /// allow err so that we can abort if a required channel does not exist in the file
    fn finish(self) -> Result<(Self::ChannelsInfo, Self::PixelReader)>;
}



/// Define how to store a pixel in your custom pixel storage.
/// Can be a closure of type [`Fn(&mut YourPixelStorage, Vec2<usize>, YourPixel)`].
/// The Pixel type should be a tuple containing any combination of `f32`, `f16`, or `u32` values.
pub trait SetPixel<PixelStorage, Pixel> {

    /// Will be called for all pixels in the file, resulting in a complete image.
    fn set_pixel(&self, pixels: &mut PixelStorage, position: Vec2<usize>, pixel: Pixel); // TODO impl From<RgbaPixel>?
}

/// Define how to create your custom pixel storage for a given layer.
/// Can be a closure of type [`Fn(&RgbaChannelsInfo) -> YourPixelStorage`].
pub trait CreatePixels<SampleTypes> {

    /// Your custom pixel storage.
    type Pixels;

    /// Called once per rgba layer.
    fn create(&self, info: &ChannelsInfo<SampleTypes>) -> Self::Pixels;
}

impl<Pxs, Px, F> SetPixel<Pxs, Px> for F where F: Fn(&mut Pxs, Vec2<usize>, Px) {
    fn set_pixel(&self, pixels: &mut Pxs, position: Vec2<usize>, pixel: Px) { self(pixels, position, pixel) }
}

impl<F, P, T> CreatePixels<T> for F where F: Fn(&ChannelsInfo<T>) -> P {
    type Pixels = P;
    fn create(&self, info: &ChannelsInfo<T>) -> Self::Pixels { self(info) }
}


/*pub trait Pixel<ChannelNames> {
    type SampleTypes;

    type PixelReader: PixelReader<Self>;
    fn inspect_channels(desired: &ChannelNames, existing: &ChannelList) -> Result<(Self::SampleTypes, Self::PixelReader)>;
}*/

pub trait PixelReader<Pixel> {
    type LineReader: Clone + PixelLineReader<Pixel>;
    fn create_pixel_reader_for_line(&self, pixel_count: usize) -> Self::LineReader;
}

pub trait PixelLineReader<Pixel> {
    fn read_next_pixel(&mut self, bytes: &[u8]) -> Result<Pixel>;
}

/// Processes pixel blocks from a file and accumulates them into the specific channels pixel storage.
pub struct SpecificChannelsReader<'s, Pixel, Channels, Set, Image> where Channels: ReadFilteredChannels<Pixel> {
    storage: Image,
    set_pixel: &'s Set,
    info: ChannelsInfo<<Channels::Filter as ChannelsFilter<Pixel>>::ChannelsInfo>,
    pixel_reader: <Channels::Filter as ChannelsFilter<Pixel>>::PixelReader,
    pixel: PhantomData<(Pixel, Channels)>,
}


/// A summary of the channels of a given layer.
/// Does not contain any actual pixel data.
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub struct ChannelsInfo<SampleTypes> { // TODO remove this struct?

    /// The actual type of each channel in the file.
    /// Will be converted from and to the runtime type you specify.
    pub sample_types: SampleTypes,

    /// The dimensions of this image, width and height.
    pub resolution: Vec2<usize>,
}



// TODO what about subsampling?

impl<'s, Px, Channels, Setter: 's, Constructor: 's>
    ReadChannels<'s> for ReadSpecificChannels<Px, Channels, Constructor, Setter> where
    Channels: ReadFilteredChannels<Px>,
    Constructor: CreatePixels<<Channels::Filter as ChannelsFilter<Px>>::ChannelsInfo>,
    Setter: SetPixel<Constructor::Pixels, Px>,
{
    type Reader = SpecificChannelsReader<'s, Px, Channels, Setter, Constructor::Pixels>;

    fn create_channels_reader(&'s self, header: &Header) -> Result<Self::Reader> {
        if header.deep { return Err(Error::invalid("`SpecificChannels` does not support deep data")) }

        let (sample_types, reader) = {
            let mut filter = self.channel_names.filter();
            let mut byte_offset = 0;

            for (channel_index, channel) in header.channels.list.iter().enumerate() {
                let chan_info = ChannelIndexInfo {
                    sample_byte_offset: byte_offset,
                    info: channel.clone(),
                    channel_index
                };

                filter.visit_channel(chan_info);
                byte_offset += channel.sample_type.bytes_per_sample();
            }

            filter.finish()?
        };

        // let (sample_types, reader) = self.channel_names.inspect_channels(&header.channels)?;
        let info = ChannelsInfo { sample_types, resolution: header.layer_size, };

        Ok(SpecificChannelsReader {
            set_pixel: &self.set_pixel,
            storage: self.create.create(&info),
            pixel_reader: reader,
            pixel: Default::default(),
            info
        })
    }
}

/*
impl<'s, Setter: 's, Constructor: 's>
ReadChannels<'s> for ReadSpecificChannels<Constructor, Setter> where
    Constructor: CreatePixels<RgbaSampleTypes>,
    Setter: SetPixel<Constructor::Pixels, RgbaPixel>
{
    type Reader = SpecificChannelsReader<'s, RgbaPx, RgbaFilterChannels, Setter, Constructor::Pixels>;

    fn create_channels_reader(&'s self, header: &Header) -> Result<Self::Reader> {
        if header.deep { return Err(Error::invalid("layer has deep data, no flat rgba data")) }

        let (sample_types, reader) = self.channel_names.inspect_channels(&header.channels)?;
        let info = ChannelsInfo { sample_types, resolution: header.layer_size, };

        Ok(SpecificChannelsReader {
            set_pixel: &self.set_pixel,
            storage: self.create.create(&info),
            pixel_reader: reader,
            pixel: Default::default(),
            info
        })
    }
}
*/

impl<Px, ChannelNames, Setter, Storage>
    ChannelsReader for SpecificChannelsReader<'_, Px, ChannelNames, Setter, Storage>
where
    ChannelNames: ReadFilteredChannels<Px>,
    Setter: SetPixel<Storage, Px>,
{
    type Channels = SpecificChannels<Storage, <ChannelNames::Filter as ChannelsFilter<Px>>::ChannelsInfo>;

    // TODO levels?
    fn filter_block(&self, (_, tile): (usize, &TileCoordinates)) -> bool {
        tile.is_largest_resolution_level()
    }

    fn read_block(&mut self, header: &Header, block: UncompressedBlock) -> UnitResult {
        if header.channels.bytes_per_pixel * block.index.pixel_size.area() != block.data.len() {
            return Err(Error::invalid("block size for header"))
        }

        let pixels_per_line = block.index.pixel_size.width();
        let line_bytes = pixels_per_line * header.channels.bytes_per_pixel;
        let byte_lines = block.data.chunks_exact(line_bytes);
        assert_eq!(byte_lines.len(), block.index.pixel_size.height(), "invalid byte count for pixel block height");

        let initial_pixel_line_reader = self.pixel_reader
            .create_pixel_reader_for_line(pixels_per_line);

        for (y, byte_line) in byte_lines.enumerate() {
            let mut line_reader = initial_pixel_line_reader.clone();

            for x in 0..block.index.pixel_size.0 {
                let pixel = line_reader.read_next_pixel(byte_line)?;
                let position = block.index.pixel_position + Vec2(x,y);
                self.set_pixel.set_pixel(&mut self.storage, position, pixel);
            }
        }

        Ok(())
    }

    fn into_channels(self) -> Self::Channels {
        SpecificChannels {
            channels: self.info.sample_types,
            storage: self.storage
        }
    }
}



#[derive(Clone, Debug)]
pub struct ChannelIndexInfo {
    pub info: ChannelInfo,
    pub sample_byte_offset: usize,
    pub channel_index: usize,
}

pub trait ChannelParameter: Clone {
    type ChannelInfo;
    type ChannelPixelReader: ChannelPixelReader<Self>;
    fn create_channel_pixel_reader(info: Option<ChannelIndexInfo>) -> Result<(Self::ChannelInfo, Self::ChannelPixelReader)>;
}

pub trait ChannelPixelReader<Sample>: Clone {
    type ChannelLineReader: ChannelLineReader<Sample>;
    fn create_channel_line_reader(&self, line_width: usize) -> Self::ChannelLineReader;
}

pub trait ChannelLineReader<Sample>: Clone {
    fn read_next_sample(&mut self, bytes: &[u8]) -> Result<Sample>;
}

pub trait FromSample { fn from_sample(sample: Sample) -> Self; }
impl FromSample for f16 { fn from_sample(sample: Sample) -> Self { sample.to_f16() } }
impl FromSample for f32 { fn from_sample(sample: Sample) -> Self { sample.to_f32() } }
impl FromSample for u32 { fn from_sample(sample: Sample) -> Self { sample.to_u32() } }
impl FromSample for Sample { fn from_sample(sample: Sample) -> Self { sample } }

impl<S> ChannelParameter for S where S: Clone + FromSample {
    type ChannelInfo = ChannelInfo;
    type ChannelPixelReader = ChannelIndexInfo;
    fn create_channel_pixel_reader(info: Option<ChannelIndexInfo>) -> Result<(Self::ChannelInfo, Self::ChannelPixelReader)> {
        info.map(|info| (info.info.clone(), info))
            .ok_or_else(|| Error::invalid("layer does not contain all of the specified required channels")) // TODO which channel??
    }
}

impl<S> ChannelParameter for Option<S> where S: Clone + FromSample {
    type ChannelInfo = Option<ChannelInfo>;
    type ChannelPixelReader = Option<ChannelIndexInfo>;
    fn create_channel_pixel_reader(info: Option<ChannelIndexInfo>) -> Result<(Self::ChannelInfo, Self::ChannelPixelReader)> {
        Ok(info.map_or((None,None), |info| (Some(info.info.clone()), Some(info)))) // TODO no clone
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexChannelLineReader<S> {
    file_sample_type: SampleType,
    next_byte: usize,
    s: PhantomData<S>,
}

impl<S> ChannelPixelReader<S> for ChannelIndexInfo where S: Clone + FromSample {
// impl ChannelPixelReader<Sample> for ChannelIndexInfo {
    type ChannelLineReader = IndexChannelLineReader<S>;
    fn create_channel_line_reader(&self, pixel_count: usize) -> Self::ChannelLineReader {
        let start = self.sample_byte_offset * pixel_count; // TODO  will never work with subsampling?
        IndexChannelLineReader { file_sample_type: self.info.sample_type, next_byte: start, s: Default::default() }
    }
}


impl<S> ChannelPixelReader<Option<S>> for Option<ChannelIndexInfo> where S: Clone + FromSample {
// impl ChannelPixelReader<Option<Sample>> for Option<ChannelIndexInfo> {
    type ChannelLineReader = Option<IndexChannelLineReader<S>>;
    fn create_channel_line_reader(&self, line_width: usize) -> Self::ChannelLineReader {
        self.as_ref().map(|this| {
            this.create_channel_line_reader(line_width)
        })
    }
}

impl<S> ChannelLineReader<S> for IndexChannelLineReader<S> where S: FromSample + Clone {
    fn read_next_sample(&mut self, bytes: &[u8]) -> Result<S> {
        let bytes = &mut &bytes[(self.next_byte).min(bytes.len())..]; // required for index out of bounds overflow

        // TODO not match as much?

        self.next_byte += self.file_sample_type.bytes_per_sample();

        let file_sample = match self.file_sample_type {
            SampleType::F16 => Sample::F16(f16::read(bytes)?),
            SampleType::F32 => Sample::F32(f32::read(bytes)?),
            SampleType::U32 => Sample::U32(u32::read(bytes)?),
        };

        Ok(S::from_sample(file_sample))
    }
}

impl<S> ChannelLineReader<Option<S>> for Option<IndexChannelLineReader<S>> where S: FromSample + Clone {
    fn read_next_sample(&mut self, bytes: &[u8]) -> Result<Option<S>> {
        self.as_mut()
            .map(|this| this.read_next_sample(bytes))
            .transpose()
    }
}

/*macro_rules! impl_read_for_tuple {
    (
        $( $ChannelTypes:ident ),* |
        $( $ChannelNames:ident ),* |
        $( $index:literal ),* |
        $( $var_name:ident ),*
    )
        =>
    {

        impl<  $($ChannelNames),*  ,  $($ChannelTypes),*   > // <Na,Nb,Nc, A,B,C>
            ReadFilteredChannels<(  $($ChannelTypes),*  )>
            for (  $($ChannelNames),*  )
            where
            A: ChannelParameter, B: ChannelParameter, C: ChannelParameter,
            Na: AsRef<str>, Nb: AsRef<str>, Nc: AsRef<str>,
        {
            type PixelReader = (A::ChannelPixelReader, B::ChannelPixelReader, C::ChannelPixelReader);
            type SampleTypes = (A::SampleType, B::SampleType, C::SampleType);

            fn inspect_channels(&self, channels: &ChannelList) -> Result<(Self::SampleTypes, Self::PixelReader)> {
                let mut result = (None, None, None);
                let mut byte_offset = 0;

                for (channel_index, channel) in channels.list.iter().enumerate() {
                    let chan_info = ChannelIndexInfo {
                        sample_byte_offset: byte_offset,
                        info: channel.clone(),
                        channel_index
                    };

                    if      &channel.name == self.0.as_ref() { result.0 = Some(chan_info); }
                    else if &channel.name == self.1.as_ref() { result.1 = Some(chan_info); }
                    else if &channel.name == self.2.as_ref() { result.2 = Some(chan_info); }

                    byte_offset += channel.sample_type.bytes_per_sample();
                }

                let (a_type, a_reader) = A::create_channel_pixel_reader(result.0)?;
                let (b_type, b_reader) = B::create_channel_pixel_reader(result.1)?;
                let (c_type, c_reader) = C::create_channel_pixel_reader(result.2)?;

                Ok((
                    (a_type, b_type, c_type),
                    (a_reader, b_reader, c_reader)
                ))
            }
        }


        impl<A,B,C> PixelReader<(A,B,C)> for (
            <A as ChannelParameter>::ChannelPixelReader,
            <B as ChannelParameter>::ChannelPixelReader,
            <C as ChannelParameter>::ChannelPixelReader,
        )
            where A: ChannelParameter, B: ChannelParameter, C: ChannelParameter,
            // (A::ChannelLineReader, B::ChannelLineReader, C::ChannelLineReader): PixelLineReader<(A,B,C)>,
        {
            type LineReader = (
                <<A as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<A>>::ChannelLineReader,
                <<B as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<B>>::ChannelLineReader,
                <<C as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<C>>::ChannelLineReader,
            );

            fn create_pixel_reader_for_line(&self, pixel_count: usize) -> Self::LineReader {
                (
                    self.0.create_channel_line_reader(pixel_count),
                    self.1.create_channel_line_reader(pixel_count),
                    self.2.create_channel_line_reader(pixel_count),
                )
            }
        }

        impl<A,B,C> PixelLineReader<(A,B,C)> for (
            <<A as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<A>>::ChannelLineReader,
            <<B as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<B>>::ChannelLineReader,
            <<C as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<C>>::ChannelLineReader,
        )
            where A: ChannelParameter, B: ChannelParameter, C: ChannelParameter,
        {
            // TODO not index each time?
            fn read_next_pixel(&mut self, bytes: &[u8]) -> Result<(A,B,C)> {
                Ok((
                    self.0.read_next_sample(bytes)?,
                    self.1.read_next_sample(bytes)?,
                    self.2.read_next_sample(bytes)?,
                ))
            }
        }
    };
}*/


// impl_read_for_tuple!{ A,B,C | Na, Nb, Nc | 0,1,2 | a,b,c }

pub struct ChannelFilter<N> {
    required_name: N,
    found_channel: Option<ChannelIndexInfo>,
}
impl<N> ChannelFilter<N> where N: AsRef<str> {
    pub fn new(name:N) -> Self { Self { required_name: name, found_channel: None }  }
    pub fn filter(&mut self, info: &ChannelIndexInfo) {
        if &info.info.name == self.required_name.as_ref() { self.found_channel = Some(info.clone()); }
    }
}

impl<A,B,C,D, L,M,N,O> ReadFilteredChannels<(A,B,C,D)> for (L,M,N,O) where
    A: ChannelParameter, B: ChannelParameter, C: ChannelParameter, D: ChannelParameter,
    L: Clone + AsRef<str>, M: Clone + AsRef<str>, N: Clone + AsRef<str>, O: Clone + AsRef<str>,
{
    type Filter = (ChannelFilter<L>, ChannelFilter<M>, ChannelFilter<N>,  ChannelFilter<O>, );
    fn filter(&self) -> Self::Filter { (
        ChannelFilter::new(self.0.clone()),
        ChannelFilter::new(self.1.clone()),
        ChannelFilter::new(self.2.clone()),
        ChannelFilter::new(self.3.clone()),
    ) }
}

impl<A,B,C,D, L,M,N,O> ChannelsFilter<(A,B,C,D)> for (ChannelFilter<L>, ChannelFilter<M>, ChannelFilter<N>, ChannelFilter<O>)
    where
        A: ChannelParameter, B: ChannelParameter, C: ChannelParameter, D: ChannelParameter,
        L: Clone + AsRef<str>, M: Clone + AsRef<str>, N: Clone + AsRef<str>, O: Clone + AsRef<str>,
{
    type PixelReader = (A::ChannelPixelReader, B::ChannelPixelReader, C::ChannelPixelReader, D::ChannelPixelReader);
    type ChannelsInfo = (A::ChannelInfo, B::ChannelInfo, C::ChannelInfo, D::ChannelInfo);

    fn visit_channel(&mut self, channel: ChannelIndexInfo) {
        self.0.filter(&channel);
        self.1.filter(&channel);
        self.2.filter(&channel);
        self.3.filter(&channel);
    }

    fn finish(self) -> Result<(Self::ChannelsInfo, Self::PixelReader)> {
        let (a_type, a_reader) = A::create_channel_pixel_reader(self.0.found_channel)?;
        let (b_type, b_reader) = B::create_channel_pixel_reader(self.1.found_channel)?;
        let (c_type, c_reader) = C::create_channel_pixel_reader(self.2.found_channel)?;
        let (d_type, d_reader) = D::create_channel_pixel_reader(self.3.found_channel)?;

        Ok((
            (a_type, b_type, c_type, d_type),
            (a_reader, b_reader, c_reader, d_reader)
        ))
    }
}



/*impl<Na,Nb,Nc, A,B,C> ReadFilteredChannels<(A,B,C)> for (Na,Nb,Nc) where
    A: ChannelParameter, B: ChannelParameter, C: ChannelParameter,
    Na: AsRef<str>, Nb: AsRef<str>, Nc: AsRef<str>,
{
    type PixelReader = (A::ChannelPixelReader, B::ChannelPixelReader, C::ChannelPixelReader);
    type SampleTypes = (A::SampleType, B::SampleType, C::SampleType);

    fn inspect_channels(&self, channels: &ChannelList) -> Result<(Self::SampleTypes, Self::PixelReader)> {
        let mut result = (None, None, None);
        let mut byte_offset = 0;

        for (channel_index, channel) in channels.list.iter().enumerate() {
            let chan_info = ChannelIndexInfo {
                sample_byte_offset: byte_offset,
                info: channel.clone(),
                channel_index
            };

            if      &channel.name == self.0.as_ref() { result.0 = Some(chan_info); }
            else if &channel.name == self.1.as_ref() { result.1 = Some(chan_info); }
            else if &channel.name == self.2.as_ref() { result.2 = Some(chan_info); }

            byte_offset += channel.sample_type.bytes_per_sample();
        }

        let (a_type, a_reader) = A::create_channel_pixel_reader(result.0)?;
        let (b_type, b_reader) = B::create_channel_pixel_reader(result.1)?;
        let (c_type, c_reader) = C::create_channel_pixel_reader(result.2)?;

        Ok((
            (a_type, b_type, c_type),
            (a_reader, b_reader, c_reader)
        ))
    }
}*/


impl<A,B,C,D> PixelReader<(A,B,C,D)> for (
    <A as ChannelParameter>::ChannelPixelReader,
    <B as ChannelParameter>::ChannelPixelReader,
    <C as ChannelParameter>::ChannelPixelReader,
    <D as ChannelParameter>::ChannelPixelReader,
)
    where A: ChannelParameter, B: ChannelParameter, C: ChannelParameter, D: ChannelParameter,
    // (A::ChannelLineReader, B::ChannelLineReader, C::ChannelLineReader): PixelLineReader<(A,B,C)>,
{
    type LineReader = (
        <<A as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<A>>::ChannelLineReader,
        <<B as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<B>>::ChannelLineReader,
        <<C as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<C>>::ChannelLineReader,
        <<D as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<D>>::ChannelLineReader,
    );

    fn create_pixel_reader_for_line(&self, pixel_count: usize) -> Self::LineReader {
        (
            self.0.create_channel_line_reader(pixel_count),
            self.1.create_channel_line_reader(pixel_count),
            self.2.create_channel_line_reader(pixel_count),
            self.3.create_channel_line_reader(pixel_count),
        )
    }
}

impl<A,B,C,D> PixelLineReader<(A,B,C,D)> for (
    <<A as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<A>>::ChannelLineReader,
    <<B as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<B>>::ChannelLineReader,
    <<C as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<C>>::ChannelLineReader,
    <<D as ChannelParameter>::ChannelPixelReader as ChannelPixelReader<D>>::ChannelLineReader,
)
    where A: ChannelParameter, B: ChannelParameter, C: ChannelParameter, D: ChannelParameter,
{
    // TODO not index each time?
    fn read_next_pixel(&mut self, bytes: &[u8]) -> Result<(A,B,C,D)> {
        Ok((
            self.0.read_next_sample(bytes)?,
            self.1.read_next_sample(bytes)?,
            self.2.read_next_sample(bytes)?,
            self.3.read_next_sample(bytes)?,
        ))
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
    #[derive(Eq, PartialEq, Clone)]
    pub struct Flattened<T> {

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

        /// Create a new flattened pixel storage, checking the length of the provided samples vector.
        pub fn new(resolution: impl Into<Vec2<usize>>, samples: Vec<T>) -> Self {
            let size = resolution.into();
            assert_eq!(size.area(), samples.len(), "expected {} samples, but vector length is {}", size.area(), samples.len());
            Self { size, samples }
        }

        /// Compute the flat index of a specific pixel. Returns a range of either 3 or 4 samples.
        /// The computed index can be used with `Flattened.samples[index]`.
        /// Panics for invalid sample coordinates.
        #[inline]
        pub fn compute_pixel_index(&self, position: Vec2<usize>) -> usize {
            position.flat_index_for_size(self.size)
        }
    }

    impl<T> ContainsNaN for Flattened<T> where T: ContainsNaN {
        fn contains_nan_pixels(&self) -> bool {
           self.samples.as_slice().contains_nan_pixels()
        }
    }

    /*impl<T> GetPixel<T> for Flattened<T> where T: Sync {
        type Pixel = RgbaPixel;
        fn get_pixel(&self, position: Vec2<usize>) -> RgbaPixel {
            get_flattened_pixel(self, position)
        }
    }*/

    impl<Px> GetPixel for Flattened<Px> where Px: Clone + Sync {
        type Pixel = Px;
        fn get_pixel(&self, position: Vec2<usize>) -> Self::Pixel {
            get_flattened_pixel(self, position).clone()
        }
    }

    #[inline] pub fn create_flattened<Pixel: Clone + Default, SampleTypes>(image: &ChannelsInfo<SampleTypes>) -> Flattened<Pixel> {
        Flattened { size: image.resolution, samples: vec![Pixel::default(); image.resolution.area()] }
    }

    /// Examine a pixel of a `Flattened<T>` image.
    /// Can usually be used as a function reference instead of calling it manually.
    #[inline]
    pub fn get_flattened_pixel<Pixel>(image: &Flattened<Pixel>, position: Vec2<usize>) -> &Pixel where Pixel: Sync {
        &image.samples[image.compute_pixel_index(position)]
    }

    /// Update a pixel of a `Flattened<T>` image.
    /// Can usually be used as a function reference instead of calling it manually.
    #[inline]
    pub fn set_flattened_pixel<Pixel>(image: &mut Flattened<Pixel>, position: Vec2<usize>, pixel: Pixel) {
        let index = image.compute_pixel_index(position);
        image.samples[index] = pixel;
    }

    use std::fmt::*;
    impl<T> Debug for Flattened<T> {
        #[inline] fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
            write!(formatter, "[{}; {}]", std::any::type_name::<T>(), self.samples.len())
        }
    }
}


