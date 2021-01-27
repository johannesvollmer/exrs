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

// TODO rename all "info" to "description" or something else

/// Specify to load only specific channels and how to store the result.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadSpecificChannels<Px, CreatePixelStorage, SetPixel> where Px: DesiredPixel {

    /// A tuple containing `exr::Text` elements.
    /// Each tuple element queries the file for a channel with that name.
    pub channel_names: Px::ChannelNames,

    /// Creates a new pixel storage per layer
    pub create: CreatePixelStorage,

    /// Writes the pixels from the file to your image storage
    pub set_pixel: SetPixel,

    // TODO private
    /// Required to avoid `unconstrained type parameter` problems.
    pub px: PhantomData<Px>,
}


// TODO merge into `DesiredPixel` trait?
pub trait ReadFilteredChannels<Pixel, ChannelsInfo> { // TODO ChannelsInfo = Pixel::ChannelsInfo
    type Filter: ChannelsFilter<Pixel, ChannelsInfo>;
    fn filter(&self) -> Self::Filter;
}

pub trait ChannelsFilter<Pixel, ChannelsInfo> {
    type PixelReader: PixelReader<Pixel>;

    fn visit_channel(&mut self, channel: ChannelIndexInfo);

    /// allow err so that we can abort if a required channel does not exist in the file
    fn finish(self) -> Result<(ChannelsInfo, Self::PixelReader)>;
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


pub trait DesiredPixel: Sized {
    type ChannelNames: ReadFilteredChannels<Self, Self::ChannelsInfo>;
    type ChannelsInfo;
}


pub trait DesiredSample: Sized {
    type ChannelInfo;
    type SampleReader: ChannelPixelReader<Self>;
    fn create_channel_pixel_reader(info: Option<ChannelIndexInfo>) -> Result<(Self::ChannelInfo, Self::SampleReader)>;
}



/// Processes pixel blocks from a file and accumulates them into the specific channels pixel storage.
pub struct SpecificChannelsReader<'s, Pixel, Set, Image> where Pixel: DesiredPixel {
    storage: Image,
    set_pixel: &'s Set,
    info: ChannelsInfo<Pixel::ChannelsInfo>,
    pixel_reader: <<Pixel::ChannelNames as ReadFilteredChannels<Pixel, Pixel::ChannelsInfo>>::Filter as ChannelsFilter<Pixel, Pixel::ChannelsInfo>>::PixelReader,
    pixel: PhantomData<Pixel>,
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



pub trait PixelReader<Pixel> {
    type LineReader: Clone + PixelLineReader<Pixel>;
    fn create_pixel_reader_for_line(&self, pixel_count: usize) -> Self::LineReader;
}

pub trait PixelLineReader<Pixel> {
    fn read_next_pixel(&mut self, bytes: &[u8]) -> Result<Pixel>;
}

// TODO what about subsampling?

impl<'s, Px, Setter: 's, Constructor: 's>
    ReadChannels<'s> for ReadSpecificChannels<Px, Constructor, Setter> where
    Px: DesiredPixel,
    Constructor: CreatePixels<Px::ChannelsInfo>,
    Setter: SetPixel<Constructor::Pixels, Px>,
{
    type Reader = SpecificChannelsReader<'s, Px, Setter, Constructor::Pixels>;

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


impl<Px, Setter, Storage>
    ChannelsReader for SpecificChannelsReader<'_, Px, Setter, Storage>
where
    Px: DesiredPixel,
    Setter: SetPixel<Storage, Px>,
{
    type Channels = SpecificChannels<Storage, Px::ChannelsInfo>;

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

pub trait ChannelPixelReader<Sample>: Sized + Clone {
    type ChannelLineReader: ChannelLineReader<Sample>;
    fn create_channel_line_reader(&self, line_width: usize) -> Self::ChannelLineReader;
}

pub trait ChannelLineReader<Sample>: Sized + Clone {
    fn read_next_sample(&mut self, bytes: &[u8]) -> Result<Sample>;
}

pub trait FromSample: Copy + Clone { fn from_sample(sample: Sample) -> Self; }
impl FromSample for f16 { fn from_sample(sample: Sample) -> Self { sample.to_f16() } }
impl FromSample for f32 { fn from_sample(sample: Sample) -> Self { sample.to_f32() } }
impl FromSample for u32 { fn from_sample(sample: Sample) -> Self { sample.to_u32() } }
impl FromSample for Sample { fn from_sample(sample: Sample) -> Self { sample } }

impl<S> DesiredSample for S where S: FromSample {
    type ChannelInfo = ChannelInfo;
    type SampleReader = ChannelIndexInfo;
    fn create_channel_pixel_reader(info: Option<ChannelIndexInfo>) -> Result<(Self::ChannelInfo, Self::SampleReader)> {
        info.map(|info| (info.info.clone(), info))
            .ok_or_else(|| Error::invalid("layer does not contain all of the specified required channels")) // TODO which channel??
    }
}

impl<S> DesiredSample for Option<S> where S: FromSample {
    type ChannelInfo = Option<ChannelInfo>;
    type SampleReader = Option<ChannelIndexInfo>;
    fn create_channel_pixel_reader(info: Option<ChannelIndexInfo>) -> Result<(Self::ChannelInfo, Self::SampleReader)> {
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
    type ChannelLineReader = IndexChannelLineReader<S>;
    fn create_channel_line_reader(&self, pixel_count: usize) -> Self::ChannelLineReader {
        let start = self.sample_byte_offset * pixel_count; // TODO  will never work with subsampling?
        IndexChannelLineReader { file_sample_type: self.info.sample_type, next_byte: start, s: Default::default() }
    }
}


impl<S> ChannelPixelReader<Option<S>> for Option<ChannelIndexInfo> where S: Clone + FromSample {
    type ChannelLineReader = Option<IndexChannelLineReader<S>>;
    fn create_channel_line_reader(&self, line_width: usize) -> Self::ChannelLineReader {
        self.as_ref().map(|this| {
            this.create_channel_line_reader(line_width)
        })
    }
}

impl<S> ChannelLineReader<S> for IndexChannelLineReader<S> where S: FromSample + Clone {
    fn read_next_sample(&mut self, bytes: &[u8]) -> Result<S> {
        let bytes = &mut &bytes[(self.next_byte).min(bytes.len())..]; // required to prevent index out of bounds overflow

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



pub struct ChannelFilter {
    required_name: Text,
    found_channel: Option<ChannelIndexInfo>,
}
impl ChannelFilter {
    pub fn new(name: Text) -> Self { Self { required_name: name, found_channel: None }  }
    pub fn filter(&mut self, info: &ChannelIndexInfo) {
        if &info.info.name == &self.required_name { self.found_channel = Some(info.clone()); }
    }
}


/*macro_rules! impl_pixel_for_tuple {
    (
        $( $T: ident,)*  | $( $Text: ident,)* |
    )
    =>
    {

        // implement DesiredPixel for (A,B,C) where A/B/C: DesiredSample
        impl<  $($T,)*  > DesiredPixel for (  $($T,)*  ) where  $($T :DesiredSample,)*
        {
            type ChannelNames = (  $($Text,)*  );
            type ChannelsInfo = (  $($T ::ChannelInfo,)*  );
        }


    };
}

impl_pixel_for_tuple!{ A,B,C,D, | Text,Text,Text,Text, | }*/

impl<A,B,C,D> DesiredPixel for (A,B,C,D)
    where A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type ChannelNames = (Text, Text, Text, Text);
    type ChannelsInfo = (A::ChannelInfo, B::ChannelInfo, C::ChannelInfo, D::ChannelInfo);
}

impl<A,B,C,D> ReadFilteredChannels<(A,B,C,D), (A::ChannelInfo, B::ChannelInfo, C::ChannelInfo, D::ChannelInfo)>
for (Text, Text, Text, Text) where
    A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type Filter = (ChannelFilter, ChannelFilter, ChannelFilter,  ChannelFilter, );
    fn filter(&self) -> Self::Filter { (
        ChannelFilter::new(self.0.clone().into()),
        ChannelFilter::new(self.1.clone().into()),
        ChannelFilter::new(self.2.clone().into()),
        ChannelFilter::new(self.3.clone().into()),
    ) }
}

impl<A,B,C,D> ChannelsFilter<(A,B,C,D), (A::ChannelInfo, B::ChannelInfo, C::ChannelInfo, D::ChannelInfo)>
for (ChannelFilter, ChannelFilter, ChannelFilter, ChannelFilter)
    where
        A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type PixelReader = (A::SampleReader, B::SampleReader, C::SampleReader, D::SampleReader);

    fn visit_channel(&mut self, channel: ChannelIndexInfo) {
        self.0.filter(&channel);
        self.1.filter(&channel);
        self.2.filter(&channel);
        self.3.filter(&channel);
    }

    fn finish(self) -> Result<((A::ChannelInfo, B::ChannelInfo, C::ChannelInfo, D::ChannelInfo), Self::PixelReader)> {
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

impl<A,B,C,D> PixelReader<(A,B,C,D)> for (
    <A as DesiredSample>::SampleReader,
    <B as DesiredSample>::SampleReader,
    <C as DesiredSample>::SampleReader,
    <D as DesiredSample>::SampleReader,
)
    where A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type LineReader = (
        <<A as DesiredSample>::SampleReader as ChannelPixelReader<A>>::ChannelLineReader,
        <<B as DesiredSample>::SampleReader as ChannelPixelReader<B>>::ChannelLineReader,
        <<C as DesiredSample>::SampleReader as ChannelPixelReader<C>>::ChannelLineReader,
        <<D as DesiredSample>::SampleReader as ChannelPixelReader<D>>::ChannelLineReader,
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
    <<A as DesiredSample>::SampleReader as ChannelPixelReader<A>>::ChannelLineReader,
    <<B as DesiredSample>::SampleReader as ChannelPixelReader<B>>::ChannelLineReader,
    <<C as DesiredSample>::SampleReader as ChannelPixelReader<C>>::ChannelLineReader,
    <<D as DesiredSample>::SampleReader as ChannelPixelReader<D>>::ChannelLineReader,
)
    where A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
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





