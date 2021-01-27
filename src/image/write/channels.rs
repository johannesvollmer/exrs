//! How to read arbitrary channels and rgb channels.
// TODO this module can be simplified A LOT by using SmallVec<Sample> objects, which is anyways how it works,
// TODO as the internal sample type always differs from the user-specified concrete type

use crate::meta::attribute::{LevelMode, ChannelDescription, SampleType, ChannelList, Text};
use smallvec::SmallVec;
use crate::meta::header::Header;
use crate::block::{BlockIndex, UncompressedBlock};
use crate::image::{AnyChannels, SpecificChannels};
use crate::math::{Vec2, RoundingMode};
use crate::io::{Data};
use crate::block::samples::Sample;
use crate::image::write::samples::{WritableSamples, SamplesWriter};
use crate::error::UnitResult;

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
    Channels: 'c + WritableChannelsDescription<Storage::Pixel>,
    Storage: 'c + GetPixel
{
    fn infer_channel_list(&self) -> ChannelList {
        let mut vec = self.channels.channel_descriptions_list();
        vec.sort_by_key(|channel:&ChannelDescription| channel.name.clone()); // TODO no clone?
        ChannelList::new(vec)
    }

    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        (LevelMode::Singular, RoundingMode::Down) // TODO
    }

    type Writer = SpecificChannelsWriter<
        'c,
        <Channels::PixelsWriterBuilder as PixelsWriterBuilder<Storage::Pixel>>::CreatePixelsWriterForWidth,
        Storage,
        Channels
    >;

    fn create_writer(&'c self, header: &Header) -> Self::Writer {
        let mut writer_builder = self.channels.pixel_writer_builder(); // (None, None, None, None);

        // this loop is required because the channels in the header are sorted
        // and the channels specified by the user are probably not.

        // the resulting tuple will have non-increasing start indices from first to last tuple element
        let mut byte_offset = 0;
        for channel in &header.channels.list {
            writer_builder.with_channel(&channel, byte_offset);
            byte_offset += channel.sample_type.bytes_per_sample();
        }

        let pixel_writer = writer_builder.build_width_aware_pixel_writer();

        SpecificChannelsWriter {
            channels: self,
            width_aware_pixel_writer: pixel_writer,
            // px: Default::default()
        }
    }
}



/// A temporary writer for a layer of rgba channels, alpha being optional
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SpecificChannelsWriter<'channels, PixelWriter, Storage, Channels> {
    channels: &'channels SpecificChannels<Storage, Channels>, // TODO this need not be a reference?? impl writer for specific_channels directly?
    width_aware_pixel_writer: PixelWriter,
    // px: Px,
}


impl<'channels, PxWriter, Storage, Channels> ChannelsWriter
for SpecificChannelsWriter<'channels, PxWriter, Storage, Channels>
    where
        PxWriter: CreatePixelsWriterForWidth<Storage::Pixel>,
        Storage: GetPixel,
        Channels: Sync
{
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex) -> Vec<u8> {
        let block_bytes = block_index.pixel_size.area() * header.channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; block_bytes];

        let width = block_index.pixel_size.0;
        let line_bytes = width * header.channels.bytes_per_pixel;
        let byte_lines = block_bytes.chunks_exact_mut(line_bytes);
        assert_eq!(byte_lines.len(), block_index.pixel_size.height());

        let pixel_writer_for_width = self
            .width_aware_pixel_writer.pixel_writer_for_line_width(width);

        for (y, line_bytes) in byte_lines.enumerate() {
            let mut pixel_writer = pixel_writer_for_width.clone();

            for x in 0..width {
                let position = block_index.pixel_position + Vec2(x,y);
                let pixel = self.channels.storage.get_pixel(position);
                pixel_writer.write_pixel(line_bytes, pixel);
            }
        }

        block_bytes
    }
}


pub trait WritableChannelsDescription<Pixel>: Sync {
    type PixelsWriterBuilder: PixelsWriterBuilder<Pixel>;
    fn pixel_writer_builder(&self) -> Self::PixelsWriterBuilder;
    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]>;
}

pub trait WritableChannelDescription: Sync {
    type SampleWriterBuilder: SampleWriterBuilder;
    fn sample_writer_builder(&self) -> Self::SampleWriterBuilder;
    fn channel_description(&self) -> Option<&ChannelDescription>;
}

pub trait PixelsWriterBuilder<Pixel> {
    type CreatePixelsWriterForWidth: CreatePixelsWriterForWidth<Pixel>;
    fn with_channel(&mut self, channel: &ChannelDescription, byte_offset: usize);
    fn build_width_aware_pixel_writer(self) -> Self::CreatePixelsWriterForWidth;
}

pub trait SampleWriterBuilder {
    type CreateSampleWriterForWidth: CreateSampleWriterForWidth;
    fn visit_channel(&mut self, channel: &ChannelDescription, byte_offset: usize);
    fn build_width_aware_sample_writer(self) -> Self::CreateSampleWriterForWidth;
}


impl<A,B,C,D, L,M,N,O> WritableChannelsDescription<(A, B, C, D)> for (L, M, N, O)
    where L: WritableChannelDescription, M: WritableChannelDescription, N: WritableChannelDescription, O: WritableChannelDescription,
          A: Into<Sample>, B: Into<Sample>, C: Into<Sample>, D: Into<Sample>,
{
    type PixelsWriterBuilder = (L::SampleWriterBuilder, M::SampleWriterBuilder, N::SampleWriterBuilder, O::SampleWriterBuilder, );

    fn pixel_writer_builder(&self) -> Self::PixelsWriterBuilder {
        (
            self.0.sample_writer_builder(),
            self.1.sample_writer_builder(),
            self.2.sample_writer_builder(),
            self.3.sample_writer_builder(),
        )
    }

    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]> {
        [
            self.0.channel_description(),
            self.1.channel_description(),
            self.2.channel_description(),
            self.3.channel_description(),
        ]
            .iter()
            .flatten()
            .map(|&chan| chan.clone())
            .collect()
    }
}

impl<A,B,C,D, L,M,N,O> PixelsWriterBuilder<(A, B, C, D)> for (L,M,N,O)
    where L: SampleWriterBuilder, M: SampleWriterBuilder, N: SampleWriterBuilder, O: SampleWriterBuilder,
        A: Into<Sample>, B: Into<Sample>, C: Into<Sample>, D: Into<Sample>,
{
    type CreatePixelsWriterForWidth = (L::CreateSampleWriterForWidth, M::CreateSampleWriterForWidth, N::CreateSampleWriterForWidth, O::CreateSampleWriterForWidth);

    fn with_channel(&mut self, channel: &ChannelDescription, byte_offset: usize) {
        self.0.visit_channel(channel, byte_offset);
        self.1.visit_channel(channel, byte_offset);
        self.2.visit_channel(channel, byte_offset);
        self.3.visit_channel(channel, byte_offset);
    }

    fn build_width_aware_pixel_writer(self) -> Self::CreatePixelsWriterForWidth {
        (
            self.0.build_width_aware_sample_writer(),
            self.1.build_width_aware_sample_writer(),
            self.2.build_width_aware_sample_writer(),
            self.3.build_width_aware_sample_writer(),
        )
    }
}


impl WritableChannelDescription for ChannelDescription {
    type SampleWriterBuilder = AlwaysSampleWriterBuilder;
    fn sample_writer_builder(&self) -> Self::SampleWriterBuilder {
        AlwaysSampleWriterBuilder {
            desired_channel_name: self.name.clone(),
            found_channel: None
        }
    }

    fn channel_description(&self) -> Option<&ChannelDescription> { Some(self) }
}

impl WritableChannelDescription for Option<ChannelDescription> {
    type SampleWriterBuilder = Option<AlwaysSampleWriterBuilder>;
    fn sample_writer_builder(&self) -> Self::SampleWriterBuilder {
        self.as_ref().map(|channel| channel.sample_writer_builder())
    }

    fn channel_description(&self) -> Option<&ChannelDescription> { self.as_ref() }
}

#[derive(Debug)]
pub struct AlwaysSampleWriterBuilder {
    desired_channel_name: Text,
    found_channel: Option<AlwaysCreateSampleWriterForWidth>
}

impl SampleWriterBuilder for AlwaysSampleWriterBuilder {
    type CreateSampleWriterForWidth = AlwaysCreateSampleWriterForWidth;

    fn visit_channel(&mut self, channel: &ChannelDescription, byte_offset: usize) {
        if self.desired_channel_name == channel.name {
            self.found_channel = Some(AlwaysCreateSampleWriterForWidth {
                target_sample_type: channel.sample_type,
                start_byte_offset: byte_offset
            })
        }
    }

    fn build_width_aware_sample_writer(self) -> Self::CreateSampleWriterForWidth {
        self.found_channel.expect("channel has not been extracted properly (bug)")
    }
}

impl SampleWriterBuilder for Option<AlwaysSampleWriterBuilder> {
    type CreateSampleWriterForWidth = Option<AlwaysCreateSampleWriterForWidth>;

    fn visit_channel(&mut self, channel: &ChannelDescription, byte_offset: usize) {
        if let Some(this) = self { this.visit_channel(channel, byte_offset) }
    }

    fn build_width_aware_sample_writer(self) -> Self::CreateSampleWriterForWidth {
        self.map(|s| s.build_width_aware_sample_writer())
    }
}


pub trait CreatePixelsWriterForWidth<Pixel>: Sync {
    type PixelWriter: Clone + PixelWriter<Pixel>;
    fn pixel_writer_for_line_width(&self, width: usize) -> Self::PixelWriter;
}

pub trait CreateSampleWriterForWidth: Sync {
    type SampleWriter: SampleWriter;
    fn sample_writer_for_width(&self, width: usize) -> Self::SampleWriter;
}

// TODO no need to separate PixelsWriter and PixelLineWriter?
pub trait PixelWriter<Pixel> {
    fn write_pixel(&mut self, whole_line: &mut [u8], pixel: Pixel);
}

pub trait SampleWriter: Clone {
    fn write_next_sample<T>(&mut self, line: &mut [u8], sample: T) -> UnitResult where T: Into<Sample>;
}

// TODO redundant structs?
#[derive(Clone, Copy, Debug)]
pub struct AlwaysCreateSampleWriterForWidth {
    // px: PhantomData<T>,
    target_sample_type: SampleType,
    start_byte_offset: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct AlwaysSampleWriter {
    // px: PhantomData<T>,
    target_sample_type: SampleType,
    next_byte_index: usize,
}

impl CreateSampleWriterForWidth for AlwaysCreateSampleWriterForWidth {
    type SampleWriter = AlwaysSampleWriter;
    fn sample_writer_for_width(&self, width: usize) -> AlwaysSampleWriter {
        AlwaysSampleWriter {
            next_byte_index: self.start_byte_offset * width,
            target_sample_type: self.target_sample_type
        }
    }
}

impl CreateSampleWriterForWidth for Option<AlwaysCreateSampleWriterForWidth> {
    type SampleWriter = Option<AlwaysSampleWriter>;

    fn sample_writer_for_width(&self, width: usize) -> Self::SampleWriter {
        self.map(|default_writer| default_writer.sample_writer_for_width(width))
    }
}

impl SampleWriter for AlwaysSampleWriter {
    fn write_next_sample<T>(&mut self, line: &mut [u8], sample: T) -> UnitResult where T: Into<Sample> {
        let index = self.next_byte_index.min(line.len()); // required for index out of bounds error
        self.next_byte_index += self.target_sample_type.bytes_per_sample();
        let bytes = &mut &mut line[index ..];

        // TODO not match so many times!
        match self.target_sample_type {
            SampleType::F16 => sample.into().to_f16().write(bytes)?, // TODO expect?
            SampleType::F32 => sample.into().to_f32().write(bytes)?,
            SampleType::U32 => sample.into().to_u32().write(bytes)?,
        }

        Ok(())
    }
}

// Note: If the channels info is Some, but no sample is provided,
// the default value is picked, because of `Sample::from(Option<impl IntoSample>)`.
// If the channels info is None, but the value is provided, it is ignored inside this trait implementation.
impl SampleWriter for Option<AlwaysSampleWriter> {
    fn write_next_sample<T>(&mut self, line: &mut [u8], sample: T) -> UnitResult where T: Into<Sample> {
        if let Some(this) = self { this.write_next_sample(line, sample)?; }
        Ok(())
    }
}

impl<A,B,C,D, L, M, N, O> CreatePixelsWriterForWidth<(A, B, C, D)> for (L, M, N, O)
    where A: Into<Sample>, B: Into<Sample>, C: Into<Sample>, D: Into<Sample>,
          L: CreateSampleWriterForWidth, M: CreateSampleWriterForWidth, N: CreateSampleWriterForWidth, O: CreateSampleWriterForWidth,
{
    type PixelWriter = (L::SampleWriter, M::SampleWriter, N::SampleWriter, O::SampleWriter);

    fn pixel_writer_for_line_width(&self, width: usize) -> Self::PixelWriter {
        (
            self.0.sample_writer_for_width(width),
            self.1.sample_writer_for_width(width),
            self.2.sample_writer_for_width(width),
            self.3.sample_writer_for_width(width),
        )
    }
}

impl<A,B,C,D, L, M, N, O> PixelWriter<(A, B, C, D)> for (L, M, N, O)
    where A: Into<Sample>, B: Into<Sample>, C: Into<Sample>, D: Into<Sample>,
          L: SampleWriter, M: SampleWriter, N: SampleWriter, O: SampleWriter,
{
    fn write_pixel(&mut self, whole_line: &mut [u8], pixel: (A, B, C, D)) {
        self.0.write_next_sample(whole_line, pixel.0).expect("failed in memory write"); // order does not really matter, as these start at independent points in time
        self.1.write_next_sample(whole_line, pixel.1).expect("failed in memory write");
        self.2.write_next_sample(whole_line, pixel.2).expect("failed in memory write");
        self.3.write_next_sample(whole_line, pixel.3).expect("failed in memory write");
    }
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
            SpecificChannels::named(("R", "G", "B", "A"), |_pos| px)
        );

        assert_is_writable_channels(SpecificChannels::named(
            ("R", "G", "B", "A"),
            PixelVec::new((3, 2), vec![px, px, px, px, px, px])
        ));

        let px = (3_f32, f16::ONE, Option::<f16>::None, Some(4_f32));
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




