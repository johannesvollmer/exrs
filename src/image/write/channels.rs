//! How to read arbitrary channels and rgb channels.

use crate::meta::attribute::{LevelMode, ChannelInfo, SampleType, ChannelList, Text};
use smallvec::SmallVec;
use crate::meta::header::Header;
use crate::block::{BlockIndex, UncompressedBlock};
use crate::image::{AnyChannels, SpecificChannels};
use crate::math::{Vec2, RoundingMode};
use crate::io::{Data};
use crate::block::samples::Sample;
use crate::image::write::samples::{WritableSamples, SamplesWriter};
use crate::prelude::{f16};
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
        ChannelList::new(self.list.iter().map(|channel| ChannelInfo {
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
        /*let byte_count = block_index.pixel_size.area() * header.channels.bytes_per_pixel;
        let mut block_bytes = vec![0_u8; byte_count];

        for (byte_range, line_index) in LineIndex::lines_in_block(block_index, header) {
            self.channels.get(line_index.channel).unwrap().extract_line(LineRefMut { // TODO subsampling
                value: &mut block_bytes[byte_range],
                location: line_index,
            });
        }

        block_bytes*/
        UncompressedBlock::collect_block_from_lines(header, block_index, |line_ref| {
            self.channels[line_ref.location.channel].extract_line(line_ref)
        })
    }
}



pub trait IntoSample: Into<Sample> { const SAMPLE_TYPE: SampleType; }
impl IntoSample for f16 { const SAMPLE_TYPE: SampleType = SampleType::F16; }
impl IntoSample for f32 { const SAMPLE_TYPE: SampleType = SampleType::F32; }
impl IntoSample for u32 { const SAMPLE_TYPE: SampleType = SampleType::U32; }
// impl IntoSample for Sample { const SAMPLE_TYPE: SampleType = Sample:; }

impl<'c, Channels: 'c, Storage: 'c + GetPixel>
WritableChannels<'c> for SpecificChannels<Storage, Channels>
    // where Self::Writer : ChannelsWriter //SpecificChannelsWriter<'c, Px, Storage, Channels>: ChannelsWriter // Pixels: GetPixel<(A,B,C)>, A: IntoSample, B: IntoSample, C: IntoSample
where
    // A: Into<Sample>, B: Into<Sample>, C: Into<Sample>, D: Into<Sample>,
    Channels: WritableChannelsInfo<Storage::Pixel>
{
    fn infer_channel_list(&self) -> ChannelList {
        // let mut vec = smallvec![ self.channels.0.clone(), self.channels.1.clone(), self.channels.2.clone(), self.channels.3.clone()  ];
        let mut vec = self.channels.channel_info_list();
        vec.sort_by_key(|channel:&ChannelInfo| channel.name.clone()); // TODO no clone?
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
            // if channel.name == self.channels.0.name { byte_offsets.0 = Some(byte_offset); }
            // if channel.name == self.channels.1.name { byte_offsets.1 = Some(byte_offset); }
            // if channel.name == self.channels.2.name { byte_offsets.2 = Some(byte_offset); }
            // if channel.name == self.channels.3.name { byte_offsets.3 = Some(byte_offset); }
            writer_builder.with_channel(&channel, byte_offset);
            byte_offset += channel.sample_type.bytes_per_sample();
        }

        /*let pixel_writer = (
            DefaultChannelWriter {
                start_byte_offset: byte_offsets.0.expect("internal channel mismatch"),
                target_sample_type: self.channels.0.sample_type
            },
            DefaultChannelWriter {
                start_byte_offset: byte_offsets.1.expect("internal channel mismatch"),
                target_sample_type: self.channels.1.sample_type
            },
            DefaultChannelWriter {
                start_byte_offset: byte_offsets.2.expect("internal channel mismatch"),
                target_sample_type: self.channels.2.sample_type
            },
            DefaultChannelWriter {
                start_byte_offset: byte_offsets.3.expect("internal channel mismatch"),
                target_sample_type: self.channels.3.sample_type
            },
        );*/
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

        /*// alpha would always start at 0, then comes b, g, r.
        let RgbaSampleTypes(r_type, g_type, b_type, a_type) = self.rgba.sample_types;
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
            debug_assert!(line_bytes.is_empty(), "some bytes are left after dividing input for rgba channels");

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
                let pixel: RgbaPixel = self.rgba.storage.get_pixel(position).into();

                write_r(pixel.red);
                write_g(pixel.green);
                write_b(pixel.blue);

                if let Some(write_a) = &mut write_a {
                    write_a(pixel.alpha_or_1()); // no alpha channel provided = not transparent
                }
            }
        }

        block_bytes*/
    }
}


pub trait WritableChannelsInfo<Pixel>: Sync {
    type PixelsWriterBuilder: PixelsWriterBuilder<Pixel>;
    fn pixel_writer_builder(&self) -> Self::PixelsWriterBuilder;
    fn channel_info_list(&self) -> SmallVec<[ChannelInfo; 5]>;
}

pub trait WritableChannelInfo: Sync {
    type SampleWriterBuilder: SampleWriterBuilder;
    fn sample_writer_builder(&self) -> Self::SampleWriterBuilder;
    fn channel_info(&self) -> Option<&ChannelInfo>;
}

pub trait PixelsWriterBuilder<Pixel> {
    type CreatePixelsWriterForWidth: CreatePixelsWriterForWidth<Pixel>;
    fn with_channel(&mut self, channel: &ChannelInfo, byte_offset: usize);
    fn build_width_aware_pixel_writer(self) -> Self::CreatePixelsWriterForWidth;
}

pub trait SampleWriterBuilder {
    type CreateSampleWriterForWidth: CreateSampleWriterForWidth;
    fn visit_channel(&mut self, channel: &ChannelInfo, byte_offset: usize);
    fn build_width_aware_sample_writer(self) -> Self::CreateSampleWriterForWidth;
}


impl<A,B,C,D, L,M,N,O> WritableChannelsInfo<(A, B, C, D)> for (L,M,N,O)
    where L: WritableChannelInfo, M: WritableChannelInfo, N: WritableChannelInfo, O: WritableChannelInfo,
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

    fn channel_info_list(&self) -> SmallVec<[ChannelInfo; 5]> {
        [
            self.0.channel_info(),
            self.1.channel_info(),
            self.2.channel_info(),
            self.3.channel_info(),
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

    fn with_channel(&mut self, channel: &ChannelInfo, byte_offset: usize) {
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


impl WritableChannelInfo for ChannelInfo {
    type SampleWriterBuilder = AlwaysSampleWriterBuilder;
    fn sample_writer_builder(&self) -> Self::SampleWriterBuilder {
        AlwaysSampleWriterBuilder {
            desired_channel_name: self.name.clone(),
            found_channel: None
        }
    }

    fn channel_info(&self) -> Option<&ChannelInfo> { Some(self) }
}

impl WritableChannelInfo for Option<ChannelInfo> {
    type SampleWriterBuilder = Option<AlwaysSampleWriterBuilder>;
    fn sample_writer_builder(&self) -> Self::SampleWriterBuilder {
        self.as_ref().map(|info| info.sample_writer_builder())
    }

    fn channel_info(&self) -> Option<&ChannelInfo> { self.as_ref() }
}

pub struct AlwaysSampleWriterBuilder {
    desired_channel_name: Text,
    found_channel: Option<AlwaysCreateSampleWriterForWidth>
}

impl SampleWriterBuilder for AlwaysSampleWriterBuilder {
    type CreateSampleWriterForWidth = AlwaysCreateSampleWriterForWidth;

    fn visit_channel(&mut self, channel: &ChannelInfo, byte_offset: usize) {
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

    fn visit_channel(&mut self, channel: &ChannelInfo, byte_offset: usize) {
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

impl SampleWriter for Option<AlwaysSampleWriter> {
    fn write_next_sample<T>(&mut self, line: &mut [u8], sample: T) -> UnitResult where T: Into<Sample> {
        if let Some(this) = self { this.write_next_sample(line, sample)?; }
        Ok(())
    }
}

impl<A,B,C,D, WA,WB,WC,WD> CreatePixelsWriterForWidth<(A, B, C, D)> for (WA, WB, WC, WD)
    where A: Into<Sample>, B: Into<Sample>, C: Into<Sample>, D: Into<Sample>,
          WA: CreateSampleWriterForWidth, WB: CreateSampleWriterForWidth, WC: CreateSampleWriterForWidth, WD: CreateSampleWriterForWidth,
{
    type PixelWriter = (WA::SampleWriter, WB::SampleWriter, WC::SampleWriter, WD::SampleWriter);

    fn pixel_writer_for_line_width(&self, width: usize) -> Self::PixelWriter {
        (
            self.0.sample_writer_for_width(width),
            self.1.sample_writer_for_width(width),
            self.2.sample_writer_for_width(width),
            self.3.sample_writer_for_width(width),
        )
    }
}

impl<A,B,C,D, WA,WB,WC,WD> PixelWriter<(A, B, C, D)> for (WA, WB, WC, WD)
    where A: Into<Sample>, B: Into<Sample>, C: Into<Sample>, D: Into<Sample>,
          WA: SampleWriter, WB: SampleWriter, WC: SampleWriter, WD: SampleWriter,
{
    fn write_pixel(&mut self, whole_line: &mut [u8], pixel: (A, B, C, D)) {
        self.0.write_next_sample(whole_line, pixel.0).expect("failed in memory write"); // order does not really matter, as these start at independent points in time
        self.1.write_next_sample(whole_line, pixel.1).expect("failed in memory write");
        self.2.write_next_sample(whole_line, pixel.2).expect("failed in memory write");
        self.3.write_next_sample(whole_line, pixel.3).expect("failed in memory write");
    }
}

























