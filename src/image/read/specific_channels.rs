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
pub struct ReadSpecificChannels<Pixel, CreatePixelStorage, SetPixel> where Pixel: DesiredPixel {

    /// A tuple containing `exr::Text` elements.
    /// Each tuple element queries the file for a channel with that name.
    pub channel_names: Pixel::ChannelNames,

    /// Creates a new pixel storage per layer
    pub create: CreatePixelStorage,

    /// Writes the pixels from the file to your image storage
    pub set_pixel: SetPixel,

    // TODO private
    /// Required to avoid `unconstrained type parameter` problems.
    pub px: PhantomData<Pixel>,
}


// TODO merge into `DesiredPixel` trait?
pub trait CreateChannelsFilter<Pixel, ChannelsDescription> { // TODO ChannelsInfo = Pixel::ChannelsInfo
    type Filter: ChannelsFilter<Pixel, ChannelsDescription>;
    fn filter(&self) -> Self::Filter;
}

pub trait ChannelsFilter<Pixel, ChannelsDescription> {
    type PixelReader: CreatePixelReaderForWidth<Pixel>;

    fn visit_channel(&mut self, channel: AlwaysCreateSampleReaderForWidth);

    /// allow err so that we can abort if a required channel does not exist in the file
    fn finish(self) -> Result<(ChannelsDescription, Self::PixelReader)>;
}



/// Define how to store a pixel in your custom pixel storage.
/// Can be a closure of type [`Fn(&mut YourPixelStorage, Vec2<usize>, YourPixel)`].
/// The Pixel type should be a tuple containing any combination of `f32`, `f16`, or `u32` values.
pub trait SetPixel<PixelStorage, Pixel> {

    /// Will be called for all pixels in the file, resulting in a complete image.
    fn set_pixel(&self, pixels: &mut PixelStorage, position: Vec2<usize>, pixel: Pixel); // TODO impl From<RgbaPixel>?
}

/// Define how to create your custom pixel storage for a given layer.
/// Can be a closure of type [`Fn(&ChannelsDescription<_>) -> YourPixelStorage`].
pub trait CreatePixels<SampleTypes> {

    /// Your custom pixel storage.
    type Pixels;

    /// Called once per rgba layer.
    fn create(&self, channels_description: &ChannelsDescription<SampleTypes>) -> Self::Pixels;
}

impl<Pxs, Px, F> SetPixel<Pxs, Px> for F where F: Fn(&mut Pxs, Vec2<usize>, Px) {
    fn set_pixel(&self, pixels: &mut Pxs, position: Vec2<usize>, pixel: Px) { self(pixels, position, pixel) }
}

impl<F, P, T> CreatePixels<T> for F where F: Fn(&ChannelsDescription<T>) -> P {
    type Pixels = P;
    fn create(&self, channels_description: &ChannelsDescription<T>) -> Self::Pixels { self(channels_description) }
}


pub trait DesiredPixel: Sized {
    type ChannelNames: CreateChannelsFilter<Self, Self::ChannelsDescription>;
    type ChannelsDescription;
}


pub trait DesiredSample: Sized {
    type ChannelDescription;
    type SampleReaderForWidth: CreateSampleReaderForWidth<Self>;
    fn create_channel_pixel_reader(channels_description: Option<AlwaysCreateSampleReaderForWidth>) -> Result<(Self::ChannelDescription, Self::SampleReaderForWidth)>;
}



/// Processes pixel blocks from a file and accumulates them into the specific channels pixel storage.
// TODO #[ignore_warning(missing_debug_implementations)]
pub struct SpecificChannelsReader<'s, Pixel, Set, Image> where Pixel: DesiredPixel {
    storage: Image,
    set_pixel: &'s Set,
    channels_description: ChannelsDescription<Pixel::ChannelsDescription>,
    pixel_reader: <<Pixel::ChannelNames as CreateChannelsFilter<Pixel, Pixel::ChannelsDescription>>::Filter as ChannelsFilter<Pixel, Pixel::ChannelsDescription>>::PixelReader,
    pixel: PhantomData<Pixel>,
}


/// A summary of the channels of a given layer.
/// Does not contain any actual pixel data.
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub struct ChannelsDescription<SampleTypes> { // TODO remove this struct?

    /// The actual type of each channel in the file.
    /// Will be converted from and to the runtime type you specify.
    pub sample_types: SampleTypes,

    /// The dimensions of this image, width and height.
    pub resolution: Vec2<usize>,
}



pub trait CreatePixelReaderForWidth<Pixel> {
    type PixelReader: Clone + PixelReader<Pixel>;
    fn pixel_reader_for_width(&self, pixel_count: usize) -> Self::PixelReader;
}

pub trait PixelReader<Pixel> {
    fn read_next_pixel(&mut self, bytes: &[u8]) -> Result<Pixel>;
}

// TODO what about subsampling?

impl<'s, Px, Setter: 's, Constructor: 's>
    ReadChannels<'s> for ReadSpecificChannels<Px, Constructor, Setter> where
    Px: DesiredPixel,
    Constructor: CreatePixels<Px::ChannelsDescription>,
    Setter: SetPixel<Constructor::Pixels, Px>,
{
    type Reader = SpecificChannelsReader<'s, Px, Setter, Constructor::Pixels>;

    fn create_channels_reader(&'s self, header: &Header) -> Result<Self::Reader> {
        if header.deep { return Err(Error::invalid("`SpecificChannels` does not support deep data")) }

        let (sample_types, reader) = {
            let mut filter = self.channel_names.filter();
            let mut byte_offset = 0;

            for (channel_index, channel) in header.channels.list.iter().enumerate() {
                let chan_indices = AlwaysCreateSampleReaderForWidth {
                    sample_byte_offset: byte_offset,
                    channel_description: channel.clone(),
                    channel_index
                };

                filter.visit_channel(chan_indices);
                byte_offset += channel.sample_type.bytes_per_sample();
            }

            filter.finish()?
        };

        // let (sample_types, reader) = self.channel_names.inspect_channels(&header.channels)?;
        let channels = ChannelsDescription { sample_types, resolution: header.layer_size, };

        Ok(SpecificChannelsReader {
            set_pixel: &self.set_pixel,
            storage: self.create.create(&channels),
            pixel_reader: reader,
            pixel: Default::default(),
            channels_description: channels
        })
    }
}


impl<Px, Setter, Storage>
    ChannelsReader for SpecificChannelsReader<'_, Px, Setter, Storage>
where
    Px: DesiredPixel,
    Setter: SetPixel<Storage, Px>,
{
    type Channels = SpecificChannels<Storage, Px::ChannelsDescription>;

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

        let initial_pixel_line_reader = self.pixel_reader.pixel_reader_for_width(pixels_per_line);

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
            channels: self.channels_description.sample_types,
            storage: self.storage
        }
    }
}



#[derive(Clone, Debug)]
pub struct AlwaysCreateSampleReaderForWidth {
    pub channel_description: ChannelDescription,
    pub sample_byte_offset: usize,
    pub channel_index: usize,
}

pub trait CreateSampleReaderForWidth<Sample>: Sized + Clone {
    type SampleReader: SampleReader<Sample>;
    fn sample_reader_for_width(&self, line_width: usize) -> Self::SampleReader;
}

pub trait SampleReader<Sample>: Sized + Clone {
    fn read_next_sample(&mut self, bytes: &[u8]) -> Result<Sample>;
}

pub trait FromSample: Copy + Clone { fn from_sample(sample: Sample) -> Self; }
impl FromSample for f16 { fn from_sample(sample: Sample) -> Self { sample.to_f16() } }
impl FromSample for f32 { fn from_sample(sample: Sample) -> Self { sample.to_f32() } }
impl FromSample for u32 { fn from_sample(sample: Sample) -> Self { sample.to_u32() } }
impl FromSample for Sample { fn from_sample(sample: Sample) -> Self { sample } }

impl<S> DesiredSample for S where S: FromSample {
    type ChannelDescription = ChannelDescription;
    type SampleReaderForWidth = AlwaysCreateSampleReaderForWidth;
    fn create_channel_pixel_reader(channel_indices: Option<AlwaysCreateSampleReaderForWidth>) -> Result<(Self::ChannelDescription, Self::SampleReaderForWidth)> {
        channel_indices.map(|chan| (chan.channel_description.clone(), chan))
            .ok_or_else(|| Error::invalid("layer does not contain all of the specified required channels")) // TODO which channel??
    }
}

impl<S> DesiredSample for Option<S> where S: FromSample {
    type ChannelDescription = Option<ChannelDescription>;
    type SampleReaderForWidth = Option<AlwaysCreateSampleReaderForWidth>;
    fn create_channel_pixel_reader(indices: Option<AlwaysCreateSampleReaderForWidth>) -> Result<(Self::ChannelDescription, Self::SampleReaderForWidth)> {
        Ok(indices.map_or((None, None), |chan| (Some(chan.channel_description.clone()), Some(chan)))) // TODO no clone
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlwaysSampleReader<S> {
    file_sample_type: SampleType,
    next_byte: usize,
    s: PhantomData<S>,
}

impl<S> CreateSampleReaderForWidth<S> for AlwaysCreateSampleReaderForWidth where S: Clone + FromSample {
    type SampleReader = AlwaysSampleReader<S>;
    fn sample_reader_for_width(&self, pixel_count: usize) -> Self::SampleReader {
        let start = self.sample_byte_offset * pixel_count; // TODO  will never work with subsampling?
        AlwaysSampleReader { file_sample_type: self.channel_description.sample_type, next_byte: start, s: Default::default() }
    }
}


impl<S> CreateSampleReaderForWidth<Option<S>> for Option<AlwaysCreateSampleReaderForWidth> where S: Clone + FromSample {
    type SampleReader = Option<AlwaysSampleReader<S>>;
    fn sample_reader_for_width(&self, line_width: usize) -> Self::SampleReader {
        self.as_ref().map(|this| {
            this.sample_reader_for_width(line_width)
        })
    }
}

impl<S> SampleReader<S> for AlwaysSampleReader<S> where S: FromSample + Clone {
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

impl<S> SampleReader<Option<S>> for Option<AlwaysSampleReader<S>> where S: FromSample + Clone {
    fn read_next_sample(&mut self, bytes: &[u8]) -> Result<Option<S>> {
        self.as_mut()
            .map(|this| this.read_next_sample(bytes))
            .transpose()
    }
}


#[derive(Debug, Clone)]
pub struct ChannelFilter {
    required_name: Text,
    found_channel: Option<AlwaysCreateSampleReaderForWidth>,
}
impl ChannelFilter {
    pub fn new(name: Text) -> Self { Self { required_name: name, found_channel: None }  }
    pub fn filter(&mut self, channel_indices: &AlwaysCreateSampleReaderForWidth) {
        if &channel_indices.channel_description.name == &self.required_name { self.found_channel = Some(channel_indices.clone()); }
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
    type ChannelsDescription = (A::ChannelDescription, B::ChannelDescription, C::ChannelDescription, D::ChannelDescription);
}

impl<A,B,C,D> CreateChannelsFilter<(A, B, C, D), (A::ChannelDescription, B::ChannelDescription, C::ChannelDescription, D::ChannelDescription)>
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

impl<A,B,C,D> ChannelsFilter<(A,B,C,D), (A::ChannelDescription, B::ChannelDescription, C::ChannelDescription, D::ChannelDescription)>
for (ChannelFilter, ChannelFilter, ChannelFilter, ChannelFilter)
    where
        A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type PixelReader = (A::SampleReaderForWidth, B::SampleReaderForWidth, C::SampleReaderForWidth, D::SampleReaderForWidth);

    fn visit_channel(&mut self, channel: AlwaysCreateSampleReaderForWidth) {
        self.0.filter(&channel);
        self.1.filter(&channel);
        self.2.filter(&channel);
        self.3.filter(&channel);
    }

    fn finish(self) -> Result<((A::ChannelDescription, B::ChannelDescription, C::ChannelDescription, D::ChannelDescription), Self::PixelReader)> {
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


impl<A,B,C,D> CreatePixelReaderForWidth<(A, B, C, D)> for (
    <A as DesiredSample>::SampleReaderForWidth,
    <B as DesiredSample>::SampleReaderForWidth,
    <C as DesiredSample>::SampleReaderForWidth,
    <D as DesiredSample>::SampleReaderForWidth,
)
    where A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type PixelReader = (
        <<A as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<A>>::SampleReader,
        <<B as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<B>>::SampleReader,
        <<C as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<C>>::SampleReader,
        <<D as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<D>>::SampleReader,
    );

    fn pixel_reader_for_width(&self, pixel_count: usize) -> Self::PixelReader {
        (
            self.0.sample_reader_for_width(pixel_count),
            self.1.sample_reader_for_width(pixel_count),
            self.2.sample_reader_for_width(pixel_count),
            self.3.sample_reader_for_width(pixel_count),
        )
    }
}

impl<A,B,C,D> PixelReader<(A, B, C, D)> for (
    <<A as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<A>>::SampleReader,
    <<B as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<B>>::SampleReader,
    <<C as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<C>>::SampleReader,
    <<D as DesiredSample>::SampleReaderForWidth as CreateSampleReaderForWidth<D>>::SampleReader,
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





