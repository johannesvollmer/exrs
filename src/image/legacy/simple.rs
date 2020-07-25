
//! Read and write all supported aspects of an exr image, excluding deep data and multi-resolution levels.
//! Use `exr::image::full` if you do need deep data or resolution levels.

use crate::prelude::common::*;

use crate::io::*;
use crate::meta::*;
use crate::meta::attribute::*;
use crate::error::{Result, UnitResult, Error};
use crate::math::*;
use std::io::{Seek, BufReader, BufWriter};
use crate::image::{OnWriteProgress, OnReadProgress, WriteOptions, ReadOptions};
use crate::block::lines::{LineRef, LineRefMut};
use crate::meta::header::Header;
use std::convert::TryFrom;

// TODO dry this module with image::full?



/// An exr image.
///
/// Supports all possible exr image features.
/// An exr image may contain multiple layers.
/// All meta data is encoded in this image,
/// including custom attributes.
#[derive(Clone, PartialEq, Debug)]
pub struct Image {

    /// All layers contained in the image file
    pub layers: Layers,

    /// Attributes that apply to the whole image file.
    /// These attributes appear in each layer of the file.
    /// Excludes technical meta data.
    /// Each layer in this image also has its own attributes.
    pub attributes: ImageAttributes,
}

/// List of layers in an image.
pub type Layers = SmallVec<[Layer; 3]>;


/// A single layer of an exr image.
/// Contains meta data and actual pixel information of the channels.
#[derive(Clone, PartialEq, Debug)]
pub struct Layer {

    /// List of channels in this layer.
    /// Contains the actual pixel data of the image.
    pub channels: Channels,

    /// Attributes that apply to this layer. Excludes technical meta data.
    /// May still contain attributes that should be considered global for an image file.
    /// Does not contain data window size, line order, tiling, or compression attributes.
    /// The image also has attributes that do not differ per layer.
    pub attributes: LayerAttributes,

    /// The pixel resolution of this layer.
    /// See `layer.attributes` for more attributes, like for example layer position.
    pub size: Vec2<usize>,

    /// In what order the tiles of this header occur in the file.
    /// Does not change any actual image orientation.
    /// See `layer.attributes` for more attributes.
    pub line_order: LineOrder,

    /// How the pixel data of all channels in this layer is compressed. May be `Compression::Uncompressed`.
    /// See `layer.attributes` for more attributes.
    pub compression: Compression,

    /// If this is some pair of numbers, the image is divided into tiles of that size.
    /// If this is none, the image is divided into scan line blocks, depending on the compression method.
    pub tile_size: Option<Vec2<usize>>,

}


/// List of channels in a Layer
// TODO API use sorted set by name instead??
pub type Channels = SmallVec<[Channel; 5]>;


/// Contains an arbitrary list of pixel data.
/// Each channel can have a different pixel type,
/// either f16, f32, or u32.
#[derive(Clone, Debug, PartialEq)]
pub struct Channel {

    /// One of "R", "G", or "B" most of the time.
    pub name: Text,

    /// The actual pixel data. Contains a flattened vector of samples.
    /// The vector contains each row, one after another.
    /// The number of pixels depends on the resolution of the layer
    /// and the sampling rate of this channel.
    ///
    /// Thus, a specific pixel value can be found at the index
    /// `samples[(y_index / sampling_y) * width + (x_index / sampling_x)]`.
    pub samples: Samples,

    /// This attribute only tells lossy compression methods
    /// whether this value should be quantized exponentially or linearly.
    ///
    /// Should be `false` for red, green, or blue channels.
    /// Should be `true` for hue, chroma, saturation, or alpha channels.
    pub quantize_linearly: bool,

    /// How many of the samples are skipped compared to the other channels in this layer.
    ///
    /// Can be used for chroma subsampling for manual lossy data compression.
    /// Values other than 1 are allowed only in flat, scan-line based images.
    /// If an image is deep or tiled, x and y sampling rates for all of its channels must be 1.
    pub sampling: Vec2<usize>,
}

/// Actual pixel data in a channel. Is either one of f16, f32, or u32.
// TODO not require vec storage but also on-the-fly generation
#[derive(Clone, PartialEq)]
pub enum Samples {

    /// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
    /// but with 5 exponent bits and 10 bits for the fraction.
    ///
    /// Currently this crate is using the `half` crate, which is an implementation of the IEEE 754-2008 standard, meeting that requirement.
    F16(Vec<f16>),

    /// 32-bit float samples.
    F32(Vec<f32>),

    /// 32-bit unsigned int samples.
    /// Used for segmentation of layers.
    U32(Vec<u32>),
}


/*#[derive(Clone, PartialEq)] TODO
pub enum Samples {
    F16(SampleStorage<f16>),
    F32(SampleStorage<f32>),
    U32(SampleStorage<u32>),
}

pub trait SampleStorage<T> {
    fn sample(position: Vec2, resolution: Vec2) -> T,
    fn allocate() ???
}

impl SampleStorage<f16> for Vec<f16> { }
impl SampleStorage<f16> for Fn(Vec2) -> Iterator<Item=f16> { }*/




/*#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ChannelSampler<'t, T: 't> {
    samples: &'t [T],
    subsampled_size: Vec2<usize>,
    subsampling_factor: Vec2<usize>,
}

impl<'t, T> ChannelSampler<'t, T> {
    pub fn sample(&self, pixel: Vec2<usize>) -> &'t T {
        let local_index = pixel / self.subsampling_factor;
        debug_assert!(local_index.0 < self.subsampled_size.0, "invalid x coordinate");
        debug_assert!(local_index.1 < self.subsampled_size.1, "invalid y coordinate");
        &self.samples[local_index.1 * self.subsampled_size.0 + local_index.0]
    }
}*/



impl Image {

    /// Create an image that is to be written to a file.
    ///
    /// Consider using `Image::new_from_layers` for creating an image with multiple layers.
    /// Use the raw `Image { .. }` constructor for even more complex cases.
    pub fn new_from_single_layer(layer: Layer) -> Self {
        Self {
            attributes: ImageAttributes::new(layer.size),
            layers: smallvec![ layer ],
        }
    }

    /// Create an image that is to be written to a file.
    /// Define the `display_window` to describe the area
    /// within the infinite 2D space that should be visible.
    ///
    /// Consider using `Image::new_from_single_layer` for simpler cases.
    /// Use the raw `Image { .. }` constructor for more complex cases.
    pub fn new_from_layers(layers: Layers, display_window: IntegerBounds) -> Self {
        Self { layers, attributes: ImageAttributes::default().with_display_window(display_window) }
    }


    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    /// Returns an empty image in case only deep data exists in the file.
    #[must_use]
    pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions<impl OnReadProgress>) -> Result<Self> {
        Self::read_from_unbuffered(std::fs::File::open(path)?, options)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    ///
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read + Send + Seek, options: ReadOptions<impl OnReadProgress>) -> Result<Self> { // TODO not need be seek nor send
        Self::read_from_buffered(BufReader::new(unbuffered), options)
    }

    /// Read the exr image from a reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    ///
    ///
    /// _Note: If you encounter a reader that is not send or not seek,
    /// open an issue on the github repository._
    #[must_use]
    pub fn read_from_buffered(read: impl Read + Send + Seek, options: ReadOptions<impl OnReadProgress>) -> Result<Self> { // TODO not need be seek nor send
        let mut image: Image = crate::block::lines::read_filtered_lines_from_buffered(
            read,
            Image::allocate,

            |_image, (_, header), (_, tile_index)| {
                !header.deep && tile_index.location.is_largest_resolution_level()
            },

            |image, _meta, line| Image::insert_line(image, line),

            options
        )?;

        {   // remove channels that had no data (deep data is not loaded)
            for layer in &mut image.layers {
                layer.channels.retain(|channel| channel.samples.len() > 0);
            }

            // remove parts that had only deep channels
            image.layers.retain(|layer| layer.channels.len() > 0);
        }

        Ok(image)
    }

    /// Write the exr image to a file.
    /// Use `write_to_unbuffered` instead if you do not have a file.
    /// If an error occurs, attempts to delete the partially written file.
    #[must_use]
    pub fn write_to_file(&self, path: impl AsRef<std::path::Path>, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        crate::io::attempt_delete_file_on_write_error(path.as_ref(), |write|
            self.write_to_unbuffered(write, options)
        )
    }

    /// Buffer the writer and then write the exr image to it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first, using `write_to_buffered`.
    #[must_use]
    pub fn write_to_unbuffered(&self, unbuffered: impl Write + Seek, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        self.write_to_buffered(BufWriter::new(unbuffered), options)
    }

    /// Write the exr image to a writer.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory writer.
    /// If your writer cannot seek, you can write to an in-memory vector of bytes first.
    #[must_use]
    pub fn write_to_buffered(&self, write: impl Write + Seek, options: WriteOptions<impl OnWriteProgress>) -> UnitResult {
        crate::block::lines::write_all_lines_to_buffered(
            write,  self.infer_meta_data(),
            |_meta, line_mut| self.extract_line(line_mut),
            options
        )
    }

    /// This returns whether the image contains at least one sample that has is not a number.
    pub fn contains_nan_pixels(&self) -> bool {
        self.layers.iter().flat_map(|layer: &Layer| &layer.channels)
            .any(|channel: &Channel| channel.samples.contains_nan())
    }

    /// Crops each layer by removing excess pixels with a value of zero.
    /// Layer that have only pixels with a value of zero are removed from the image.
    ///
    /// The layers will visually appear at the same position as before.
    pub fn remove_excess(&mut self) {
        let layers = std::mem::take(&mut self.layers);
        let layers = layers.into_iter().flat_map(|layer| layer.without_excess()).collect();
        self.layers = layers;
    }
}


impl Layer {

    /// Create a new layer with all required fields.
    /// Uses scan line blocks, and no custom attributes.
    /// Use `Layer::with_compression` or `Layer::with_block_format`
    /// to further configure the file.
    ///
    /// Infers the display window from the data size.
    /// Note that for all layers of a file, the display window must be the same.
    ///
    /// Panics if anything is invalid or missing.
    /// Will sort channels to correct order if necessary.
    pub fn new(name: Text, data_size: impl Into<Vec2<usize>>, mut channels: Channels) -> Self {
        let data_size: Vec2<usize> = data_size.into();

        assert!(!channels.is_empty(), "at least one channel is required");

        assert!(
            channels.iter().all(|chan|
                chan.samples.len() / chan.sampling.area() == data_size.area()
            ),
            "channel data size must conform to data window size (scaled by channel sampling)"
        );

        channels.sort_by_key(|chan| chan.name.clone()); // TODO why clone?!

        Layer {
            channels,
            size: data_size,
            compression: Compression::Uncompressed,

            tile_size: None,
            line_order: LineOrder::Unspecified, // non-parallel write will set this to increasing if possible

            attributes: LayerAttributes::new(name),
        }
    }

    /// Specify how the image is split into blocks in the file.
    /// See `Image::tiles` and `Image::line_order` for more information.
    pub fn with_block_format(self, tiles: Option<Vec2<usize>>, line_order: LineOrder) -> Self {
        Self { tile_size: tiles, line_order, .. self }
    }

    /// Set the compression of this layer.
    pub fn with_compression(self, compression: Compression) -> Self {
        Self { compression, .. self }
    }

    /// The rectangle describing the bounding box of this layer
    /// within the infinite global 2D space of the file.
    pub fn data_window(&self) -> IntegerBounds {
        IntegerBounds::new(self.attributes.layer_position, self.size)
    }

    /// Find the smallest possible bounds of this image, keeping only pixels that have at least one non-zero sample.
    /// Moves the data window such that the image appears in the same place as before.
    ///
    /// This does not discard pixels that have an `u32` of zero, as these are commonly used for `id`s.
    /// If this layer has no pixels left after cropping, `None` is returned.
    /// Use `find_content_bounds()` and `crop` directly to customize this behaviour.
    ///
    /// _Note: This method has O(n) complexity, scaling with the number of pixels,
    /// which is a rather brute force approach. It utilizes multithreading but does not use the graphics card.
    /// Consider implementing your own algorithm, if a faster cropping method is required._
    pub fn without_excess(mut self) -> Option<Self> {
        let content = self.find_content_bounds()?;
        self.crop(content);
        Some(self)
    }

    /// Keep only pixels that are inside the specified bounds. Remove all the other pixels.
    /// Moves the data window such that the image appears in the same place as before.
    /// Can be used with the bounds returned from `find_content_bounds()`.
    /// The specified bounds must be in absolute coordinates, which is the infinite 2D space of the whole file.
    pub fn crop(&mut self, absolute_bounds: IntegerBounds) {
        let bounds = absolute_bounds.with_origin(-self.attributes.layer_position);

        assert!(self.data_window().contains(absolute_bounds), "bounds not valid for layer dimensions");
        assert!(bounds.size.area() > 0, "the cropped image would be empty");

        let start_x = usize::try_from(bounds.position.x()).unwrap();
        let start_y = usize::try_from(bounds.position.y()).unwrap();

        if bounds.size != self.size {
            fn crop_samples<T: Copy>(samples: &[T], old_width: usize, new_height: usize, x_range: std::ops::Range<usize>, y_start: usize) -> Vec<T> {
                let kept_old_lines = samples.chunks_exact(old_width).skip(y_start).take(new_height);
                let trimmed_lines = kept_old_lines.map(|line| &line[x_range.clone()]);
                trimmed_lines.flatten().map(|x| *x).collect() // TODO does this use memcpy?
            }

            for channel in &mut self.channels {
                let samples: &mut Samples = &mut channel.samples;
                let x_range = start_x .. start_x + bounds.size.width();

                match samples {
                    Samples::F16(samples) => *samples = crop_samples(samples, self.size.width(), bounds.size.height(), x_range.clone(), start_y),
                    Samples::F32(samples) => *samples = crop_samples(samples, self.size.width(), bounds.size.height(), x_range.clone(), start_y),
                    Samples::U32(samples) => *samples = crop_samples(samples, self.size.width(), bounds.size.height(), x_range.clone(), start_y),
                }
            }

            self.size = bounds.size;
            self.attributes.layer_position = absolute_bounds.position;
        }
    }

    /// Find the smallest possible bounds of this image, keeping only pixels that have at least one non-zero sample.
    /// This does not discard pixels that have an `u32` of zero, as these are commonly used for `id`s.
    /// The specified bounds are in absolute coordinates, which is the infinite 2D space of the whole file.
    ///
    /// If this layer has no pixels left after cropping, `None` is returned.
    ///
    /// _Note: This method has O(n) complexity, scaling with the number of pixels,
    /// which is a rather brute force approach. It utilizes multithreading but does not use the graphics card.
    /// Consider implementing your own algorithm, if a faster cropping method is required._
    pub fn find_content_bounds(&mut self) -> Option<IntegerBounds> {
        type Bounds = (Vec2<usize>, Vec2<usize>); // min + max

        fn extend_bounds(bounds: Option<Bounds>, element: Option<Bounds>) -> Option<Bounds> {
            if let Some((min, max)) = element {
                // this is not the first line with content => append
                if let Some((min0, max0)) = bounds { Some((min0.min(min), max0.max(max))) }

                // this is the first line with content => create
                else { Some((min, max)) }
            }
            // this is an empty line
            else { bounds }
        }

        // shrink a single line. returns none, if all pixels should be discarded
        fn crop_line<T>(samples: &[T]) -> Option<(usize, usize)> where T: PartialEq + Default {
            let discard = |value: &T| *value != T::default();
            let end = samples.iter().rposition(discard)? + 1; // return none if every pixel should be discarded
            let start = samples[..end].iter().position(discard);
            Some((start.unwrap_or(end), end))
        }

        // shrink a whole channel. returns (min, max)
        fn crop_lines<T: Sync + PartialEq + Default>(samples: &[T], resolution: Vec2<usize>) -> Option<Bounds> {
            use rayon::prelude::*;

             samples
                 .par_chunks(resolution.width())
                 .enumerate()
                 .map(|(y, line)|
                     crop_line(line).map(|(start_x, end_x)| (Vec2(start_x, y), Vec2(end_x, y + 1)))
                 )
                 .reduce(|| None, extend_bounds)
        }

        let original_bounds = (Vec2(0,0), self.size);

        let new_layer_bounds = {
            if self.channels.iter().any(|channel| matches!(channel.samples, Samples::U32(_))) {
                Some(original_bounds)
            }
            else {
                self.channels.iter()
                    .map(|channel| {
                        match &channel.samples { // FIXME iterates ALL channels even if first one is 100% opaque
                            Samples::F16(samples) => crop_lines(samples, self.size),
                            Samples::F32(samples) => crop_lines(samples, self.size),
                            Samples::U32(_) => unreachable!("do not crop id pixels"),
                        }
                    })

                    // pick the largest rectangle, as ALL channel values must be zero
                    .fold(None, extend_bounds)
            }
        };

        new_layer_bounds.map(|(min, max)| IntegerBounds::new(
            self.attributes.layer_position + min.to_i32(),
            max - min
        ))
    }
}


impl Channel {

    /// Create a Channel from name and samples.
    /// Use this for red, green, blue, and luminance channels, but use `non_color_data` otherwise.
    pub fn color_data(name: Text, samples: Samples) -> Self {
        Self { name, samples, quantize_linearly: false, sampling: Vec2(1, 1) }
    }

    /// Create a Channel from name and samples.
    /// Use this for alpha, depth, hue, and saturation channels, but use `color_data` otherwise.
    pub fn non_color_data(name: Text, samples: Samples) -> Self {
        Self { name, samples, quantize_linearly: true, sampling: Vec2(1, 1) }
    }
}

impl Samples {

    /// Number of samples in this vector.
    pub fn len(&self) -> usize {
        match self {
            Samples::F16(vec) => vec.len(),
            Samples::F32(vec) => vec.len(),
            Samples::U32(vec) => vec.len(),
        }
    }

    /// Returns whether these samples contain at least one that is not a number.
    pub fn contains_nan(&self) -> bool {
        match self {
            Samples::F16(ref values) => values.iter().any(|sample| sample.is_nan()),
            Samples::F32(ref values) => values.iter().any(|sample| sample.is_nan()),
            Samples::U32(_) => false,
        }
    }
}



impl Image {

    /// Allocate an image ready to be filled with pixel data.
    pub fn allocate(headers: &[Header]) -> Result<Self> {
        let shared_attributes = &headers.iter()
            // pick the header with the most attributes (ignoring optional default attributes)
            // (all headers should have the same shared attributes anyways)
            .max_by_key(|header| header.shared_attributes.other.len())
            .expect("no headers found").shared_attributes;

        let headers : Result<_> = headers.iter()
            .map(Layer::allocate).collect();

        Ok(Image {
            layers: headers?,
            attributes: shared_attributes.clone(),
        })
    }

    /// Insert one line of pixel data into this image.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        debug_assert_ne!(line.location.sample_count, 0, "line width calculation bug");

        let layer = self.layers.get_mut(line.location.layer)
            .ok_or(Error::invalid("chunk part index"))?;

        layer.insert_line(line)
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        debug_assert_ne!(line.location.sample_count, 0, "line width calculation bug");

        let layer = self.layers.get(line.location.layer)
            .expect("invalid part index");

        layer.extract_line(line)
    }

    /// Create the meta data that describes this image.
    pub fn infer_meta_data(&self) -> Headers {
        self.layers.iter()
            .map(|layer| layer.infer_header(&self.attributes))
            .collect()
    }
}


impl Layer {

    /// Allocate an layer ready to be filled with pixel data.
    pub fn allocate(header: &Header) -> Result<Self> {
        Ok(Layer {
            size: header.layer_size,
            attributes: header.own_attributes.clone(),
            channels: header.channels.list.iter().map(|channel| Channel::allocate(header, channel)).collect(),
            compression: header.compression,
            line_order: header.line_order,

            tile_size: match header.blocks {
                Blocks::ScanLines => None,
                Blocks::Tiles(tiles) => Some(tiles.tile_size),
            }
        })
    }


    // TODO no insert or extract, only `get(line_index) -> Line<'_ mut>`?

    /// Insert one line of pixel data into this layer.
    /// Returns an error for invalid index or line contents.
    pub fn insert_line(&mut self, line: LineRef<'_>) -> UnitResult {
        debug_assert!(line.location.position.x() + line.location.sample_count <= self.size.width(), "line index calculation bug");
        debug_assert!(line.location.position.y() < self.size.height(), "line index calculation bug");

        self.channels.get_mut(line.location.channel)
            .expect("invalid channel index")
            .insert_line(line, self.size)
    }

    /// Read one line of pixel data from this layer.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>) {
        debug_assert!(line.location.position.x() + line.location.sample_count <= self.size.width(), "line index calculation bug");
        debug_assert!(line.location.position.y() < self.size.height(), "line index calculation bug");

        self.channels.get(line.location.channel)
            .expect("invalid channel index")
            .extract_line(line, self.size)
    }

    /// Create the meta data that describes this layer.
    pub fn infer_header(&self, shared_attributes: &ImageAttributes) -> Header {
        let blocks = match self.tile_size {
            Some(tiles) => Blocks::Tiles(TileDescription {
                tile_size: tiles,
                level_mode: LevelMode::Singular,
                rounding_mode: RoundingMode::Down
            }),

            None => Blocks::ScanLines,
        };

        let channels = self.channels.iter()
            .map(Channel::infer_channel_attribute).collect();

        let chunk_count = compute_chunk_count(
            self.compression, self.size, blocks
        );

        Header {
            chunk_count,

            layer_size: self.size,
            compression: self.compression,
            channels: ChannelList::new(channels),
            line_order: self.line_order,

            own_attributes: self.attributes.clone(), // TODO no clone?
            shared_attributes: shared_attributes.clone(),

            blocks,

            deep_data_version: None,
            max_samples_per_pixel: None,
            deep: false,
        }
    }
}

impl Channel {

    /// Allocate a channel ready to be filled with pixel data.
    pub fn allocate(header: &Header, channel: &crate::meta::attribute::ChannelInfo) -> Self {
        // do not allocate for deep data
        let size = if header.deep { Vec2(0, 0) } else {
            header.layer_size / channel.sampling
        };

        Channel {
            name: channel.name.clone(), quantize_linearly: channel.quantize_linearly, sampling: channel.sampling,
            samples: Samples::allocate(size, channel.sample_type)
        }
    }

    /// Insert one line of pixel data into this channel.
    pub fn insert_line(&mut self, line: LineRef<'_>, resolution: Vec2<usize>) -> UnitResult {
        assert_eq!(line.location.level, Vec2(0,0), "line index calculation bug");
        self.samples.insert_line(resolution / self.sampling, line)
    }

    /// Read one line of pixel data from this channel.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>, resolution: Vec2<usize>) {
        debug_assert_eq!(line.location.level, Vec2(0,0), "line index calculation bug");
        self.samples.extract_line(line, resolution / self.sampling)
    }

    /// Create the meta data that describes this channel.
    pub fn infer_channel_attribute(&self) -> attribute::ChannelInfo {
        attribute::ChannelInfo {
            sample_type: match self.samples {
                Samples::F16(_) => SampleType::F16,
                Samples::F32(_) => SampleType::F32,
                Samples::U32(_) => SampleType::U32,
            },

            name: self.name.clone(),
            quantize_linearly: self.quantize_linearly,
            sampling: self.sampling,
        }
    }
}


impl Samples {

    /// Allocate a sample block ready to be filled with pixel data.
    pub fn allocate(resolution: Vec2<usize>, sample_type: SampleType) -> Self {
        let count = resolution.area();
        debug_assert!(count < 1920*20 * 1920*20, "suspiciously large image: {} mega pixels", count / 1_000_000);

        match sample_type {
            SampleType::F16 => Samples::F16(vec![f16::ZERO; count ] ),
            SampleType::F32 => Samples::F32(vec![0.0; count ] ),
            SampleType::U32 => Samples::U32(vec![0; count ] ),
        }
    }

    /// Insert one line of pixel data into this sample block.
    pub fn insert_line(&mut self, resolution: Vec2<usize>, line: LineRef<'_>) -> UnitResult {
        if line.location.position.x() + line.location.sample_count > resolution.width() {
            return Err(Error::invalid("data block x coordinate"))
        }

        if line.location.position.y() > resolution.height() {
            return Err(Error::invalid("data block y coordinate"))
        }

        let start_index = line.location.position.y() * resolution.width() + line.location.position.x();
        let end_index = start_index + line.location.sample_count;

        match self {
            Samples::F16(samples) => line.read_samples_into_slice(&mut samples[start_index .. end_index]),
            Samples::F32(samples) => line.read_samples_into_slice(&mut samples[start_index .. end_index]),
            Samples::U32(samples) => line.read_samples_into_slice(&mut samples[start_index .. end_index]),
        }
    }

    /// Read one line of pixel data from this sample block.
    /// Panics for an invalid index or write error.
    pub fn extract_line(&self, line: LineRefMut<'_>, resolution: Vec2<usize>) {
        let index = line.location;

        // the index is generated by ourselves and must always be correct
        debug_assert!(index.position.x() + index.sample_count <= resolution.width(), "line index calculation bug");
        debug_assert!(index.position.y() < resolution.height(), "line index calculation bug");
        debug_assert_ne!(resolution.0, 0, "sample size but");

        let start_index = index.position.y() * resolution.width() + index.position.x();
        let end_index = start_index + index.sample_count;

        match &self {
            Samples::F16(samples) =>
                line.write_samples_from_slice(&samples[start_index .. end_index])
                .expect("writing line bytes failed"),

            Samples::F32(samples) =>
                line.write_samples_from_slice(&samples[start_index .. end_index])
                .expect("writing line bytes failed"),

            Samples::U32(samples) =>
                line.write_samples_from_slice(&samples[start_index .. end_index])
                .expect("writing line bytes failed"),
        }
    }
}

impl std::fmt::Debug for Samples {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.len() < 32 {
            match self {
                Samples::F16(vec) => vec.fmt(formatter),
                Samples::F32(vec) => vec.fmt(formatter),
                Samples::U32(vec) => vec.fmt(formatter),
            }
        }
        else {
            match self {
                Samples::F16(vec) => write!(formatter, "[f16; {}]", vec.len()),
                Samples::F32(vec) => write!(formatter, "[f32; {}]", vec.len()),
                Samples::U32(vec) => write!(formatter, "[u32; {}]", vec.len()),
            }
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn test_crop(){
        let channel = Channel::color_data("".try_into().unwrap(), Samples::F32(vec![
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 3.0, 0.0,
            0.0, 0.4, 0.3, 0.0,
            0.0, 0.0, 0.0, 0.0,
        ]));

        let expected_channel = Channel::color_data("".try_into().unwrap(), Samples::F32(vec![
            0.0, 3.0,
            0.4, 0.3,
        ]));

        let image = Image::new_from_single_layer(Layer::new(
            "".try_into().unwrap(), Vec2(4, 4), smallvec![ channel ]
        ));

        let mut cropped = image.clone();
        cropped.remove_excess();

        assert_ne!(image, cropped);
        assert_eq!(cropped.layers[0].channels[0], expected_channel);
        assert_eq!(cropped.layers[0].attributes.layer_position, Vec2(1,1));
    }

    #[test]
    pub fn test_crop_size(){
        let channel_a = Channel::color_data("".try_into().unwrap(), Samples::F32(vec![
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 3.3, 0.0,
            0.0, 0.0, 0.0, 0.0,
        ]));

        let channel_b = Channel::color_data("".try_into().unwrap(), Samples::F32(vec![
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
            3.3, 0.0, 0.0, 0.0,
        ]));

        let image = Image::new_from_single_layer(Layer::new(
            "".try_into().unwrap(), Vec2(4, 4), smallvec![ channel_a, channel_b ]
        ));

        let mut cropped = image.clone();
        cropped.remove_excess();

        assert_ne!(image, cropped);
        assert_eq!(cropped.layers[0].attributes.layer_position, Vec2(0,2));
        assert_eq!(cropped.layers[0].size, Vec2(3,2));
    }
}
