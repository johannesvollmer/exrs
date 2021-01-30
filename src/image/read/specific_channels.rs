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
// use crate::block::samples::Sample;
use crate::block::chunk::TileCoordinates;
use std::marker::PhantomData;

pub trait FromNativeSample: Sized + Copy + Default + 'static {
    fn from_f16(value: f16) -> Self;
    fn from_f32(value: f32) -> Self;
    fn from_u32(value: u32) -> Self;
}

// TODO havent i implemented this exact behaviour already somewhere else in this library...??
impl FromNativeSample for f32 {
    fn from_f16(value: f16) -> Self { value.to_f32() }
    fn from_f32(value: f32) -> Self { value } // this branch means that we never have to match every single sample if the file format matches the expected output
    fn from_u32(value: u32) -> Self { value as f32 }
}

impl FromNativeSample for u32 {
    fn from_f16(value: f16) -> Self { value.to_f32() as u32 }
    fn from_f32(value: f32) -> Self { value as u32 }
    fn from_u32(value: u32) -> Self { value }
}

impl FromNativeSample for f16 {
    fn from_f16(value: f16) -> Self { value }
    fn from_f32(value: f32) -> Self { f16::from_f32(value) }
    fn from_u32(value: u32) -> Self { f16::from_f32(value as f32) }
}

impl FromNativeSample for Sample {
    fn from_f16(value: f16) -> Self { Self::from(value) }
    fn from_f32(value: f32) -> Self { Self::from(value) }
    fn from_u32(value: u32) -> Self { Self::from(value) }
}

pub trait ReadSpecificChannel: Sized {
    type RecursivePixelReader: RecursivePixelReader;
    fn create_recursive_reader(&self, channels: &ChannelList) -> Result<Self::RecursivePixelReader>;


    fn required<Sample>(self, channel_name: impl Into<Text>) -> ReadRequiredChannel<Self, Sample> {
        ReadRequiredChannel { channel_name: channel_name.into(), previous_channels: self, px: Default::default() }
    }

    fn optional<Sample>(self, channel_name: impl Into<Text>, default_sample: Sample)
        -> ReadOptionalChannel<Self, Sample>
    {
        ReadOptionalChannel { channel_name: channel_name.into(), previous_channels: self, default_sample }
    }

    fn collect_channels<Pixel, PixelStorage, CreatePixels, SetPixel>(
        self, create_pixels: CreatePixels, set_pixel: SetPixel
    ) -> CollectSpecificChannels<Self, Pixel, PixelStorage, CreatePixels, SetPixel>
        where
            <Self::RecursivePixelReader as RecursivePixelReader>::RecursivePixel: IntoTuple<Pixel>,
            <Self::RecursivePixelReader as RecursivePixelReader>::RecursiveChannelDescriptions: IntoNonRecursive,
            CreatePixels: Fn(Vec2<usize>, &<<Self::RecursivePixelReader as RecursivePixelReader>::RecursiveChannelDescriptions as IntoNonRecursive>::NonRecursive) -> PixelStorage,
            SetPixel: Fn(&mut PixelStorage, Vec2<usize>, Pixel),
    {
        CollectSpecificChannels { read_channels: self, set_pixel, create_pixels, px: Default::default() }
    }
}

pub trait RecursivePixelReader {
    type RecursiveChannelDescriptions;
    fn get_descriptions(&self) -> Self::RecursiveChannelDescriptions;

    type RecursivePixel: Copy + Default + 'static;
    fn read_pixels<'s>(&self, bytes: &'s[u8], pixels: impl 's + ExactSizeIterator<Item=&'s mut Self::RecursivePixel>);
}

// does not use the generic `Recursive` struct to reduce the number of angle brackets in the public api
#[derive(Clone, Debug)]
pub struct ReadOptionalChannel<ReadChannels, Sample> {
    previous_channels: ReadChannels,
    channel_name: Text,
    default_sample: Sample,
}

// does not use the generic `Recursive` struct to reduce the number of angle brackets in the public api
#[derive(Clone, Debug)]
pub struct ReadRequiredChannel<ReadChannels, Sample> {
    previous_channels: ReadChannels,
    channel_name: Text,
    px: PhantomData<Sample>,
}


#[derive(Copy, Clone, Debug, Default)]
pub struct Recursive<Inner, Value> {
    inner: Inner,
    value: Value,
}

impl<Inner, Value> Recursive<Inner, Value> { pub fn new(inner: Inner, value: Value) -> Self { Self { inner, value } } }

#[derive(Copy, Clone, Debug, Default)]
pub struct NoneMore;


pub trait IntoTuple<Tuple> {
    fn into_tuple(self) -> Tuple;
}

pub trait IntoNonRecursive {
    type NonRecursive;
    fn into_non_recursive(self) -> Self::NonRecursive;
}

impl IntoTuple<()> for NoneMore { fn into_tuple(self) -> () { () } }
impl<A> IntoTuple<A> for Recursive<NoneMore, A> { fn into_tuple(self) -> A { self.value } }
impl<A,B> IntoTuple<(A,B)> for Recursive<Recursive<NoneMore, A>, B> { fn into_tuple(self) -> (A, B) { (self.inner.value, self.value) } }
impl<A,B,C> IntoTuple<(A,B,C)> for Recursive<Recursive<Recursive<NoneMore, A>, B>, C> { fn into_tuple(self) -> (A, B, C) { (self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D> IntoTuple<(A,B,C,D)> for Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D> { fn into_tuple(self) -> (A, B, C, D) { (self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }

/*macro_rules! impl_into_tuple_for_recursive_type {
    ( $channel: ident, ) => {
        impl<A,B,C> IntoTuple<(A,B,C)>
        for Recursive<Recursive<Recursive<NoneMore, A>, B>, C> {
            fn into_tuple(self) -> (A, B, C) { (self.inner.inner.value, self.inner.value, self.value) }
        }

    };
}*/

// impl<AsTuple, Tuple> IntoNonRecursive for AsTuple where AsTuple: IntoTuple<Tuple> {
//     type NonRecursive = Tuple;
//     fn into_friendlier(self) -> Self::NonRecursive { self.into_tuple() }
// }
impl IntoNonRecursive for NoneMore { type NonRecursive = ();fn into_non_recursive(self) -> Self::NonRecursive { () } }
impl<A> IntoNonRecursive for Recursive<NoneMore, A> { type NonRecursive = A; fn into_non_recursive(self) -> Self::NonRecursive { self.value } }
impl<A,B> IntoNonRecursive for Recursive<Recursive<NoneMore, A>, B> { type NonRecursive = (A, B); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C> IntoNonRecursive for Recursive<Recursive<Recursive<NoneMore, A>, B>, C> { type NonRecursive = (A, B, C); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D> { type NonRecursive = (A, B, C, D); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }


#[derive(Copy, Clone, Debug)]
pub struct CollectSpecificChannels<ReadChannels, Pixel, PixelStorage, CreatePixels, SetPixel> {
    read_channels: ReadChannels,
    create_pixels: CreatePixels,
    set_pixel: SetPixel,
    px: PhantomData<(Pixel, PixelStorage)>,
}

impl<'s, InnerChannels, Pixel, PixelStorage, CreatePixels, SetPixel: 's>
ReadChannels<'s> for CollectSpecificChannels<InnerChannels, Pixel, PixelStorage, CreatePixels, SetPixel>
    where
        InnerChannels: ReadSpecificChannel,
        <InnerChannels::RecursivePixelReader as RecursivePixelReader>::RecursivePixel: IntoTuple<Pixel>,
        <InnerChannels::RecursivePixelReader as RecursivePixelReader>::RecursiveChannelDescriptions: IntoNonRecursive,
        CreatePixels: Fn(Vec2<usize>, &<<InnerChannels::RecursivePixelReader as RecursivePixelReader>::RecursiveChannelDescriptions as IntoNonRecursive>::NonRecursive) -> PixelStorage,
        SetPixel: Fn(&mut PixelStorage, Vec2<usize>, Pixel),
{
    type Reader = SpecificChannelsReader<
        PixelStorage, &'s SetPixel,
        InnerChannels::RecursivePixelReader,
        Pixel,
    >;

    fn create_channels_reader(&'s self, header: &Header) -> Result<Self::Reader> {
        if header.deep { return Err(Error::invalid("`SpecificChannels` does not support deep data yet")) }

        let pixel_reader = self.read_channels.create_recursive_reader(&header.channels)?;
        let channel_descriptions = pixel_reader.get_descriptions().into_non_recursive();// TODO not call this twice

        let create = &self.create_pixels;
        let pixel_storage = create(header.layer_size, &channel_descriptions);

        Ok(SpecificChannelsReader {
            set_pixel: &self.set_pixel,
            pixel_storage,
            pixel_reader,
            px: Default::default()
        })
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SpecificChannelsReader<PixelStorage, SetPixel, PixelReader, Pixel> {
    set_pixel: SetPixel,
    pixel_storage: PixelStorage,
    pixel_reader: PixelReader,
    px: PhantomData<Pixel>
}

impl<PixelStorage, SetPixel, PxReader, Pixel>
ChannelsReader for SpecificChannelsReader<PixelStorage, SetPixel, PxReader, Pixel>
    where PxReader: RecursivePixelReader,
          PxReader::RecursivePixel: IntoTuple<Pixel>,
          PxReader::RecursiveChannelDescriptions: IntoNonRecursive,
          SetPixel: Fn(&mut PixelStorage, Vec2<usize>, Pixel),
{
    type Channels = SpecificChannels<PixelStorage, <PxReader::RecursiveChannelDescriptions as IntoNonRecursive>::NonRecursive>;

    fn filter_block(&self, (_, tile): (usize, &TileCoordinates)) -> bool { tile.is_largest_resolution_level() } // TODO all levels

    fn read_block(&mut self, header: &Header, block: UncompressedBlock) -> UnitResult {
        let mut pixels = vec![PxReader::RecursivePixel::default(); block.index.pixel_size.width()]; // TODO allocate once in self

        let byte_lines = block.data.chunks_exact(header.channels.bytes_per_pixel * block.index.pixel_size.width());
        debug_assert_eq!(byte_lines.len(), block.index.pixel_size.height());

        for (y_offset, line_bytes) in byte_lines.enumerate() { // TODO sampling
            // this two-step copy method should be very cache friendly in theory, and also reduce sample_type lookup count
            self.pixel_reader.read_pixels(line_bytes, pixels.iter_mut());

            for (x_offset, pixel) in pixels.iter().enumerate() {
                let set_pixel = &self.set_pixel;
                set_pixel(&mut self.pixel_storage, block.index.pixel_position + Vec2(x_offset, y_offset), pixel.into_tuple());
            }
        }

        Ok(())
    }

    fn into_channels(self) -> Self::Channels {
        SpecificChannels { channels: self.pixel_reader.get_descriptions().into_non_recursive(), storage: self.pixel_storage }
    }
}


pub type ReadNoChannels = NoneMore;
impl ReadSpecificChannel for NoneMore {
    type RecursivePixelReader = NoneMore;
    fn create_recursive_reader(&self, _: &ChannelList) -> Result<Self::RecursivePixelReader> { Ok(NoneMore) }
}

impl<DefaultSample, ReadChannels> ReadSpecificChannel for ReadOptionalChannel<ReadChannels, DefaultSample>
    where ReadChannels: ReadSpecificChannel, DefaultSample: FromNativeSample + 'static,
{
    type RecursivePixelReader = Recursive<ReadChannels::RecursivePixelReader, OptionalSampleReader<DefaultSample>>;

    fn create_recursive_reader(&self, channels: &ChannelList) -> Result<Self::RecursivePixelReader> {
        let inner_samples_reader = self.previous_channels.create_recursive_reader(channels)?;
        let reader = channels.channels_with_byte_offset()
            .find(|(_, channel)| channel.name == self.channel_name)
            .map(|(channel_byte_offset, channel)| SampleReader {
                channel_byte_offset, channel: channel.clone(),
                px: Default::default()
            });

        Ok(Recursive::new(inner_samples_reader, OptionalSampleReader {
            reader, default_sample: self.default_sample,
        }))
    }
}

impl<Sample, ReadChannels> ReadSpecificChannel for ReadRequiredChannel<ReadChannels, Sample>
    where ReadChannels: ReadSpecificChannel, Sample: FromNativeSample + 'static
{
    type RecursivePixelReader = Recursive<ReadChannels::RecursivePixelReader, SampleReader<Sample>>;

    fn create_recursive_reader(&self, channels: &ChannelList) -> Result<Self::RecursivePixelReader> {
        let previous_samples_reader = self.previous_channels.create_recursive_reader(channels)?;
        let (channel_byte_offset, channel) = channels.channels_with_byte_offset()
                .find(|(_, channel)| channel.name == self.channel_name)
                .ok_or_else(|| Error::invalid(format!(
                    "layer does not contain all of your specified channels (`{}` is missing)",
                    self.channel_name
                )))?;

        Ok(Recursive::new(previous_samples_reader, SampleReader { channel_byte_offset, channel: channel.clone(), px: Default::default() }))
    }
}

#[derive(Clone, Debug)]
pub struct SampleReader<Sample> {
    /// to be multiplied with line width!
    channel_byte_offset: usize,
    channel: ChannelDescription,
    px: PhantomData<Sample>
}

#[derive(Clone, Debug)]
pub struct OptionalSampleReader<DefaultSample> {
    reader: Option<SampleReader<DefaultSample>>,
    default_sample: DefaultSample,
}



impl RecursivePixelReader for NoneMore {
    type RecursiveChannelDescriptions = NoneMore;
    fn get_descriptions(&self) -> Self::RecursiveChannelDescriptions { NoneMore }

    type RecursivePixel = NoneMore;
    fn read_pixels<'s>(&self, _: &'s[u8], uniterated_samples: impl 's + ExactSizeIterator<Item=&'s mut Self::RecursivePixel>) {
        for _ in uniterated_samples { } // FIXME needs to run iterator once, this is ugly!
    }
}

impl<Sample, InnerReader: RecursivePixelReader>
    RecursivePixelReader
    for Recursive<InnerReader, SampleReader<Sample>>
    where Sample: FromNativeSample + 'static
{
    type RecursiveChannelDescriptions = Recursive<InnerReader::RecursiveChannelDescriptions, ChannelDescription>;
    fn get_descriptions(&self) -> Self::RecursiveChannelDescriptions { Recursive::new(self.inner.get_descriptions(), self.value.channel.clone()) }

    type RecursivePixel = Recursive<InnerReader::RecursivePixel, Sample>;

    fn read_pixels<'s>(&self, bytes: &'s[u8], pixels: impl 's + ExactSizeIterator<Item=&'s mut Self::RecursivePixel>) {
        let start_index = pixels.len() * self.value.channel_byte_offset;
        let byte_count = pixels.len() * self.value.channel.sample_type.bytes_per_sample();
        let mut own_bytes_reader = &bytes[start_index .. start_index + byte_count]; // TODO check block size somewhere

        // TODO deduplicate with `Optional[Self]`
        match self.value.channel.sample_type {
            SampleType::F16 => {

                // FIXME this will not go through per channel, but instead go through all channels in parallel! would need to collect somehow...?
                let updated_samples = pixels.map(|pixel|{
                    pixel.value = Sample::from_f16(f16::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                    &mut pixel.inner
                });

                self.inner.read_pixels(bytes, updated_samples);
            },

            SampleType::F32 => {
                // TODO inner first, self second, because of cache
                let updated_samples = pixels.map(|pixel|{
                    pixel.value = Sample::from_f32(f32::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                    &mut pixel.inner
                });

                self.inner.read_pixels(bytes, updated_samples);
            },

            SampleType::U32 => {
                // TODO inner first, self second, because of cache
                let updated_samples = pixels.map(|pixel|{
                    pixel.value = Sample::from_u32(u32::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                    &mut pixel.inner
                });

                self.inner.read_pixels(bytes, updated_samples);
            },
        }

        debug_assert!(own_bytes_reader.is_empty());
    }
}

impl<Sample, InnerReader: RecursivePixelReader>
RecursivePixelReader
for Recursive<InnerReader, OptionalSampleReader<Sample>>
    where Sample: FromNativeSample + 'static
{
    type RecursiveChannelDescriptions = Recursive<InnerReader::RecursiveChannelDescriptions, Option<ChannelDescription>>;
    fn get_descriptions(&self) -> Self::RecursiveChannelDescriptions { Recursive::new(
        self.inner.get_descriptions(), self.value.reader.as_ref().map(|reader| reader.channel.clone())
    ) }

    type RecursivePixel = Recursive<InnerReader::RecursivePixel, Sample>;

    fn read_pixels<'s>(&self, bytes: &'s[u8], pixels: impl 's + ExactSizeIterator<Item=&'s mut Self::RecursivePixel>) {
        match &self.value.reader {
            Some(reader) => {
                let start_index = pixels.len() * reader.channel_byte_offset;
                let byte_count = pixels.len() * reader.channel.sample_type.bytes_per_sample();
                let mut own_bytes_reader = &bytes[start_index .. start_index + byte_count]; // TODO check block size somewhere

                match reader.channel.sample_type {
                    SampleType::F16 => {
                        // TODO inner first, self second, because of cache
                        let updated_samples = pixels.map(|pixel|{
                            pixel.value = Sample::from_f16(f16::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                            &mut pixel.inner
                        });

                        self.inner.read_pixels(bytes, updated_samples);
                    },

                    SampleType::F32 => {
                        // TODO inner first, self second, because of cache
                        let updated_samples = pixels.map(|pixel|{
                            pixel.value = Sample::from_f32(f32::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                            &mut pixel.inner
                        });

                        self.inner.read_pixels(bytes, updated_samples);
                    },

                    SampleType::U32 => {
                        // TODO inner first, self second, because of cache
                        let updated_samples = pixels.map(|pixel|{
                            pixel.value = Sample::from_u32(u32::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                            &mut pixel.inner
                        });

                        self.inner.read_pixels(bytes, updated_samples);
                    },
                }

                debug_assert!(own_bytes_reader.is_empty());
            }

            // if this channel is optional and was not found in the file, fill the default sample
            None => { // None is the default value, so don't do anything, just continue with the next channel:
                let updated_samples = pixels.map(|pixel|{
                    pixel.value = self.value.default_sample;
                    &mut pixel.inner
                });

                self.inner.read_pixels(bytes, updated_samples);
            },

            _ => panic!("contradicting state in recursive pixel reader (bug)")
        }
    }
}






/*

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
pub trait CreateChannelsLocator<Pixel, ChannelsDescription> { // TODO ChannelsInfo = Pixel::ChannelsInfo
    type Locator: ChannelsLocator<Pixel, ChannelsDescription>;
    fn create_locator(&self) -> Self::Locator;
}

pub trait ChannelsLocator<Pixel, ChannelsDescription> {
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
    type ChannelNames: CreateChannelsLocator<Self, Self::ChannelsDescription>;
    type ChannelsDescription;
}


pub trait DesiredSample: Sized {
    type ChannelDescription;
    type SampleReaderForWidth: CreateSampleReaderForWidth<Self>;
    fn get_channel_description(indices: &Option<AlwaysCreateSampleReaderForWidth>) -> Result<Self::ChannelDescription>;
    fn create_sample_reader_for_width(indices: Option<AlwaysCreateSampleReaderForWidth>) -> Result<Self::SampleReaderForWidth>;
}



/// Processes pixel blocks from a file and accumulates them into the specific channels pixel storage.
// TODO #[ignore_warning(missing_debug_implementations)]
pub struct SpecificChannelsReader<'s, Pixel, Set, Image> where Pixel: DesiredPixel {
    storage: Image,
    set_pixel: &'s Set,
    channels_description: ChannelsDescription<Pixel::ChannelsDescription>,
    pixel_reader: <<Pixel::ChannelNames as CreateChannelsLocator<Pixel, Pixel::ChannelsDescription>>::Locator as ChannelsLocator<Pixel, Pixel::ChannelsDescription>>::PixelReader,
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
            let mut locator = self.channel_names.create_locator();
            let mut byte_offset = 0;

            for (channel_index, channel) in header.channels.list.iter().enumerate() {
                let chan_indices = AlwaysCreateSampleReaderForWidth {
                    sample_byte_offset: byte_offset,
                    channel_description: channel.clone(),
                    channel_index
                };

                locator.visit_channel(chan_indices);
                byte_offset += channel.sample_type.bytes_per_sample();
            }

            locator.finish()?
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

    fn get_channel_description(indices: &Option<AlwaysCreateSampleReaderForWidth>) -> Result<Self::ChannelDescription> {
        Ok(Self::create_sample_reader_for_width(indices.clone())?.channel_description) // call other method to reuse error message
    }

    fn create_sample_reader_for_width(indices: Option<AlwaysCreateSampleReaderForWidth>) -> Result<Self::SampleReaderForWidth> {
        indices.ok_or_else(|| Error::invalid("layer does not contain all of your specified channels")) // TODO be more precise: which layer is missing?
    }
}

impl<S> DesiredSample for Option<S> where S: FromSample {
    type ChannelDescription = Option<ChannelDescription>;
    type SampleReaderForWidth = Option<AlwaysCreateSampleReaderForWidth>;

    fn get_channel_description(indices: &Option<AlwaysCreateSampleReaderForWidth>) -> Result<Self::ChannelDescription> {
        Ok(indices.as_ref().map(|chan| chan.channel_description.clone())) // TODO no clone
    }

    fn create_sample_reader_for_width(indices: Option<AlwaysCreateSampleReaderForWidth>) -> Result<Self::SampleReaderForWidth> {
        Ok(indices)
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
pub struct ChannelLocator {
    required_name: Text,
    found_channel: Option<AlwaysCreateSampleReaderForWidth>,
}
impl ChannelLocator {
    pub fn new(name: Text) -> Self { Self { required_name: name, found_channel: None }  }
    pub fn filter(&mut self, channel_indices: &AlwaysCreateSampleReaderForWidth) {
        if &channel_indices.channel_description.name == &self.required_name { self.found_channel = Some(channel_indices.clone()); }
    }
}


/*macro_rules! impl_pixel_for_tuple {
    (
        $($T: ident,)*  | $($Text: ident,)* | $($Locator: ident,)* | $($index: ident,)* $
    )
    =>
    {

        // implement DesiredPixel for (A,B,C) where A/B/C: DesiredSample
        impl<  $($T,)*  > DesiredPixel for (  $($T,)*  ) where  $($T: DesiredSample,)*
        {
            type ChannelNames = (  $($Text,)*  );
            type ChannelsDescription = (  $($T::ChannelDescription,)*  );
        }

        // implement CreateChannelsLocator<(A,B,C), (A,B,C)::ChannelsDescr> for (Text,Text,Text)
        impl<  $($T,)*  > CreateChannelsLocator<(  $($T,)*  ),  (  $($T::ChannelDescription,)*  )>
        for (  $($Text,)*  ) where  $($T: DesiredSample,)*
        {
            type Locator = (  $($Locator,)*  );
            fn create_locator(&self) -> Self::Locator { (
                $(  ChannelLocator::new( self .$index .clone().into()),  )*
            ) }
        }
    };
}

impl_pixel_for_tuple!{ A,B,C,D, | Text,Text,Text,Text, | ChannelLocator,ChannelLocator,ChannelLocator,ChannelLocator, | 0,1,2,3, $ }*/

impl<A,B,C,D> DesiredPixel for (A,B,C,D)
    where A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type ChannelNames = (Text, Text, Text, Text);
    type ChannelsDescription = (A::ChannelDescription, B::ChannelDescription, C::ChannelDescription, D::ChannelDescription);
}

impl<A,B,C,D> CreateChannelsLocator<(A, B, C, D), (A::ChannelDescription, B::ChannelDescription, C::ChannelDescription, D::ChannelDescription)>
for (Text, Text, Text, Text) where
    A: DesiredSample, B: DesiredSample, C: DesiredSample, D: DesiredSample,
{
    type Locator = (ChannelLocator, ChannelLocator, ChannelLocator, ChannelLocator, );
    fn create_locator(&self) -> Self::Locator { (
        ChannelLocator::new(self.0.clone().into()),
        ChannelLocator::new(self.1.clone().into()),
        ChannelLocator::new(self.2.clone().into()),
        ChannelLocator::new(self.3.clone().into()),
    ) }
}

impl<A,B,C,D> ChannelsLocator<(A, B, C, D), (A::ChannelDescription, B::ChannelDescription, C::ChannelDescription, D::ChannelDescription)>
for (ChannelLocator, ChannelLocator, ChannelLocator, ChannelLocator)
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
        let a_type = A::get_channel_description(&self.0.found_channel)?;
        let b_type = B::get_channel_description(&self.1.found_channel)?;
        let c_type = C::get_channel_description(&self.2.found_channel)?;
        let d_type = D::get_channel_description(&self.3.found_channel)?;

        let a_reader = A::create_sample_reader_for_width(self.0.found_channel)?;
        let b_reader = B::create_sample_reader_for_width(self.1.found_channel)?;
        let c_reader = C::create_sample_reader_for_width(self.2.found_channel)?;
        let d_reader = D::create_sample_reader_for_width(self.3.found_channel)?;

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


*/


