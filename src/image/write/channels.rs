//! How to read arbitrary channels and rgb channels.
// TODO this module can be simplified A LOT by using SmallVec<Sample> objects, which is anyways how it works,
// TODO as the internal sample type always differs from the user-specified concrete type

use crate::meta::attribute::{LevelMode, ChannelDescription, SampleType, ChannelList};
use smallvec::SmallVec;
use crate::meta::header::Header;
use crate::block::{BlockIndex, UncompressedBlock};
use crate::image::{AnyChannels, SpecificChannels};
use crate::math::{Vec2, RoundingMode};
use crate::io::{Data};
use crate::block::samples::Sample;
use crate::image::write::samples::{WritableSamples, SamplesWriter};
use std::marker::PhantomData;
use crate::prelude::f16;
use crate::prelude::read::specific_channels::FromNativeSample;
use crate::image::recursive::*;

// TODO TupleChannelsWriter: Fn(Vec2<usize>) -> impl IntoSamples, where IntoSamples is implemented for tuples, inferring the channel type

/// Enables an image containing this list of channels to be written to a file.
pub trait WritableChannels<'slf> {

    /// Generate the file meta data for this list of channel
    fn infer_channel_list(&self) -> ChannelList;

    ///  Generate the file meta data of whether and how resolution levels should be stored in the file
    fn infer_level_modes(&self) -> (LevelMode, RoundingMode);

    /// The type of temporary writer
    type Writer: ChannelsWriter;

    /// Create a temporary writer for this list of channels
    fn create_writer(&'slf self, header: &Header) -> Self::Writer;
}

/// A temporary writer for a list of channels
pub trait ChannelsWriter: Sync {

    /// Deliver a block of pixels, containing all channel data, to be stored in the file
    fn extract_uncompressed_block(&self, header: &Header, block: BlockIndex) -> Vec<u8>; // TODO return uncompressed block?
}


/// Define how to get an rgba pixel from your custom pixel storage.
/// Can be a closure of type [`Sync + Fn(Vec2<usize>) -> RgbaPixel`].
pub trait GetPixel: Sync {
    type Pixel;

    /// Inspect a single rgba pixel at the requested position.
    /// Will be called exactly once for each pixel in the image.
    /// The position will not exceed the image dimensions.
    /// Might be called from multiple threads at the same time.
    fn get_pixel(&self, position: Vec2<usize>) -> Self::Pixel;
}

impl<F, P> GetPixel for F where F: Sync + Fn(Vec2<usize>) -> P {
    type Pixel = P;
    fn get_pixel(&self, position: Vec2<usize>) -> P { self(position) }
}

impl<'samples, Samples> WritableChannels<'samples> for AnyChannels<Samples>
    where Samples: 'samples + WritableSamples<'samples>
{
    fn infer_channel_list(&self) -> ChannelList {
        ChannelList::new(self.list.iter().map(|channel| ChannelDescription {
            name: channel.name.clone(),
            sample_type: channel.sample_data.sample_type(),
            quantize_linearly: channel.quantize_linearly,
            sampling: channel.sampling
        }).collect())
    }

    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        let mode = self.list.iter().next().unwrap().sample_data.infer_level_modes();

        debug_assert!(
            std::iter::repeat(mode).zip(self.list.iter().skip(1))
                .all(|(first, other)| other.sample_data.infer_level_modes() == first),

            "level mode must be the same across all levels (do not nest resolution levels!)"
        );

        mode
    }

    type Writer = AnyChannelsWriter<Samples::Writer>;
    fn create_writer(&'samples self, header: &Header) -> Self::Writer {
        let channels = self.list.iter()
            .map(|chan| chan.sample_data.create_samples_writer(header))
            .collect();

        AnyChannelsWriter { channels }
    }
}

/// A temporary writer for an arbitrary list of channels
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AnyChannelsWriter<SamplesWriter> {
    channels: SmallVec<[SamplesWriter; 4]>
}

impl<Samples> ChannelsWriter for AnyChannelsWriter<Samples> where Samples: SamplesWriter {
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex) -> Vec<u8> {
        UncompressedBlock::collect_block_from_lines(header, block_index, |line_ref| {
            self.channels[line_ref.location.channel].extract_line(line_ref)
        })
    }
}






impl<'c, Channels, Storage>
WritableChannels<'c> for SpecificChannels<Storage, Channels>
where
    Storage: 'c + GetPixel,
    Storage::Pixel: IntoRecursive,
    Channels: 'c + Sync + Clone + IntoRecursive,
    <Channels as IntoRecursive>::Recursive: WritableChannelsDescription<<Storage::Pixel as IntoRecursive>::Recursive>,
{
    fn infer_channel_list(&self) -> ChannelList {
        let mut vec = self.channels.clone().into_recursive().channel_descriptions_list();
        vec.sort_by_key(|channel:&ChannelDescription| channel.name.clone()); // TODO no clone?
        ChannelList::new(vec)
    }

    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        (LevelMode::Singular, RoundingMode::Down) // TODO
    }

    type Writer = SpecificChannelsWriter<
        'c,
        <<Channels as IntoRecursive>::Recursive as WritableChannelsDescription<<Storage::Pixel as IntoRecursive>::Recursive>>::RecursiveWriter,
        Storage,
        Channels
    >;

    fn create_writer(&'c self, header: &Header) -> Self::Writer {
        // this loop is required because the channels in the header are sorted
        // and the channels specified by the user are probably not.

        SpecificChannelsWriter {
            channels: self,
            recursive_channel_writer: self.channels.clone().into_recursive().create_recursive_writer(&header.channels),
        }
    }
}



/// A temporary writer for a layer of rgba channels, alpha being optional
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SpecificChannelsWriter<'channels, PixelWriter, Storage, Channels> {
    channels: &'channels SpecificChannels<Storage, Channels>, // TODO this need not be a reference?? impl writer for specific_channels directly?
    recursive_channel_writer: PixelWriter,
}


impl<'channels, PxWriter, Storage, Channels> ChannelsWriter
for SpecificChannelsWriter<'channels, PxWriter, Storage, Channels>
    where
        Channels: Sync,
        Storage: GetPixel,
        Storage::Pixel: IntoRecursive,
        PxWriter: Sync + RecursivePixelWriter<<Storage::Pixel as IntoRecursive>::Recursive>,
{
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex) -> Vec<u8> {
        let block_bytes = block_index.pixel_size.area() * header.channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; block_bytes];

        let width = block_index.pixel_size.0;
        let line_bytes = width * header.channels.bytes_per_pixel;
        let byte_lines = block_bytes.chunks_exact_mut(line_bytes);
        assert_eq!(byte_lines.len(), block_index.pixel_size.height());

        let mut pixel_line = Vec::with_capacity(width);

        for (y, line_bytes) in byte_lines.enumerate() {
            pixel_line.clear();
            pixel_line.extend((0 .. width).map(|x|
                self.channels.storage.get_pixel(block_index.pixel_position + Vec2(x,y)).into_recursive()
            ));

            self.recursive_channel_writer.write_pixels(line_bytes, pixel_line.as_slice(), |px| px);
        }

        block_bytes
    }
}


pub trait WritableChannelsDescription<Pixel>: Sync {
    type RecursiveWriter: RecursivePixelWriter<Pixel>;
    fn create_recursive_writer(&self, channels: &ChannelList) -> Self::RecursiveWriter;
    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]>;
}

impl WritableChannelsDescription<NoneMore> for NoneMore {
    type RecursiveWriter = NoneMore;
    fn create_recursive_writer(&self, _: &ChannelList) -> Self::RecursiveWriter { NoneMore }
    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]> { SmallVec::new() }
}

impl<InnerDescriptions, InnerPixel, Sample: IntoNativeSample>
    WritableChannelsDescription<Recursive<InnerPixel, Sample>>
    for Recursive<InnerDescriptions, ChannelDescription>
    where InnerDescriptions: WritableChannelsDescription<InnerPixel>
{
    type RecursiveWriter = RecursiveWriter<InnerDescriptions::RecursiveWriter, Sample>;

    fn create_recursive_writer(&self, channels: &ChannelList) -> Self::RecursiveWriter {
        // this linear lookup is required because the order of the channels changed, due to alphabetical sorting
        let (start_byte_offset, target_sample_type) = channels.channels_with_byte_offset()
            .find(|(_offset, channel)| channel.name == self.value.name)
            .map(|(offset, channel)| (offset, channel.sample_type))
            .expect("a channel has not been put into channel list");

        Recursive::new(self.inner.create_recursive_writer(channels), SampleWriter {
            start_byte_offset, target_sample_type,
            px: PhantomData::default()
        })
    }

    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]> {
        let mut inner_list = self.inner.channel_descriptions_list();
        inner_list.push(self.value.clone());
        inner_list
    }
}

impl<InnerDescriptions, InnerPixel, Sample: IntoNativeSample>
WritableChannelsDescription<Recursive<InnerPixel, Sample>>
for Recursive<InnerDescriptions, Option<ChannelDescription>>
    where InnerDescriptions: WritableChannelsDescription<InnerPixel>
{
    type RecursiveWriter = OptionalRecursiveWriter<InnerDescriptions::RecursiveWriter, Sample>;

    fn create_recursive_writer(&self, channels: &ChannelList) -> Self::RecursiveWriter {
        // this linear lookup is required because the order of the channels changed, due to alphabetical sorting
        // FIXME check for duplicates

        let channel = self.value.as_ref().map(|required_channel|
            channels.channels_with_byte_offset()
                .find(|(_offset, channel)| channel == &required_channel)
                .map(|(offset, channel)| (offset, channel.sample_type))
                .expect("a channel has not been put into channel list")
        );

        Recursive::new(
            self.inner.create_recursive_writer(channels),
            channel.map(|(start_byte_offset, target_sample_type)| SampleWriter {
                start_byte_offset, target_sample_type,
                px: PhantomData::default(),
            })
        )
    }

    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]> {
        let mut inner_list = self.inner.channel_descriptions_list();
        if let Some(value) = &self.value { inner_list.push(value.clone()); }
        inner_list
    }
}

pub trait RecursivePixelWriter<Pixel>: Sync {
    fn write_pixels<FullPixel>(&self, bytes: &mut [u8], pixels: &[FullPixel], get_pixel: impl Fn(&FullPixel) -> &Pixel);
}

type RecursiveWriter<Inner, Sample> = Recursive<Inner, SampleWriter<Sample>>;
type OptionalRecursiveWriter<Inner, Sample> = Recursive<Inner, Option<SampleWriter<Sample>>>;

#[derive(Debug, Clone)]
pub struct SampleWriter<Sample> {
    target_sample_type: SampleType,
    start_byte_offset: usize,
    px: PhantomData<Sample>,
}

impl RecursivePixelWriter<NoneMore> for NoneMore {
    fn write_pixels<FullPixel>(&self, _: &mut [u8], _: &[FullPixel], _: impl Fn(&FullPixel) -> &NoneMore) {}
}

impl<Inner, InnerPixel, Sample: IntoNativeSample>
    RecursivePixelWriter<Recursive<InnerPixel, Sample>>
    for RecursiveWriter<Inner, Sample>
    where Inner: RecursivePixelWriter<InnerPixel>
{
    fn write_pixels<FullPixel>(&self, bytes: &mut [u8], pixels: &[FullPixel], get_pixel: impl Fn(&FullPixel) -> &Recursive<InnerPixel, Sample>){
        let byte_start_index = self.value.start_byte_offset * self.value.target_sample_type.bytes_per_sample();
        let byte_count = pixels.len() * self.value.target_sample_type.bytes_per_sample();
        let ref mut byte_writer = &mut bytes[byte_start_index .. byte_start_index + byte_count];

        // match outside the loop to avoid matching on every single sample
        // TODO dedup with below
        match self.value.target_sample_type {
            SampleType::F16 => {
                for pixel in pixels {
                    get_pixel(pixel).value.to_f16().write(byte_writer).expect("memory buffer invalid length when writing");
                }

                self.inner.write_pixels(bytes, pixels, |px| &get_pixel(px).inner);
            },
            SampleType::F32 => {
                for pixel in pixels {
                    get_pixel(pixel).value.to_f32().write(byte_writer).expect("memory buffer invalid length when writing");
                }

                self.inner.write_pixels(bytes, pixels, |px| &get_pixel(px).inner);
            },
            SampleType::U32 => {
                for pixel in pixels {
                    get_pixel(pixel).value.to_u32().write(byte_writer).expect("memory buffer invalid length when writing");
                }

                self.inner.write_pixels(bytes, pixels, |px| &get_pixel(px).inner);
            },
        }
    }
}

impl<Inner, InnerPixel, Sample> RecursivePixelWriter<Recursive<InnerPixel, Sample>>
    for OptionalRecursiveWriter<Inner, Sample>
    where Inner: RecursivePixelWriter<InnerPixel>,
        Sample: IntoNativeSample
{
    fn write_pixels<FullPixel>(&self, bytes: &mut [u8], pixels: &[FullPixel], get_pixel: impl Fn(&FullPixel) -> &Recursive<InnerPixel, Sample>) {
        if let Some(writer) = &self.value {
            let byte_start_index = writer.start_byte_offset * writer.target_sample_type.bytes_per_sample();
            let byte_count = pixels.len() * writer.target_sample_type.bytes_per_sample();
            let ref mut byte_writer = &mut bytes[byte_start_index .. byte_start_index + byte_count]; // TODO this might panic!

            // match outside the loop to avoid matching on every single sample
            match writer.target_sample_type {
                SampleType::F16 => {
                    for pixel in pixels {
                        get_pixel(pixel).value.to_f16().write(byte_writer).expect("memory buffer invalid length when writing");
                    }

                    self.inner.write_pixels(bytes, pixels, |px| &get_pixel(px).inner);
                },
                SampleType::F32 => {
                    for pixel in pixels {
                        get_pixel(pixel).value.to_f32().write(byte_writer).expect("memory buffer invalid length when writing");
                    }

                    self.inner.write_pixels(bytes, pixels, |px| &get_pixel(px).inner);
                },
                SampleType::U32 => {
                    for pixel in pixels {
                        get_pixel(pixel).value.to_u32().write(byte_writer).expect("memory buffer invalid length when writing");
                    }

                    self.inner.write_pixels(bytes, pixels, |px| &get_pixel(px).inner);
                },
            }
        }
        else {
            self.inner.write_pixels(bytes, pixels, |px| &get_pixel(px).inner);
        }
    }
}






pub trait IntoNativeSample: Copy + Default + Sync + 'static {
    fn to_f16(&self) -> f16;
    fn to_f32(&self) -> f32;
    fn to_u32(&self) -> u32;
}

impl IntoNativeSample for f16 {
    fn to_f16(&self) -> f16 { f16::from_f16(*self) }
    fn to_f32(&self) -> f32 { f32::from_f16(*self) }
    fn to_u32(&self) -> u32 { u32::from_f16(*self) }
}

impl IntoNativeSample for f32 {
    fn to_f16(&self) -> f16 { f16::from_f32(*self) }
    fn to_f32(&self) -> f32 { f32::from_f32(*self) }
    fn to_u32(&self) -> u32 { u32::from_f32(*self) }
}

impl IntoNativeSample for u32 {
    fn to_f16(&self) -> f16 { f16::from_u32(*self) }
    fn to_f32(&self) -> f32 { f32::from_u32(*self) }
    fn to_u32(&self) -> u32 { u32::from_u32(*self) }
}

impl IntoNativeSample for Sample {
    fn to_f16(&self) -> f16 { Sample::to_f16(*self) }
    fn to_f32(&self) -> f32 { Sample::to_f32(*self) }
    fn to_u32(&self) -> u32 { Sample::to_u32(*self) }
}






#[cfg(test)]
pub mod test {
    use crate::image::write::channels::WritableChannels;
    use crate::image::SpecificChannels;
    use crate::prelude::{f16};
    use crate::meta::attribute::{ChannelDescription, SampleType};
    use crate::image::pixel_vec::PixelVec;

    #[test]
    fn compiles(){
        let x = 3_f32;
        let y = f16::from_f32(4.0);
        let z = 2_u32;
        let s = 1.3_f32;
        let px = (x,y,z,s);

        assert_is_writable_channels(
            SpecificChannels::rgba(|_pos| px)
        );

        assert_is_writable_channels(SpecificChannels::rgba(
            PixelVec::new((3, 2), vec![px, px, px, px, px, px])
        ));

        let px = (2333_u32, 4_f32);
        assert_is_writable_channels(
            SpecificChannels::build()
                .with_named_channel("A")
                .with_named_channel("C")
                .with_pixels(PixelVec::new((3, 2), vec![px, px, px, px, px, px]))
        );

        let px = (3_f32, f16::ONE, 2333_u32, 4_f32);
        assert_is_writable_channels(SpecificChannels::new(
            (
                ChannelDescription::named("x", SampleType::F32),
                ChannelDescription::named("y", SampleType::F16),
                Some(ChannelDescription::named("z", SampleType::U32)),
                Some(ChannelDescription::named("p", SampleType::F32)),
            ),

            PixelVec::new((3, 2), vec![px, px, px, px, px, px])
        ));



        fn assert_is_writable_channels<'s>(_channels: impl WritableChannels<'s>){}

    }
}




