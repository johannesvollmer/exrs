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
use crate::block::chunk::TileCoordinates;
use std::marker::PhantomData;

pub trait FromNativeSample: Sized + Copy + Default + 'static {
    fn from_f16(value: f16) -> Self;
    fn from_f32(value: f32) -> Self;
    fn from_u32(value: u32) -> Self;
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

    fn collect_pixels<Pixel, PixelStorage, CreatePixels, SetPixel>(
        self, create_pixels: CreatePixels, set_pixel: SetPixel
    ) -> CollectPixels<Self, Pixel, PixelStorage, CreatePixels, SetPixel>
        where
            <Self::RecursivePixelReader as RecursivePixelReader>::RecursivePixel: IntoTuple<Pixel>,
            <Self::RecursivePixelReader as RecursivePixelReader>::RecursiveChannelDescriptions: IntoNonRecursive,
            CreatePixels: Fn(Vec2<usize>, &<<Self::RecursivePixelReader as RecursivePixelReader>::RecursiveChannelDescriptions as IntoNonRecursive>::NonRecursive) -> PixelStorage,
            SetPixel: Fn(&mut PixelStorage, Vec2<usize>, Pixel),
    {
        CollectPixels { read_channels: self, set_pixel, create_pixels, px: Default::default() }
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

// TODO use a macro to generate these impls!
impl IntoTuple<()> for NoneMore { fn into_tuple(self) -> () { () } }
impl<A> IntoTuple<A> for Recursive<NoneMore, A> { fn into_tuple(self) -> A { self.value } }
impl<A,B> IntoTuple<(A,B)> for Recursive<Recursive<NoneMore, A>, B> { fn into_tuple(self) -> (A, B) { (self.inner.value, self.value) } }
impl<A,B,C> IntoTuple<(A,B,C)> for Recursive<Recursive<Recursive<NoneMore, A>, B>, C> { fn into_tuple(self) -> (A, B, C) { (self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D> IntoTuple<(A,B,C,D)> for Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D> { fn into_tuple(self) -> (A, B, C, D) { (self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E> IntoTuple<(A,B,C,D,E)> for Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E> { fn into_tuple(self) -> (A, B, C, D, E) { (self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E,F> IntoTuple<(A,B,C,D,E,F)> for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F> { fn into_tuple(self) -> (A, B, C, D, E, F) { (self.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E,F,G> IntoTuple<(A,B,C,D,E,F,G)> for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G> { fn into_tuple(self) -> (A, B, C, D, E, F, G) { (self.inner.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }
impl<A,B,C,D,E,F,G,H> IntoTuple<(A,B,C,D,E,F,G,H)> for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G>, H> { fn into_tuple(self) -> (A, B, C, D, E, F, G, H) { (self.inner.inner.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.inner.value, self.inner.inner.inner.inner.value, self.inner.inner.inner.value, self.inner.inner.value, self.inner.value, self.value) } }

/*macro_rules! impl_into_tuple_for_recursive_type {
    ( $($types: ident),* => => $nested_type:ty => $($accessors:expr),* => /*empty $acessor_prefix:expr*/ ) => {
        impl<  $($types),*  > IntoTuple<(  $($types),*  )> for $nested_type {
            fn into_tuple(self) -> (  $($types),*  ) { (  $($accessors),*  ) }
        }
    };

    ( $($types: ident),* => $last_type:ident, $($remaining_types:ident),* => $nested_type:ty => $($accessors:expr),* => $acessor_prefix:expr ) => {
        impl_into_tuple_for_recursive_type!{
            $($types),* =>
            $($remaining_types),* =>
            Recursive< $nested_type, $last_type > =>
            $acessor_prefix .value, $($accessors),* =>
            $acessor_prefix .inner
        }
    };

    ( $($types:ident),* ) => {
        impl_into_tuple_for_recursive_type!{
            $($types),* => $($types),* => NoneMore => => self.value
        }
    };
}*/

/*macro_rules! gen_impl {

    ( IntoTuple:nested_type: $inner_recursive:ty |   ) => {
        $inner_recursive<>
    };
    ( IntoTuple:nested_type: $inner_recursive:ty | $first_chan:ident $(,$remaining_chans:ident)*  ) => {
        gen_impl!(IntoTuple:nested_type: Recursive<$inner_recursive, $first_chan> | $($remaining_chans),* )
    };

    ( IntoTuple:accessors: $self:ident | $($types:ident),* ) => {
        ($self.inner.inner.inner.value, $self.inner.inner.value, $self.inner.value, $self.value)
    };

    ( IntoTuple: $($types: ident),* ) => {
        impl<  $($types),*  > IntoTuple<(  $($types),*  )> for ( gen_impl!( IntoTuple:nested_type: $($types),* ) ) {
            fn into_tuple(self) -> (  $($types),*  ) {
                gen_impl!( IntoTuple:accessors: self | $($types),* )
            }
        }
    };

}

gen_impl! {
    IntoTuple:
    A,B,C,D
}*/

//impl_into_tuple_for_recursive_type! { A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T }

/*macro_rules! impl_into_tuple_for_recursive_type_all {

    // internal initial call
    ( $types:expr ) => {
        impl_into_tuple_for_recursive_type!{
            $types => $types => NoneMore => => self.value
        }
    };

    // initial call
    ( $( $types:expr );* ) => {
        impl_into_tuple_for_recursive_type_all!{
            $($types),* => $($types),* => NoneMore => => self.value
        }
    };
}

// impl for sizes 2,3,4,5,6,7,8,12,16,20.
impl_into_tuple_for_recursive_type_all! {
    A,B; A,B,C; A,B,C,D; A,B,C,D,E; A,B,C,D,E,F; A,B,C,D,E,F,G; A,B,C,D,E,F,G,H;
    A,B,C,D,E,F,G,H,I,J,K,L; A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P; A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T;
}*/


// impl<AsTuple, Tuple> IntoNonRecursive for AsTuple where AsTuple: IntoTuple<Tuple> {
//     type NonRecursive = Tuple;
//     fn into_friendlier(self) -> Self::NonRecursive { self.into_tuple() }
// }

// TODO use macro!
impl IntoNonRecursive for NoneMore { type NonRecursive = (); fn into_non_recursive(self) -> Self::NonRecursive { () } }
impl<A> IntoNonRecursive for Recursive<NoneMore, A> { type NonRecursive = A; fn into_non_recursive(self) -> Self::NonRecursive { self.value } }
impl<A,B> IntoNonRecursive for Recursive<Recursive<NoneMore, A>, B> { type NonRecursive = (A, B); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C> IntoNonRecursive for Recursive<Recursive<Recursive<NoneMore, A>, B>, C> { type NonRecursive = (A, B, C); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D> { type NonRecursive = (A, B, C, D); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E> { type NonRecursive = (A, B, C, D, E); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E,F> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F> { type NonRecursive = (A, B, C, D, E, F); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E,F,G> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G> { type NonRecursive = (A, B, C, D, E, F, G); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }
impl<A,B,C,D,E,F,G,H> IntoNonRecursive for Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<Recursive<NoneMore, A>, B>, C>, D>, E>, F>, G>, H> { type NonRecursive = (A, B, C, D, E, F, G, H); fn into_non_recursive(self) -> Self::NonRecursive { self.into_tuple() } }


#[derive(Copy, Clone, Debug)]
pub struct CollectPixels<ReadChannels, Pixel, PixelStorage, CreatePixels, SetPixel> {
    read_channels: ReadChannels,
    create_pixels: CreatePixels,
    set_pixel: SetPixel,
    px: PhantomData<(Pixel, PixelStorage)>,
}

impl<'s, InnerChannels, Pixel, PixelStorage, CreatePixels, SetPixel: 's>
ReadChannels<'s> for CollectPixels<InnerChannels, Pixel, PixelStorage, CreatePixels, SetPixel>
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
                let updated_samples = pixels.map(|pixel|{
                    pixel.value = Sample::from_f32(f32::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                    &mut pixel.inner
                });

                self.inner.read_pixels(bytes, updated_samples);
            },

            SampleType::U32 => {
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
                        let updated_samples = pixels.map(|pixel|{
                            pixel.value = Sample::from_f16(f16::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                            &mut pixel.inner
                        });

                        self.inner.read_pixels(bytes, updated_samples);
                    },

                    SampleType::F32 => {
                        let updated_samples = pixels.map(|pixel|{
                            pixel.value = Sample::from_f32(f32::read(&mut own_bytes_reader).expect("invalid byte slice in read pixels (bug)"));
                            &mut pixel.inner
                        });

                        self.inner.read_pixels(bytes, updated_samples);
                    },

                    SampleType::U32 => {
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