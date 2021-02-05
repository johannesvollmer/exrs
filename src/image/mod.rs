
//! Data structures that represent a complete exr image.
//! Contains generic structs that must be nested to obtain a complete image type.
//!
//!
//! For example, an rgba image containing multiple layers
//! can be represented using `Image<Layers<RgbaChannels<MyPixelStorage>>>`.
//! An image containing a single layer with arbitrary channels and no deep data
//! can be represented using `Image<Layer<AnyChannels<FlatSamples>>>`.
//!
//!
//! These and other predefined types are included in this module as
//! 1. `RgbaImage`: A single layer, rgb or rgba channels
//! 1. `RgbaLayersImage`: Multiple layers, rgb or rgba channels
//! 1. `FlatImage`: Multiple layers, any channels, no deep data.
//! 1. `AnyImage`: All supported data (multiple layers, arbitrary channels, no deep data yet)
//!
//! You can also use your own types inside an image,
//! for example if you want to use a custom sample storage.
//!

pub mod read;
pub mod write;
pub mod crop;
pub mod pixel_vec;
pub mod recursive;
// pub mod channel_groups;


use crate::meta::header::{ImageAttributes, LayerAttributes};
use crate::meta::attribute::{Text, LineOrder};
use half::f16;
use crate::math::{Vec2, RoundingMode};
use crate::compression::Compression;
use smallvec::{SmallVec, Array};
use crate::error::Error;

/// Don't do anything
pub(crate) fn ignore_progress(_progress: f64){}

/// This image type contains all supported exr features and can represent almost any image.
/// It currently does not support deep data yet.
pub type AnyImage = Image<Layers<AnyChannels<Levels<FlatSamples>>>>;

/// This image type contains the most common exr features and can represent almost any plain image.
/// Does not contain resolution levels. Does not support deep data.
pub type FlatImage = Image<Layers<AnyChannels<FlatSamples>>>;

/// This image type contains multiple layers, with each layer containing a user-defined type of pixels.
pub type PixelLayersImage<Storage, Channels> = Image<Layers<SpecificChannels<Storage, Channels>>>;

/// This image type contains a single layer containing a user-defined type of pixels.
pub type PixelImage<Storage, Channels> = Image<Layer<SpecificChannels<Storage, Channels>>>;

/// This image type contains multiple layers, with each layer containing a user-defined type of rgba pixels.
pub type RgbaLayersImage<Storage> = PixelLayersImage<Storage, RgbaChannels>;

/// This image type contains a single layer containing a user-defined type of rgba pixels.
pub type RgbaImage<Storage> = PixelImage<Storage, RgbaChannels>;

/// Contains information about the channels in an rgba image, in the order `(red, green, blue, alpha)`.
/// The alpha channel is not required. May be `None` if the image did not contain an alpha channel.
pub type RgbaChannels = (ChannelDescription, ChannelDescription, ChannelDescription, Option<ChannelDescription>);

/// The complete exr image.
/// `Layers` can be either a single `Layer` or `Layers`.
#[derive(Debug, Clone, PartialEq)]
pub struct Image<Layers> {

    /// Attributes that apply to the whole image file.
    /// These attributes appear in each layer of the file.
    /// Excludes technical meta data.
    /// Each layer in this image also has its own attributes.
    pub attributes: ImageAttributes,

    /// The layers contained in the image file.
    /// Can be either a single `Layer` or a list of layers.
    pub layer_data: Layers,
}

/// A list of layers. `Channels` can be `RgbaChannels` or `AnyChannels`.
pub type Layers<Channels> = SmallVec<[Layer<Channels>; 2]>;

/// A single Layer, including fancy attributes and compression settings.
/// `Channels` can be either `RgbaChannels` or `AnyChannels`
#[derive(Debug, Clone, PartialEq)]
pub struct Layer<Channels> {

    /// The actual pixel data. Either `RgbaChannels` or `AnyChannels`
    pub channel_data: Channels,

    /// Attributes that apply to this layer.
    /// May still contain attributes that should be considered global for an image file.
    /// Excludes technical meta data: Does not contain data window size, line order, tiling, or compression attributes.
    /// The image also has attributes, which do not differ per layer.
    pub attributes: LayerAttributes,

    /// The pixel resolution of this layer.
    /// See `layer.attributes` for more attributes, like for example layer position.
    pub size: Vec2<usize>,

    /// How the pixels are split up and compressed.
    pub encoding: Encoding
}

/// How the pixels are split up and compressed.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Encoding {

    /// How the pixel data of all channels in this layer is compressed. May be `Compression::Uncompressed`.
    /// See `layer.attributes` for more attributes.
    pub compression: Compression,

    /// Describes how the pixels of this layer are divided into smaller blocks.
    /// Either splits the image into its scan lines or splits the image into tiles of the specified size.
    /// A single block can be loaded without processing all bytes of a file.
    pub blocks: Blocks,

    /// In what order the tiles of this header occur in the file.
    /// Does not change any actual image orientation.
    /// See `layer.attributes` for more attributes.
    pub line_order: LineOrder,
}

/// How the image pixels are split up into separate blocks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Blocks {

    /// The image is divided into scan line blocks.
    /// The number of scan lines in a block depends on the compression method.
    ScanLines,

    /// The image is divided into tile blocks.
    /// Also specifies the size of each tile in the image
    /// and whether this image contains multiple resolution levels.
    ///
    /// The inner `Vec2` describes the size of each tile.
    /// Stays the same number of pixels across all levels.
    Tiles (Vec2<usize>)
}


/// A grid of rgba pixels. The pixels are written to your custom pixel storage.
/// `PixelStorage` can be anything, from a flat `Vec<f16>` to `Vec<Vec<AnySample>>`, as desired.
/// In order to write this image to a file, your `PixelStorage` must implement [`GetRgbaPixel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecificChannels<PixelStorage, ChannelsDescription> {

    /// A description of the channels in the file, as opposed to the channels in memory.
    /// Should always be a tuple containing `ChannelDescription`s, one description for each channel.
    pub channels: ChannelsDescription, // TODO this is awkward. can this be not a type parameter please? maybe vec<option<chan_info>> ??

    /// Your custom rgba pixel storage
    // TODO should also support `Levels<YourStorage>`, where rgba levels are desired!
    pub storage: PixelStorage, // TODO rename to "pixels"?
}


/// A full list of arbitrary channels, not just rgba.
/// `Samples` can currently only be `FlatSamples` or `Levels<FlatSamples>`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnyChannels<Samples> {

    /// This list must be sorted alphabetically, by channel name.
    /// Use `AnyChannels::sorted` for automatic sorting.
    pub list: SmallVec<[AnyChannel<Samples>; 4]>
}

/// A single arbitrary channel.
/// `Samples` can currently only be `FlatSamples` or `Levels<FlatSamples>`
#[derive(Debug, Clone, PartialEq)]
pub struct AnyChannel<Samples> {

    /// One of "R", "G", or "B" most of the time.
    pub name: Text,

    /// The actual pixel data.
    /// Can be `FlatSamples` or `Levels<FlatSamples>`.
    pub sample_data: Samples,

    /// This attribute only tells lossy compression methods
    /// whether this value should be quantized exponentially or linearly.
    ///
    /// Should be `false` for red, green, blue and luma channels, as they are not perceived linearly.
    /// Should be `true` for hue, chroma, saturation, and alpha channels.
    pub quantize_linearly: bool,

    /// How many of the samples are skipped compared to the other channels in this layer.
    ///
    /// Can be used for chroma subsampling for manual lossy data compression.
    /// Values other than 1 are allowed only in flat, scan-line based images.
    /// If an image is deep or tiled, the sampling rates for all of its channels must be 1.
    pub sampling: Vec2<usize>,
}

/// One or multiple resolution levels of the same image.
/// `Samples` can be `FlatSamples`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Levels<Samples> {

    /// A single image without smaller versions of itself.
    /// If you only want to handle exclusively this case, use `Samples` directly, and not `Levels<Samples>`.
    Singular(Samples),

    /// Contains uniformly scaled smaller versions of the original.
    Mip
    {
        /// Whether to round up or down when calculating Mip/Rip levels.
        rounding_mode: RoundingMode,

        /// The smaller versions of the original.
        level_data: LevelMaps<Samples>
    },

    /// Contains any possible combination of smaller versions of the original.
    Rip
    {
        /// Whether to round up or down when calculating Mip/Rip levels.
        rounding_mode: RoundingMode,

        /// The smaller versions of the original.
        level_data: RipMaps<Samples>
    },
}

/// A list of resolution levels. `Samples` can currently only be `FlatSamples`.
// or `DeepAndFlatSamples` (not yet implemented).
pub type LevelMaps<Samples> = Vec<Samples>;

/// In addition to the full resolution image,
/// this layer also contains smaller versions,
/// and each smaller version has further versions with varying aspect ratios.
/// `Samples` can currently only be `FlatSamples`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RipMaps<Samples> {

    /// A flattened list containing the individual levels
    pub map_data: LevelMaps<Samples>,

    /// The number of levels that were generated along the x-axis and y-axis.
    pub level_count: Vec2<usize>,
}


// TODO deep data
/*#[derive(Clone, PartialEq)]
pub enum DeepAndFlatSamples {
    Deep(DeepSamples),
    Flat(FlatSamples)
}*/

/// A vector of non-deep values (one value per pixel per channel).
/// Stores row after row in a single vector.
/// The precision of all values is either `f16`, `f32` or `u32`.
///
/// Since this is close to the pixel layout in the byte file,
/// this will most likely be the fastest storage.
/// Using a different storage, for example `RgbaChannels`,
/// will probably be slower.
#[derive(Clone, PartialEq)] // debug is implemented manually
pub enum FlatSamples {

    /// A vector of non-deep `f16` values.
    F16(Vec<f16>),

    /// A vector of non-deep `f32` values.
    F32(Vec<f32>),

    /// A vector of non-deep `u32` values.
    U32(Vec<u32>),
}


/*#[derive(Clone, PartialEq)]
pub enum DeepSamples {
    F16(Vec<Vec<f16>>),
    F32(Vec<Vec<f32>>),
    U32(Vec<Vec<u32>>),
}*/


use crate::meta::attribute::*;
use crate::error::Result;
use crate::block::samples::Sample;
use crate::image::write::channels::{GetPixel, WritableChannels, IntoNativeSample, WritableChannelsDescription};
use crate::image::write::layers::WritableLayers;
use crate::image::write::samples::{WritableSamples};
use crate::meta::{mip_map_levels, rip_map_levels};
use crate::io::Data;
use crate::image::recursive::{NoneMore, Recursive, IntoRecursive};
use std::marker::PhantomData;
use std::ops::Not;


impl<Channels> Layer<Channels> {
    /// Sometimes called "data window"
    pub fn absolute_bounds(&self) -> IntegerBounds {
        IntegerBounds::new(self.attributes.layer_position, self.size)
    }
}


impl<SampleStorage, Channels> SpecificChannels<SampleStorage, Channels> {
    /// Create some pixels with channel information.
    /// The `Channels` must be a tuple containing either `ChannelDescription` or `Option<ChannelDescription>`.
    /// The length of the tuple dictates the number of channels in the sample storage.
    pub fn new(channels: Channels, source_samples: SampleStorage) -> Self
        where
            SampleStorage: GetPixel,
            SampleStorage::Pixel: IntoRecursive,
            Channels: Sync + Clone + IntoRecursive,
            <Channels as IntoRecursive>::Recursive: WritableChannelsDescription<<SampleStorage::Pixel as IntoRecursive>::Recursive>,
    {
        SpecificChannels { channels, storage: source_samples }
    }
}

/// Convert this type into one of the known sample types.
/// Also specify the preferred native type, which dictates the default sample type in the image.
pub trait IntoSample: IntoNativeSample {

    /// The native sample types that this type should be converted to.
    const PREFERRED_SAMPLE_TYPE: SampleType;
}

impl IntoSample for f16 { const PREFERRED_SAMPLE_TYPE: SampleType = SampleType::F16; }
impl IntoSample for f32 { const PREFERRED_SAMPLE_TYPE: SampleType = SampleType::F32; }
impl IntoSample for u32 { const PREFERRED_SAMPLE_TYPE: SampleType = SampleType::U32; }

/// Used to construct a `SpecificChannels`.
/// Call `with_named_channel` as many times as desired,
/// and then call `with_pixels` to define the colors.
#[derive(Debug)]
pub struct SpecificChannelsBuilder<RecursiveChannels, RecursivePixel> {
    channels: RecursiveChannels,
    px: PhantomData<RecursivePixel>
}

/// This check can be executed at compile time
/// if the channel names are `&'static str` and the compiler is smart enough.
pub trait CheckDuplicates {

    /// Check for duplicate channel names.
    fn already_contains(&self, name: &Text) -> bool;
}

impl CheckDuplicates for NoneMore {
    fn already_contains(&self, _: &Text) -> bool { false }
}

impl<Inner: CheckDuplicates> CheckDuplicates for Recursive<Inner, ChannelDescription> {
    fn already_contains(&self, name: &Text) -> bool {
        &self.value.name == name || self.inner.already_contains(name)
    }
}

impl SpecificChannels<(),()>
{
    /// Start building some specific channels. On the result of this function,
    /// call `with_named_channel` as many times as desired,
    /// and then call `with_pixels` to define the colors.
    pub fn build() -> SpecificChannelsBuilder<NoneMore, NoneMore> {
        SpecificChannelsBuilder { channels: NoneMore, px: Default::default() }
    }
}

impl<RecursiveChannels: CheckDuplicates, RecursivePixel> SpecificChannelsBuilder<RecursiveChannels, RecursivePixel>
{
    /// Add another channel to this image. Does not add the actual pixels,
    /// but instead only declares the presence of the channel.
    /// Panics if the name contains unsupported characters.
    /// Panics if a channel with the same name already exists.
    /// Use `Text::new_or_none()` to manually handle these cases.
    /// Use `with_channel_details` instead if you want to specify more options than just the name of the channel.
    /// The generic parameter can usually be inferred from the closure in `with_pixels`.
    pub fn with_channel<Sample: IntoSample>(self, name: impl Into<Text>)
                                            -> SpecificChannelsBuilder<Recursive<RecursiveChannels, ChannelDescription>, Recursive<RecursivePixel, Sample>>
    {
        self.with_channel_details::<Sample>(ChannelDescription::named(name, Sample::PREFERRED_SAMPLE_TYPE))
    }

    /// Add another channel to this image. Does not add the actual pixels,
    /// but instead only declares the presence of the channel.
    /// Use `with_channel` instead if you only want to specify the name of the channel.
    /// Panics if a channel with the same name already exists.
    /// The generic parameter can usually be inferred from the closure in `with_pixels`.
    pub fn with_channel_details<Sample: Into<Sample>>(self, channel: ChannelDescription)
        -> SpecificChannelsBuilder<Recursive<RecursiveChannels, ChannelDescription>, Recursive<RecursivePixel, Sample>>
    {
        // duplicate channel names are checked later, but also check now to make sure there are no problems with the `SpecificChannelsWriter`
        assert!(self.channels.already_contains(&channel.name).not(), "channel name `{}` is duplicate", channel.name);

        SpecificChannelsBuilder {
            channels: Recursive::new(self.channels, channel),
            px: Default::default()
        }
    }

    /// Specify the actual pixel contents of the image.
    /// You can pass a closure that returns a color for each pixel (`Fn(Vec2<usize>) -> Pixel`),
    /// or you can pass your own image if it implements `GetPixel`.
    /// The pixel type must be a tuple with the correct number of entries, depending on the number of channels.
    /// The tuple entries can be either `f16`, `f32`, `u32` or `Sample`.
    /// Use `with_pixel_fn` instead of this function, to get extra type safety for your pixel closure.
    pub fn with_pixels<Pixels>(self, get_pixel: Pixels) -> SpecificChannels<Pixels, RecursiveChannels>
        where Pixels: GetPixel, <Pixels as GetPixel>::Pixel: IntoRecursive<Recursive=RecursivePixel>,
    {
        SpecificChannels {
            channels: self.channels,
            storage: get_pixel
        }
    }

    /// Specify the contents of the image.
    /// The pixel type must be a tuple with the correct number of entries, depending on the number of channels.
    /// The tuple entries can be either `f16`, `f32`, `u32` or `Sample`.
    /// Use `with_pixels` instead of this function, if you want to pass an object that is not a closure.
    ///
    /// Usually, the compiler can infer the type of the pixel (for example, `f16,f32,f32`) from the closure.
    /// If that's not possible, you can specify the type of the channels
    /// when declaring the channel (for example, `with_named_channel::<f32>("R")`).
    pub fn with_pixel_fn<Pixel, Pixels>(self, get_pixel: Pixels) -> SpecificChannels<Pixels, RecursiveChannels>
        where Pixels: Sync + Fn(Vec2<usize>) -> Pixel, Pixel: IntoRecursive<Recursive=RecursivePixel>,
    {
        SpecificChannels {
            channels: self.channels,
            storage: get_pixel
        }
    }
}

impl<SampleStorage> SpecificChannels<
    SampleStorage,
    Recursive<Recursive<Recursive<Recursive<NoneMore, ChannelDescription>, ChannelDescription>, ChannelDescription>, ChannelDescription>
>
{

    /// Create an image with red, green, blue, and alpha channels.
    /// You can pass a closure that returns a color for each pixel (`Fn(Vec2<usize>) -> (R,G,B,A)`),
    /// or you can pass your own image if it implements `GetPixel<Pixel=(R,G,B,A)>`.
    /// Each of `R`, `G`, `B` and `A` can be either `f16`, `f32`, `u32`, or `Sample`.
    pub fn rgba<R, G, B, A>(source_samples: SampleStorage) -> Self
        where R: IntoSample, G: IntoSample,
              B: IntoSample, A: IntoSample,
              SampleStorage: GetPixel<Pixel=(R, G, B, A)>
    {
        SpecificChannels::build()
            .with_channel("R")
            .with_channel("G")
            .with_channel("B")
            .with_channel("A")
            .with_pixels(source_samples)
    }
}

impl<SampleStorage> SpecificChannels<
    SampleStorage,
    Recursive<Recursive<Recursive<NoneMore, ChannelDescription>, ChannelDescription>, ChannelDescription>
>
{

    /// Create an image with red, green, and blue channels.
    /// You can pass a closure that returns a color for each pixel (`Fn(Vec2<usize>) -> (R,G,B)`),
    /// or you can pass your own image if it implements `GetPixel<Pixel=(R,G,B)>`.
    /// Each of `R`, `G` and `B` can be either `f16`, `f32`, `u32`, or `Sample`.
    pub fn rgb<R, G, B>(source_samples: SampleStorage) -> Self
        where R: IntoSample, G: IntoSample, B: IntoSample,
              SampleStorage: GetPixel<Pixel=(R, G, B)>
    {
        SpecificChannels::build()
            .with_channel("R")
            .with_channel("G")
            .with_channel("B")
            .with_pixels(source_samples)
    }
}


/// A list of samples representing a single pixel.
/// Does not heap allocate for images with 8 or fewer channels.
pub type FlatSamplesPixel = SmallVec<[Sample; 8]>;

// TODO also deep samples?
impl Layer<AnyChannels<FlatSamples>> {

    /// Use `samples_at` if you can borrow from this layer
    pub fn sample_vec_at(&self, position: Vec2<usize>) -> FlatSamplesPixel {
        self.samples_at(position).collect()
    }

    /// Lookup all channels of a single pixel in the image
    pub fn samples_at(&self, position: Vec2<usize>) -> FlatSampleIterator<'_> {
        FlatSampleIterator {
            layer: self,
            channel_index: 0,
            position
        }
    }
}

/// Iterate over all channels of a single pixel in the image
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FlatSampleIterator<'s> {
    layer: &'s Layer<AnyChannels<FlatSamples>>,
    channel_index: usize,
    position: Vec2<usize>,
}

impl Iterator for FlatSampleIterator<'_> {
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.channel_index < self.layer.channel_data.list.len() {
            let channel = &self.layer.channel_data.list[self.channel_index];
            let sample = channel.sample_data.value_by_flat_index(self.position.flat_index_for_size(self.layer.size));
            self.channel_index += 1;
            Some(sample)
        }
        else { None }
    }
}


impl<SampleData> AnyChannels<SampleData>{

    /// A new list of arbitrary channels. Sorts the list to make it alphabetically stable.
    pub fn sort(mut list: SmallVec<[AnyChannel<SampleData>; 4]>) -> Self {
        list.sort_unstable_by_key(|channel| channel.name.clone()); // TODO no clone?
        Self { list }
    }
}

// FIXME check content size of layer somewhere??? before writing?
impl<LevelSamples> Levels<LevelSamples> {

    /// Get a resolution level by index, sorted by size, decreasing.
    pub fn get_level(&self, level: Vec2<usize>) -> Result<&LevelSamples> {
        match self {
            Levels::Singular(block) => {
                debug_assert_eq!(level, Vec2(0,0), "singular image cannot write leveled blocks bug");
                Ok(block)
            },

            Levels::Mip { level_data, .. } => {
                debug_assert_eq!(level.x(), level.y(), "mip map levels must be equal on x and y bug");
                level_data.get(level.x()).ok_or(Error::invalid("block mip level index"))
            },

            Levels::Rip { level_data, .. } => {
                level_data.get_by_level(level).ok_or(Error::invalid("block rip level index"))
            }
        }
    }

    /// Get a resolution level by index, sorted by size, decreasing.
    // TODO storage order for RIP maps?
    pub fn get_level_mut(&mut self, level: Vec2<usize>) -> Result<&mut LevelSamples> {
        match self {
            Levels::Singular(ref mut block) => {
                debug_assert_eq!(level, Vec2(0,0), "singular image cannot write leveled blocks bug");
                Ok(block)
            },

            Levels::Mip { level_data, .. } => {
                debug_assert_eq!(level.x(), level.y(), "mip map levels must be equal on x and y bug");
                level_data.get_mut(level.x()).ok_or(Error::invalid("block mip level index"))
            },

            Levels::Rip { level_data, .. } => {
                level_data.get_by_level_mut(level).ok_or(Error::invalid("block rip level index"))
            }
        }
    }

    /// Get a slice of all resolution levels, sorted by size, decreasing.
    pub fn levels_as_slice(&self) -> &[LevelSamples] {
        match self {
            Levels::Singular(data) => std::slice::from_ref(data),
            Levels::Mip { level_data, .. } => level_data,
            Levels::Rip { level_data, .. } => &level_data.map_data,
        }
    }

    // TODO simplify working with levels in general! like level_size_by_index and such

    /*pub fn levels_with_size(&self, rounding: RoundingMode, max_resolution: Vec2<usize>) -> Vec<(Vec2<usize>, &S)> {
        match self {
            Levels::Singular(ref data) => vec![ (max_resolution, data) ],
            Levels::Mip(ref maps) => mip_map_levels(rounding, max_resolution).map(|(_index, size)| size).zip(maps).collect(),
            Levels::Rip(ref rip_maps) => rip_map_levels(rounding, max_resolution).map(|(_index, size)| size).zip(&rip_maps.map_data).collect(),
        }
    }*/

    /// Whether this stores multiple resolution levels.
    pub fn level_mode(&self) -> LevelMode {
        match self {
            Levels::Singular(_) => LevelMode::Singular,
            Levels::Mip { .. } => LevelMode::MipMap,
            Levels::Rip { .. } => LevelMode::RipMap,
        }
    }
}

impl<Samples> RipMaps<Samples> {

    /// Flatten the 2D level index to a one dimensional index.
    pub fn get_level_index(&self, level: Vec2<usize>) -> usize {
        level.flat_index_for_size(self.level_count)
    }

    /// Return a level by level index. Level `0` has the largest resolution.
    pub fn get_by_level(&self, level: Vec2<usize>) -> Option<&Samples> {
        self.map_data.get(self.get_level_index(level))
    }

    /// Return a mutable level reference by level index. Level `0` has the largest resolution.
    pub fn get_by_level_mut(&mut self, level: Vec2<usize>) -> Option<&mut Samples> {
        let index = self.get_level_index(level);
        self.map_data.get_mut(index)
    }
}

impl FlatSamples {

    /// The number of samples in the image. Should be the width times the height.
    /// Might vary when subsampling is used.
    pub fn len(&self) -> usize {
        match self {
            FlatSamples::F16(vec) => vec.len(),
            FlatSamples::F32(vec) => vec.len(),
            FlatSamples::U32(vec) => vec.len(),
        }
    }

    /// Views all samples in this storage as f32.
    /// Matches the underlying sample type again for every sample,
    /// match yourself if performance is critical! Does not allocate.
    pub fn values_as_f32<'s>(&'s self) -> impl 's + Iterator<Item = f32> {
        self.values().map(|sample| sample.to_f32())
    }

    /// All samples in this storage as iterator.
    /// Matches the underlying sample type again for every sample,
    /// match yourself if performance is critical! Does not allocate.
    pub fn values<'s>(&'s self) -> impl 's + Iterator<Item = Sample> {
        (0..self.len()).map(move |index| self.value_by_flat_index(index))
    }

    /// Lookup a single value, by flat index.
    /// The flat index can be obtained using `Vec2::flatten_for_width`
    /// which computes the index in a flattened array of pixel rows.
    pub fn value_by_flat_index(&self, index: usize) -> Sample {
        match self {
            FlatSamples::F16(vec) => Sample::F16(vec[index]),
            FlatSamples::F32(vec) => Sample::F32(vec[index]),
            FlatSamples::U32(vec) => Sample::U32(vec[index]),
        }
    }
}


impl<'s, ChannelData:'s> Layer<ChannelData> {

    /// Create a layer with the specified size, attributes, encoding and channels.
    /// The channels can be either `RgbaChannels` or `AnyChannels`.
    pub fn new(
        dimensions: impl Into<Vec2<usize>>,
        attributes: LayerAttributes,
        encoding: Encoding,
        channels: ChannelData
    ) -> Self
        where ChannelData: WritableChannels<'s>
    {
        Layer { channel_data: channels, attributes, size: dimensions.into(), encoding }
    }

    // TODO test pls wtf
    /// Panics for images with Scanline encoding.
    pub fn levels_with_resolution<'l, L>(&self, levels: &'l Levels<L>) -> Box<dyn 'l + Iterator<Item=(&'l L, Vec2<usize>)>> {
        match levels {
            Levels::Singular(level) => Box::new(std::iter::once((level, self.size))),

            Levels::Mip { rounding_mode, level_data } => Box::new(level_data.iter().zip(
                mip_map_levels(*rounding_mode, self.size)
                    .map(|(_index, size)| size)
            )),

            Levels::Rip { rounding_mode, level_data } => Box::new(level_data.map_data.iter().zip(
                rip_map_levels(*rounding_mode, self.size)
                    .map(|(_index, size)| size)
            )),
        }
    }
}

impl Encoding {

    /// No compression. Massive space requirements.
    /// Fast, because it minimizes data shuffling and reallocation.
    pub const UNCOMPRESSED: Encoding = Encoding {
        compression: Compression::Uncompressed,
        blocks: Blocks::ScanLines, // longest lines, faster memcpy
        line_order: LineOrder::Increasing // presumably fastest?
    };

    /// Run-length encoding with tiles of 64x64 pixels. This is the recommended default encoding.
    /// Almost as fast as uncompressed data, but optimizes single-colored areas such as mattes and masks.
    pub const FAST_LOSSLESS: Encoding = Encoding {
        compression: Compression::RLE,
        blocks: Blocks::Tiles(Vec2(64, 64)), // optimize for RLE compression
        line_order: LineOrder::Unspecified
    };

    /// ZIP compression with blocks of 16 lines. Slow, but produces small files without visible artefacts.
    pub const SMALL_LOSSLESS: Encoding = Encoding {
        compression: Compression::ZIP16,
        blocks: Blocks::ScanLines, // largest possible, but also with high probability of parallel workers
        line_order: LineOrder::Increasing
    };

    /// PIZ compression with tiles of 256x256 pixels. Small images, not too slow. Might produce visible artefacts in the image.
    pub const SMALL_FAST_LOSSY: Encoding = Encoding {
        compression: Compression::PIZ,
        blocks: Blocks::Tiles(Vec2(256, 256)),
        line_order: LineOrder::Unspecified
    };
}

impl Default for Encoding {
    fn default() -> Self { Encoding::FAST_LOSSLESS }
}

impl<'s, LayerData: 's> Image<LayerData> where LayerData: WritableLayers<'s> {
    /// Create an image with one or multiple layers. The layer can be a `Layer`, or `Layers` small vector.
    pub fn new(image_attributes: ImageAttributes, layer_data: LayerData) -> Self {
        Image { attributes: image_attributes, layer_data }
    }
}

impl<'s, ChannelData:'s> Image<Layer<ChannelData>> where ChannelData: WritableChannels<'s> {

    /// Uses the display position and size to the channel position and size of the layer.
    pub fn with_layer(layer: Layer<ChannelData>) -> Self {
        let bounds = IntegerBounds::new(layer.attributes.layer_position, layer.size);
        Self::new(ImageAttributes::new(bounds), layer)
    }

    /// Uses empty attributes.
    pub fn with_encoded_layer(size: impl Into<Vec2<usize>>, encoding: Encoding, channels: ChannelData) -> Self {
        // layer name is not required for single-layer images
        Self::with_layer(Layer::new(size, LayerAttributes::default(), encoding, channels))
    }

    /// Uses empty attributes and fast compression.
    pub fn with_channels(size: impl Into<Vec2<usize>>, channels: ChannelData) -> Self {
        Self::with_encoded_layer(size, Encoding::default(), channels)
    }
}



impl<'s, SampleData: 's> AnyChannel<SampleData> {

    /// Create a new channel without subsampling.
    ///
    /// Automatically flags this channel for specialized compression
    /// if the name is "R", "G", "B", "Y", or "L",
    /// as they typically encode values that are perceived non-linearly.
    /// Construct the value yourself using `AnyChannel { .. }`, if you want to control this flag.
    pub fn new(name: impl Into<Text>, sample_data: SampleData) -> Self where SampleData: WritableSamples<'s> {
        let name: Text = name.into();

        AnyChannel {
            quantize_linearly: ChannelDescription::guess_quantization_linearity(&name),
            name, sample_data,
            sampling: Vec2(1, 1),
        }
    }

    /*/// This is the same as `AnyChannel::new()`, but additionally ensures that the closure type is correct.
    pub fn from_closure<V>(name: Text, sample_data: S) -> Self
        where S: Sync + Fn(Vec2<usize>) -> V, V: InferSampleType + Data
    {
        Self::new(name, sample_data)
    }*/
}




/// Check whether this contains any `NaN` value.
/// This is required for comparing the equality of two images, as `NaN` never equals itself (nice!).
pub trait ContainsNaN {
    /// Returns true if this contains any `NaN` value.
    fn contains_nan_pixels(&self) -> bool;
}

impl<L> ContainsNaN for Image<L> where L: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool { self.layer_data.contains_nan_pixels() }
}

impl<C> ContainsNaN for Layer<C> where C: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.channel_data.contains_nan_pixels()
    }
}

impl<C> ContainsNaN for AnyChannels<C> where C: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.list.contains_nan_pixels()
    }
}

impl<C> ContainsNaN for AnyChannel<C> where C: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.sample_data.contains_nan_pixels()
    }
}

impl<S, T> ContainsNaN for SpecificChannels<S, T> where S: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.storage.contains_nan_pixels()
    }
}

impl<C> ContainsNaN for Levels<C> where C: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.levels_as_slice().contains_nan_pixels()
    }
}

impl ContainsNaN for FlatSamples {
    fn contains_nan_pixels(&self) -> bool {
        match self {
            FlatSamples::F16(ref values) => values.as_slice().contains_nan_pixels(),
            FlatSamples::F32(ref values) => values.as_slice().contains_nan_pixels(),
            FlatSamples::U32(ref _values) => false,
        }
    }
}

impl<T> ContainsNaN for &[T] where T: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.iter().any(|value| value.contains_nan_pixels())
    }
}

impl<A: Array> ContainsNaN for SmallVec<A> where A::Item: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        self.as_ref().contains_nan_pixels()
    }
}

// TODO implement contains nan for all pixel tuples
// (low priority because it is only used in the tests)
impl<A,B,C,D> ContainsNaN for (A,B,C,D) where A: Clone+ContainsNaN, B: Clone+ContainsNaN, C: Clone+ContainsNaN, D: Clone+ContainsNaN {
    fn contains_nan_pixels(&self) -> bool { self.clone().into_recursive().contains_nan_pixels() } // TODO no clone?
}

// implement for recursive types
impl ContainsNaN for NoneMore { fn contains_nan_pixels(&self) -> bool { false } }
impl<Inner, T> ContainsNaN for Recursive<Inner, T> where Inner: ContainsNaN, T: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool { self.inner.contains_nan_pixels() || self.value.contains_nan_pixels() }
}

impl<S> ContainsNaN for Option<S> where S: ContainsNaN {
    fn contains_nan_pixels(&self) -> bool {
        match self {
            None => false,
            Some(value) => value.contains_nan_pixels(),
        }
    }
}

impl ContainsNaN for f32 {
    fn contains_nan_pixels(&self) -> bool { self.is_nan() }
}

impl ContainsNaN for f16 {
    fn contains_nan_pixels(&self) -> bool { self.is_nan() }
}

impl ContainsNaN for u32 {
    fn contains_nan_pixels(&self) -> bool { false }
}

impl ContainsNaN for Sample {
    fn contains_nan_pixels(&self) -> bool {
        match self {
            Sample::F16(n) => n.contains_nan_pixels(),
            Sample::F32(n) => n.contains_nan_pixels(),
            Sample::U32(n) => n.contains_nan_pixels(),
        }
    }
}



impl std::fmt::Debug for FlatSamples {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.len() <= 6 {
            match self {
                FlatSamples::F16(vec) => vec.fmt(formatter),
                FlatSamples::F32(vec) => vec.fmt(formatter),
                FlatSamples::U32(vec) => vec.fmt(formatter),
            }
        }
        else {
            match self {
                FlatSamples::F16(vec) => write!(formatter, "[f16; {}]", vec.len()),
                FlatSamples::F32(vec) => write!(formatter, "[f32; {}]", vec.len()),
                FlatSamples::U32(vec) => write!(formatter, "[u32; {}]", vec.len()),
            }
        }
    }
}