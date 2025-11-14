//! How to read arbitrary channels and rgb channels.

use crate::block::lines::LineRefMut;
use crate::block::samples::*;
use crate::block::*;
use crate::image::recursive::*;
use crate::image::write::samples::*;
use crate::io::*;
use crate::math::*;
use crate::meta::{attribute::*, header::*};
use crate::prelude::*;

use std::io::Cursor;
use std::marker::PhantomData;

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

/// Define how to get a pixel from your custom pixel storage.
/// Can be a closure of type [`Sync + Fn(Vec2<usize>) -> YourPixel`].
pub trait GetPixel: Sync {
    /// The pixel tuple containing `f32`, `f16`, `u32` and `Sample` values.
    /// The length of the tuple must match the number of channels in the image.
    type Pixel;

    /// Inspect a single pixel at the requested position.
    /// Will be called exactly once for each pixel in the image.
    /// The position will not exceed the image dimensions.
    /// Might be called from multiple threads at the same time.
    fn get_pixel(&self, position: Vec2<usize>) -> Self::Pixel;
}

impl<F, P> GetPixel for F
where
    F: Sync + Fn(Vec2<usize>) -> P,
{
    type Pixel = P;
    fn get_pixel(&self, position: Vec2<usize>) -> P {
        self(position)
    }
}

impl<'samples, Samples> WritableChannels<'samples> for AnyChannels<Samples>
where
    Samples: 'samples + WritableSamples<'samples>,
{
    fn infer_channel_list(&self) -> ChannelList {
        ChannelList::new(
            self.list
                .iter()
                .map(|channel| ChannelDescription {
                    name: channel.name.clone(),
                    sample_type: channel.sample_data.sample_type(),
                    quantize_linearly: channel.quantize_linearly,
                    sampling: channel.sampling,
                })
                .collect(),
        )
    }

    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        let mode = self
            .list
            .iter()
            .next()
            .expect("zero channels in list")
            .sample_data
            .infer_level_modes();

        debug_assert!(
            std::iter::repeat(mode)
                .zip(self.list.iter().skip(1))
                .all(|(first, other)| other.sample_data.infer_level_modes() == first),
            "level mode must be the same across all levels (do not nest resolution levels!)"
        );

        mode
    }

    type Writer = AnyChannelsWriter<Samples::Writer>;
    fn create_writer(&'samples self, header: &Header) -> Self::Writer {
        let channels = self
            .list
            .iter()
            .map(|chan| {
                chan.sample_data
                    .create_samples_writer(header, chan.sampling)
            })
            .collect();

        AnyChannelsWriter { channels }
    }
}

/// A temporary writer for an arbitrary list of channels
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AnyChannelsWriter<SamplesWriter> {
    channels: SmallVec<[SamplesWriter; 4]>,
}

impl<Samples> ChannelsWriter for AnyChannelsWriter<Samples>
where
    Samples: SamplesWriter,
{
    fn extract_uncompressed_block(&self, header: &Header, block_index: BlockIndex) -> Vec<u8> {
        UncompressedBlock::collect_block_data_from_lines(
            &header.channels,
            block_index,
            |line_ref| self.channels[line_ref.location.channel].extract_line(line_ref),
        )
    }
}

impl<'c, Channels, Storage> WritableChannels<'c> for SpecificChannels<Storage, Channels>
where
    Storage: 'c + GetPixel,
    Storage::Pixel: IntoRecursive,
    Channels: 'c + Sync + Clone + IntoRecursive,
    <Channels as IntoRecursive>::Recursive:
        WritableChannelsDescription<<Storage::Pixel as IntoRecursive>::Recursive>,
{
    fn infer_channel_list(&self) -> ChannelList {
        let mut vec = self
            .channels
            .clone()
            .into_recursive()
            .channel_descriptions_list();
        vec.sort_unstable_by_key(|channel: &ChannelDescription| channel.name.clone()); // TODO no clone?

        debug_assert!(
            // check for equal neighbors in sorted vec
            vec.iter()
                .zip(vec.iter().skip(1))
                .all(|(prev, next)| prev.name != next.name),
            "specific channels contain duplicate channel names"
        );

        ChannelList::new(vec)
    }

    fn infer_level_modes(&self) -> (LevelMode, RoundingMode) {
        (LevelMode::Singular, RoundingMode::Down) // TODO
    }

    type Writer = SpecificChannelsWriter<
        'c,
        <<Channels as IntoRecursive>::Recursive as WritableChannelsDescription<
            <Storage::Pixel as IntoRecursive>::Recursive,
        >>::RecursiveWriter,
        Storage,
        Channels,
    >;

    fn create_writer(&'c self, header: &Header) -> Self::Writer {
        SpecificChannelsWriter {
            channels: self,
            recursive_channel_writer: self
                .channels
                .clone()
                .into_recursive()
                .create_recursive_writer(&header.channels),
        }
    }
}

/// A temporary writer for a layer of channels, alpha being optional
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
        let width = block_index.pixel_size.width();
        let x_min = block_index.pixel_position.x() as i32;
        let x_max = x_min + width as i32 - 1;
        let mut current_line_y: Option<usize> = None;
        let mut pixel_line = Vec::with_capacity(width);

        UncompressedBlock::collect_block_data_from_lines(
            &header.channels,
            block_index,
            |line_ref| {
                let line_y = line_ref.location.position.y();
                if current_line_y != Some(line_y) {
                    pixel_line.clear();
                    pixel_line.extend((0..width).map(|x| {
                        self.channels
                            .pixels
                            .get_pixel(Vec2(block_index.pixel_position.x() + x, line_y))
                            .into_recursive()
                    }));
                    current_line_y = Some(line_y);
                }

                self.recursive_channel_writer.write_line(
                    line_ref,
                    x_min,
                    x_max,
                    pixel_line.as_slice(),
                    |px| px,
                );
            },
        )
    }
}

/// A tuple containing either `ChannelsDescription` or `Option<ChannelsDescription>` entries.
/// Use an `Option` if you want to dynamically omit a single channel (probably only for roundtrip tests).
/// The number of entries must match the number of channels.
pub trait WritableChannelsDescription<Pixel>: Sync {
    /// A type that has a recursive entry for each channel in the image,
    /// which must accept the desired pixel type.
    type RecursiveWriter: RecursivePixelWriter<Pixel>;

    /// Create the temporary writer, accepting the sorted list of channels from `channel_descriptions_list`.
    fn create_recursive_writer(&self, channels: &ChannelList) -> Self::RecursiveWriter;

    /// Return all the channels that should actually end up in the image, in any order.
    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]>;
}

impl WritableChannelsDescription<NoneMore> for NoneMore {
    type RecursiveWriter = NoneMore;
    fn create_recursive_writer(&self, _: &ChannelList) -> Self::RecursiveWriter {
        NoneMore
    }
    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]> {
        SmallVec::new()
    }
}

impl<InnerDescriptions, InnerPixel, Sample: IntoNativeSample>
    WritableChannelsDescription<Recursive<InnerPixel, Sample>>
    for Recursive<InnerDescriptions, ChannelDescription>
where
    InnerDescriptions: WritableChannelsDescription<InnerPixel>,
{
    type RecursiveWriter = RecursiveWriter<InnerDescriptions::RecursiveWriter, Sample>;

    fn create_recursive_writer(&self, channels: &ChannelList) -> Self::RecursiveWriter {
        // this linear lookup is required because the order of the channels changed, due to alphabetical sorting
        let (channel_index, target_channel) = channels
            .list
            .iter()
            .enumerate()
            .find(|(_, channel)| channel.name == self.value.name)
            .expect("a channel has not been put into channel list");

        Recursive::new(
            self.inner.create_recursive_writer(channels),
            SampleWriter {
                channel_index,
                target_sample_type: target_channel.sample_type,
                sampling: target_channel.sampling,
                px: PhantomData::default(),
            },
        )
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
where
    InnerDescriptions: WritableChannelsDescription<InnerPixel>,
{
    type RecursiveWriter = OptionalRecursiveWriter<InnerDescriptions::RecursiveWriter, Sample>;

    fn create_recursive_writer(&self, channels: &ChannelList) -> Self::RecursiveWriter {
        // this linear lookup is required because the order of the channels changed, due to alphabetical sorting

        let channel = self.value.as_ref().map(|required_channel| {
            channels
                .list
                .iter()
                .enumerate()
                .find(|(_, channel)| channel == &required_channel)
                .map(|(index, channel)| (index, channel.sample_type, channel.sampling))
                .expect("a channel has not been put into channel list")
        });

        Recursive::new(
            self.inner.create_recursive_writer(channels),
            channel.map(
                |(channel_index, target_sample_type, sampling)| SampleWriter {
                    channel_index,
                    target_sample_type,
                    sampling,
                    px: PhantomData::default(),
                },
            ),
        )
    }

    fn channel_descriptions_list(&self) -> SmallVec<[ChannelDescription; 5]> {
        let mut inner_list = self.inner.channel_descriptions_list();
        if let Some(value) = &self.value {
            inner_list.push(value.clone());
        }
        inner_list
    }
}

/// Write pixels to a slice of bytes. The top level writer contains all the other channels,
/// the most inner channel is `NoneMore`.
pub trait RecursivePixelWriter<Pixel>: Sync {
    /// Write pixels to a slice of bytes. Recursively do this for all channels.
    fn write_line<FullPixel>(
        &self,
        line: LineRefMut<'_>,
        block_x_min: i32,
        block_x_max: i32,
        pixels: &[FullPixel],
        get_pixel: impl Fn(&FullPixel) -> &Pixel,
    );
}

type RecursiveWriter<Inner, Sample> = Recursive<Inner, SampleWriter<Sample>>;
type OptionalRecursiveWriter<Inner, Sample> = Recursive<Inner, Option<SampleWriter<Sample>>>;

/// Write the pixels of a single channel, unconditionally. Generic over the concrete sample type (f16, f32, u32).
#[derive(Debug, Clone)]
pub struct SampleWriter<Sample> {
    target_sample_type: SampleType,
    channel_index: usize,
    sampling: Vec2<usize>,
    px: PhantomData<Sample>,
}

impl<Sample> SampleWriter<Sample>
where
    Sample: IntoNativeSample,
{
    fn write_line<FullPixel>(
        &self,
        line: LineRefMut<'_>,
        block_x_min: i32,
        block_x_max: i32,
        pixels: &[FullPixel],
        get_sample: impl Fn(&FullPixel) -> Sample,
    ) {
        if line.location.sample_count == 0 {
            debug_assert!(
                line.value.is_empty(),
                "line slice should be empty when sample count is zero"
            );
            return;
        }

        debug_assert_eq!(
            line.location.channel, self.channel_index,
            "line dispatched to wrong channel writer"
        );

        let bytes_per_sample = self.target_sample_type.bytes_per_sample();
        debug_assert_eq!(
            line.value.len(),
            line.location.sample_count * bytes_per_sample,
            "line byte count mismatch"
        );

        let y = line.location.position.y() as i32;
        debug_assert_eq!(
            mod_p(y, self.sampling.y()),
            0,
            "line for channel {} at invalid y {}",
            self.channel_index,
            y
        );

        let x_sampling = self.sampling.x();
        let mut byte_writer = Cursor::new(line.value);
        let mut positions = sample_x_positions(
            x_sampling,
            block_x_min,
            block_x_max,
            line.location.sample_count,
        );

        let write_error_msg = "invalid memory buffer length when writing";

        match self.target_sample_type {
            SampleType::F16 => {
                for x in positions {
                    let sample_index = (x - block_x_min)
                        .try_into()
                        .expect("invalid sample position");
                    get_sample(&pixels[sample_index])
                        .to_f16()
                        .write_ne(&mut byte_writer)
                        .expect(write_error_msg);
                }
            }
            SampleType::F32 => {
                for x in positions {
                    let sample_index = (x - block_x_min)
                        .try_into()
                        .expect("invalid sample position");
                    get_sample(&pixels[sample_index])
                        .to_f32()
                        .write_ne(&mut byte_writer)
                        .expect(write_error_msg);
                }
            }
            SampleType::U32 => {
                for x in positions {
                    let sample_index = (x - block_x_min)
                        .try_into()
                        .expect("invalid sample position");
                    get_sample(&pixels[sample_index])
                        .to_u32()
                        .write_ne(&mut byte_writer)
                        .expect(write_error_msg);
                }
            }
        }
    }
}

fn sample_x_positions(
    x_sampling: usize,
    block_x_min: i32,
    block_x_max: i32,
    count: usize,
) -> SamplePositionIter {
    if count == 0 {
        return SamplePositionIter {
            current: 0,
            remaining: 0,
            step: x_sampling as i32,
            max: block_x_max,
        };
    }

    let first = first_sample_coordinate(x_sampling, block_x_min);

    SamplePositionIter {
        current: first,
        remaining: count,
        step: x_sampling as i32,
        max: block_x_max,
    }
}

fn first_sample_coordinate(x_sampling: usize, block_x_min: i32) -> i32 {
    let mut current = div_p(block_x_min, x_sampling) * x_sampling as i32;
    if mod_p(block_x_min, x_sampling) != 0 {
        current += x_sampling as i32;
    }
    current
}

struct SamplePositionIter {
    current: i32,
    remaining: usize,
    step: i32,
    max: i32,
}

impl Iterator for SamplePositionIter {
    type Item = i32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        let value = self.current;
        debug_assert!(
            value <= self.max,
            "sample position {} exceeds block max {}",
            value,
            self.max
        );

        self.remaining -= 1;
        self.current = self.current.saturating_add(self.step);
        Some(value)
    }
}

impl RecursivePixelWriter<NoneMore> for NoneMore {
    fn write_line<FullPixel>(
        &self,
        _: LineRefMut<'_>,
        _: i32,
        _: i32,
        _: &[FullPixel],
        _: impl Fn(&FullPixel) -> &NoneMore,
    ) {
    }
}

impl<Inner, InnerPixel, Sample: IntoNativeSample>
    RecursivePixelWriter<Recursive<InnerPixel, Sample>> for RecursiveWriter<Inner, Sample>
where
    Inner: RecursivePixelWriter<InnerPixel>,
{
    fn write_line<FullPixel>(
        &self,
        line: LineRefMut<'_>,
        block_x_min: i32,
        block_x_max: i32,
        pixels: &[FullPixel],
        get_pixel: impl Fn(&FullPixel) -> &Recursive<InnerPixel, Sample>,
    ) {
        if line.location.channel == self.value.channel_index {
            self.value
                .write_line(line, block_x_min, block_x_max, pixels, |px| {
                    get_pixel(px).value
                });
        } else {
            self.inner
                .write_line(line, block_x_min, block_x_max, pixels, |px| {
                    &get_pixel(px).inner
                });
        }
    }
}

impl<Inner, InnerPixel, Sample> RecursivePixelWriter<Recursive<InnerPixel, Sample>>
    for OptionalRecursiveWriter<Inner, Sample>
where
    Inner: RecursivePixelWriter<InnerPixel>,
    Sample: IntoNativeSample,
{
    fn write_line<FullPixel>(
        &self,
        line: LineRefMut<'_>,
        block_x_min: i32,
        block_x_max: i32,
        pixels: &[FullPixel],
        get_pixel: impl Fn(&FullPixel) -> &Recursive<InnerPixel, Sample>,
    ) {
        if let Some(writer) = &self.value {
            if line.location.channel == writer.channel_index {
                writer.write_line(line, block_x_min, block_x_max, pixels, |px| {
                    get_pixel(px).value
                });
                return;
            }
        }

        self.inner
            .write_line(line, block_x_min, block_x_max, pixels, |px| {
                &get_pixel(px).inner
            });
    }
}

#[cfg(test)]
mod test {
    use crate::image::pixel_vec::PixelVec;
    use crate::image::write::channels::WritableChannels;
    use crate::image::SpecificChannels;
    use crate::meta::attribute::{ChannelDescription, SampleType};
    use crate::prelude::f16;

    #[test]
    fn compiles() {
        let x = 3_f32;
        let y = f16::from_f32(4.0);
        let z = 2_u32;
        let s = 1.3_f32;
        let px = (x, y, z, s);

        assert_is_writable_channels(SpecificChannels::rgba(|_pos| px));

        assert_is_writable_channels(SpecificChannels::rgba(PixelVec::new(
            (3, 2),
            vec![px, px, px, px, px, px],
        )));

        let px = (2333_u32, 4_f32);
        assert_is_writable_channels(
            SpecificChannels::build()
                .with_channel("A")
                .with_channel("C")
                .with_pixels(PixelVec::new((3, 2), vec![px, px, px, px, px, px])),
        );

        let px = (3_f32, f16::ONE, 2333_u32, 4_f32);
        assert_is_writable_channels(SpecificChannels::new(
            (
                ChannelDescription::named("x", SampleType::F32),
                ChannelDescription::named("y", SampleType::F16),
                Some(ChannelDescription::named("z", SampleType::U32)),
                Some(ChannelDescription::named("p", SampleType::F32)),
            ),
            PixelVec::new((3, 2), vec![px, px, px, px, px, px]),
        ));

        fn assert_is_writable_channels<'s>(_channels: impl WritableChannels<'s>) {}
    }
}
