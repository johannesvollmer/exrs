//! Sorting, merging, and flattening deep images.
//!
//! - [`DeepChannels::sort_samples_by_depth`] sorts the samples within each
//!   pixel from front to back.
//! - [`merge`] combines multiple deep layers into a single deep layer.
//! - [`composite`] combines multiple deep layers into a single flat layer, and
//!   [`flatten`] converts a single deep layer to a flat layer.
//! - [`DeepSample`] and [`composite_samples_front_to_back`] help with
//!   compositing individual pixels.
//!
//! Layers may have different positions and sizes. The result covers the union
//! of all layers. Channels are matched by name. By convention, `Z` contains the
//! depth of each sample, `ZBack` the back depth of volumetric samples, and `A`
//! the alpha of each sample. Color channels are expected to be premultiplied by
//! alpha. Samples without a `Z` channel are treated as having a depth of zero.
//!
//! ```no_run
//! use exr::{image::deep::compositing, prelude::*};
//!
//! let deep: DeepImage = read().deep_data().first_valid_layer().from_file("deep.exr")?;
//!
//! let flat_layer = compositing::flatten(&deep.layer_data)?;
//! let flat_image = Image::new(deep.attributes.clone(), flat_layer);
//! flat_image.write().to_file("flat.exr")?;
//! # Ok::<(), exr::error::Error>(())
//! ```

use std::{cmp::Ordering, convert::TryFrom, ops::Range};

use half::f16;
use smallvec::SmallVec;

use crate::{
    block::samples::Sample,
    error::{Error, Result, UnitResult},
    image::{
        deep::{sample_type, DeepChannels},
        AnyChannel, AnyChannels, FlatSamples, Layer,
    },
    math::Vec2,
    meta::attribute::{DeepImageState, IntegerBounds, SampleType, Text},
};

/// The name of the channel that contains the depth of each sample.
pub const DEPTH: &str = "Z";

/// The name of the channel that contains the back depth of volumetric samples.
pub const BACK_DEPTH: &str = "ZBack";

/// The name of the channel that contains the alpha of each sample.
pub const ALPHA: &str = "A";

impl DeepChannels {
    /// Sort the samples within each pixel by their depth, from front to back.
    /// Samples are sorted by their `Z` value, then by their `ZBack` value.
    /// Samples with equal depths keep their order.
    /// Returns an error if this layer has no `Z` channel.
    pub fn sort_samples_by_depth(&mut self) -> UnitResult {
        let depth = self
            .channel_index_by_name(DEPTH)
            .ok_or_else(|| Error::invalid("sorting deep samples requires a `Z` channel"))?;

        let back_depth = self.channel_index_by_name(BACK_DEPTH);

        let mut order: Vec<usize> = (0..self.total_sample_count()).collect();

        for pixel_index in 0..self.pixel_count() {
            let samples = &mut order[self.sample_range_by_index(pixel_index)];
            if samples.len() > 1 {
                samples.sort_by(|&a, &b| {
                    compare_depths(
                        self.depth_of(a, Some(depth), back_depth),
                        self.depth_of(b, Some(depth), back_depth),
                    )
                });
            }
        }

        for channel in &mut self.list {
            channel.sample_data = reorder(&channel.sample_data, &order);
        }

        Ok(())
    }

    /// The samples of the pixel at the specified position, using the
    /// channels `R`, `G`, `B`, `A`, and `Z`. Missing channels contain zero.
    /// Panics if the position is outside of the resolution.
    #[must_use]
    pub fn rgba_samples_at(&self, position: Vec2<usize>) -> Vec<DeepSample> {
        let value = |name: &str, sample: usize| {
            self.channel_by_name(name)
                .map_or(0.0, |channel| channel.sample_data.value_by_flat_index(sample).to_f32())
        };

        self.sample_range(position)
            .map(|sample| DeepSample {
                depth: value(DEPTH, sample),
                color: [value("R", sample), value("G", sample), value("B", sample)],
                alpha: value(ALPHA, sample),
            })
            .collect()
    }

    /// The exact front and back depth of a sample.
    fn depth_of(
        &self,
        sample: usize,
        depth: Option<usize>,
        back_depth: Option<usize>,
    ) -> (f64, f64) {
        let value = |channel: usize| {
            exact_value(self.list[channel].sample_data.value_by_flat_index(sample))
        };

        let front = depth.map_or(0.0, value);
        (front, back_depth.map_or(front, value))
    }
}

/// Composite one or more deep layers into a single flat layer.
///
/// The samples of all layers are sorted by depth, and then composited front
/// to back using the "over" operation: each sample contributes its values,
/// weighted by the remaining transparency in front of it. Compositing stops
/// when a pixel is fully opaque. For sorted samples, the color and alpha
/// channels match the result of `CompositeDeepScanLine` in the `OpenEXR`
/// library. Unlike `OpenEXR`, the samples are also sorted if there is only
/// one layer, layers without `A` or `Z` channels are allowed, and depth
/// channels are not composited.
///
/// The result covers the union of all layers, and contains all channels of all
/// layers, using the sample type of the first layer containing each channel.
/// The depth channels use `f32` if the depth channels of the layers do not all
/// use the same sample type, and the samples are sorted by their depth in the
/// sample type of the result. Instead of being composited, the depth channels
/// `Z` and `ZBack`, as well as all `u32` channels, contain the value of the
/// nearest sample of each pixel. For layers without a `ZBack` channel, the
/// `ZBack` value of a sample is its `Z` value. Pixels without samples contain
/// zero in all channels. The attributes and encoding of the result are copied
/// from the first layer.
pub fn composite<'l>(
    layers: impl IntoIterator<Item = &'l Layer<DeepChannels>>,
) -> Result<Layer<AnyChannels<FlatSamples>>> {
    let sources = Sources::new(layers)?;
    let output_size = sources.bounds.size;
    let alpha = sources.output_channel_index(ALPHA);

    let mut outputs: SmallVec<[FlatSamples; 8]> = sources
        .output_channels
        .iter()
        .map(|channel| zeroes(channel.sample_type, output_size.area()))
        .collect();

    let is_composited: SmallVec<[bool; 8]> = sources
        .output_channels
        .iter()
        .map(|channel| {
            channel.sample_type != SampleType::U32
                && !channel.name.eq(DEPTH)
                && !channel.name.eq(BACK_DEPTH)
        })
        .collect();

    let mut gathered = Vec::new();
    let mut accumulated = vec![0.0_f32; outputs.len()];

    for (pixel_index, position) in pixel_positions(output_size).enumerate() {
        sources.gather_sorted_samples(position, &mut gathered);

        let nearest = match gathered.first() {
            Some(&nearest) => nearest,
            None => continue, // leave the pixel at zero
        };

        accumulated.fill(0.0);

        for sample in &gathered {
            let opacity = alpha.map_or(0.0, |alpha| accumulated[alpha]);
            if opacity >= 1.0 {
                break;
            }

            let source = &sources.layers[sample.source];
            for (channel, accumulated) in accumulated.iter_mut().enumerate() {
                if is_composited[channel] {
                    if let Some(value) = source.value(channel, sample.index) {
                        *accumulated += (1.0 - opacity) * value.to_f32();
                    }
                }
            }
        }

        for (channel, output) in outputs.iter_mut().enumerate() {
            let value = if is_composited[channel] {
                Sample::F32(accumulated[channel])
            } else {
                sources.value(channel, &nearest).unwrap_or(Sample::F32(0.0))
            };

            set_value(output, pixel_index, value);
        }
    }

    let first = sources.first_layer;
    let list = sources
        .output_channels
        .into_iter()
        .zip(outputs)
        .map(|(channel, sample_data)| AnyChannel {
            name: channel.name,
            sample_data,
            quantize_linearly: channel.quantize_linearly,
            sampling: Vec2(1, 1),
        })
        .collect();

    let mut attributes = first.attributes.clone();
    attributes.layer_position = sources.bounds.position;
    attributes.deep_image_state = None;

    Ok(Layer {
        channel_data: AnyChannels {
            list,
        },
        attributes,
        size: output_size,
        encoding: first.encoding,
    })
}

/// Convert a deep layer into a flat layer, by compositing all samples of each
/// pixel. See [`composite`] for details.
pub fn flatten(layer: &Layer<DeepChannels>) -> Result<Layer<AnyChannels<FlatSamples>>> {
    composite(std::iter::once(layer))
}

/// Merge one or more deep layers into a single deep layer.
///
/// The result covers the union of all layers, and contains all channels of all
/// layers, using the sample type of the first layer containing each channel.
/// The depth channels `Z` and `ZBack` use `f32` if the depth channels of the
/// layers do not all use the same sample type. The samples are sorted by their
/// depth as stored in the result.
/// If a layer does not contain a channel, its samples have the value zero in
/// that channel, except for `ZBack`, which then contains the `Z` value of the
/// sample. The samples of each pixel are sorted by depth, from front to
/// back. Overlapping volumetric samples are not split.
/// The attributes and encoding of the result are copied from the first layer.
pub fn merge<'l>(
    layers: impl IntoIterator<Item = &'l Layer<DeepChannels>>,
) -> Result<Layer<DeepChannels>> {
    let sources = Sources::new(layers)?;
    let output_size = sources.bounds.size;

    let mut sample_offsets = Vec::with_capacity(output_size.area() + 1);
    sample_offsets.push(0);

    let mut total_sample_count = 0_usize;
    for position in pixel_positions(output_size) {
        let global_position = sources.global_position(position);

        total_sample_count += sources
            .layers
            .iter()
            .filter_map(|source| source.sample_range(global_position))
            .map(|range| range.len())
            .sum::<usize>();

        sample_offsets.push(total_sample_count);
    }

    let mut outputs: SmallVec<[FlatSamples; 8]> = sources
        .output_channels
        .iter()
        .map(|channel| zeroes(channel.sample_type, total_sample_count))
        .collect();

    let mut gathered = Vec::new();
    for (pixel_index, position) in pixel_positions(output_size).enumerate() {
        sources.gather_sorted_samples(position, &mut gathered);

        for (sample, output_index) in gathered.iter().zip(sample_offsets[pixel_index]..) {
            for (channel, output) in outputs.iter_mut().enumerate() {
                if let Some(value) = sources.value(channel, sample) {
                    set_value(output, output_index, value);
                }
            }
        }
    }

    let has_depth = sources.output_channel_index(DEPTH).is_some();
    let first = sources.first_layer;

    let list = sources
        .output_channels
        .into_iter()
        .zip(outputs)
        .map(|(channel, sample_data)| AnyChannel {
            name: channel.name,
            sample_data,
            quantize_linearly: channel.quantize_linearly,
            sampling: Vec2(1, 1),
        })
        .collect();

    let mut attributes = first.attributes.clone();
    attributes.layer_position = sources.bounds.position;
    attributes.deep_image_state = if has_depth {
        Some(DeepImageState::Sorted)
    } else {
        None
    };

    Ok(Layer {
        channel_data: DeepChannels::from_sample_offsets(output_size, sample_offsets, list)?,
        attributes,
        size: output_size,
        encoding: first.encoding,
    })
}

/// A single deep sample with a depth, a color, and an alpha value.
/// The color is premultiplied by alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeepSample {
    /// The distance of the sample from the camera.
    pub depth: f32,

    /// The red, green, and blue values, premultiplied by alpha.
    pub color: [f32; 3],

    /// The opacity of this sample.
    pub alpha: f32,
}

impl DeepSample {
    /// Create a sample from a color that is already premultiplied by alpha.
    #[must_use]
    pub fn new_premultiplied(depth: f32, color: [f32; 3], alpha: f32) -> Self {
        Self {
            depth,
            color,
            alpha,
        }
    }

    /// Create a sample from a color that is not yet premultiplied by alpha.
    #[must_use]
    pub fn new_unpremultiplied(depth: f32, color: [f32; 3], alpha: f32) -> Self {
        Self {
            depth,
            color: color.map(|value| value * alpha),
            alpha,
        }
    }
}

/// Sort the samples by depth, from front to back.
/// Samples with equal depths keep their order.
pub fn sort_samples_by_depth(samples: &mut [DeepSample]) {
    samples.sort_by(|a, b| a.depth.total_cmp(&b.depth));
}

/// Composite samples that are sorted from front to back, using the "over"
/// operation. Returns the premultiplied color and the alpha of the result.
/// Stops when the result is fully opaque.
#[must_use]
pub fn composite_samples_front_to_back(samples: &[DeepSample]) -> ([f32; 3], f32) {
    let mut color = [0.0; 3];
    let mut alpha = 0.0;

    for sample in samples {
        if alpha >= 1.0 {
            break;
        }

        let transparency = 1.0 - alpha;
        for (color, sample_color) in color.iter_mut().zip(sample.color) {
            *color += transparency * sample_color;
        }

        alpha += transparency * sample.alpha;
    }

    (color, alpha)
}

/// Composite samples that are sorted from front to back, and return the
/// resulting color as `[r, g, b, a]`, where the color is not premultiplied.
#[must_use]
pub fn flatten_to_rgba(samples: &[DeepSample]) -> [f32; 4] {
    let ([r, g, b], alpha) = composite_samples_front_to_back(samples);

    if alpha > 0.0 {
        [r / alpha, g / alpha, b / alpha, alpha]
    } else {
        [0.0; 4]
    }
}

/// The channel of the result of compositing or merging.
#[derive(Debug, Clone)]
struct OutputChannel {
    name: Text,
    sample_type: SampleType,
    quantize_linearly: bool,
}

/// A deep layer that is composited or merged.
#[derive(Debug)]
struct Source<'l> {
    bounds: IntegerBounds,
    channels: &'l DeepChannels,

    /// For each output channel, the index of the corresponding channel in this
    /// layer.
    channel_indices: SmallVec<[Option<usize>; 8]>,

    depth: Option<usize>,
    back_depth: Option<usize>,
}

/// A sample that contributes to a pixel.
#[derive(Debug, Clone, Copy)]
struct GatheredSample {
    /// The depth and back depth, exactly as stored in the result.
    depth: (f64, f64),
    source: usize,
    index: usize,
}

/// All layers that are composited or merged.
#[derive(Debug)]
struct Sources<'l> {
    layers: Vec<Source<'l>>,
    output_channels: Vec<OutputChannel>,
    bounds: IntegerBounds,
    first_layer: &'l Layer<DeepChannels>,
    depth_type: Option<SampleType>,
    back_depth_type: Option<SampleType>,
}

impl<'l> Sources<'l> {
    fn new(layers: impl IntoIterator<Item = &'l Layer<DeepChannels>>) -> Result<Self> {
        let layers: Vec<&'l Layer<DeepChannels>> = layers.into_iter().collect();
        let first_layer = *layers.first().ok_or_else(|| Error::invalid("no deep layers"))?;

        let mut output_channels: Vec<OutputChannel> = Vec::new();
        for layer in &layers {
            layer.channel_data.validate()?;

            if layer.size != layer.channel_data.resolution() {
                return Err(Error::invalid(
                    "deep layer size does not match the resolution of its channels",
                ));
            }

            for channel in layer.channel_data.channels() {
                if !output_channels.iter().any(|output| output.name == channel.name) {
                    output_channels.push(OutputChannel {
                        name: channel.name.clone(),
                        sample_type: sample_type(&channel.sample_data),
                        quantize_linearly: channel.quantize_linearly,
                    });
                }
            }
        }

        output_channels.sort_by(|a, b| a.name.cmp(&b.name));

        // samples are sorted by their depth as stored in the result,
        // so avoid rounding the depth of some samples but not others
        let is_depth = |name: &Text| name.eq(DEPTH) || name.eq(BACK_DEPTH);
        let depth_sample_types: SmallVec<[SampleType; 8]> = layers
            .iter()
            .flat_map(|layer| layer.channel_data.channels())
            .filter(|channel| is_depth(&channel.name))
            .map(|channel| sample_type(&channel.sample_data))
            .collect();

        if depth_sample_types.windows(2).any(|pair| pair[0] != pair[1]) {
            for output in output_channels.iter_mut().filter(|output| is_depth(&output.name)) {
                output.sample_type = SampleType::F32;
            }
        }

        let sources: Vec<Source<'l>> = layers
            .iter()
            .map(|layer| {
                let channels = &layer.channel_data;

                Source {
                    bounds: layer.absolute_bounds(),
                    channels,
                    depth: channels.channel_index_by_name(DEPTH),
                    back_depth: channels.channel_index_by_name(BACK_DEPTH),
                    channel_indices: output_channels
                        .iter()
                        .map(|output| {
                            channels
                                .channels()
                                .iter()
                                .position(|channel| channel.name == output.name)
                        })
                        .collect(),
                }
            })
            .collect();

        let output_type = |name: &str| {
            output_channels
                .iter()
                .find(|output| output.name.eq(name))
                .map(|output| output.sample_type)
        };

        Ok(Self {
            bounds: union_of_bounds(layers.iter().map(|layer| layer.absolute_bounds()))?,
            depth_type: output_type(DEPTH),
            back_depth_type: output_type(BACK_DEPTH),
            layers: sources,
            output_channels,
            first_layer,
        })
    }

    fn output_channel_index(&self, name: &str) -> Option<usize> {
        self.output_channels.iter().position(|channel| channel.name.eq(name))
    }

    /// The value of a sample in the specified output channel, if its layer
    /// contains that channel. If its layer has no back depth channel,
    /// the back depth of the sample is its front depth.
    fn value(&self, output_channel: usize, sample: &GatheredSample) -> Option<Sample> {
        let source = &self.layers[sample.source];

        source.value(output_channel, sample.index).or_else(|| {
            let is_back_depth = self.output_channels[output_channel].name.eq(BACK_DEPTH);
            if is_back_depth {
                Some(source.depth_samples(sample.index).1)
            } else {
                None
            }
        })
    }

    /// The depth and back depth of a sample, exactly as they will be stored
    /// in the result.
    fn stored_depth_of(&self, source: &Source<'_>, sample: usize) -> (f64, f64) {
        let (front, back) = source.depth_samples(sample);

        let stored = |sample: Sample, sample_type: Option<SampleType>| {
            exact_value(sample_type.map_or(sample, |sample_type| convert(sample, sample_type)))
        };

        (stored(front, self.depth_type), stored(back, self.back_depth_type.or(self.depth_type)))
    }

    /// Convert a position in the result to a position in the global 2D space.
    fn global_position(&self, position: Vec2<usize>) -> Vec2<i64> {
        Vec2(
            i64::from(self.bounds.position.x()) + position.x() as i64,
            i64::from(self.bounds.position.y()) + position.y() as i64,
        )
    }

    /// Collect the samples of all layers at the specified position in the
    /// result, sorted by depth from front to back.
    fn gather_sorted_samples(&self, position: Vec2<usize>, gathered: &mut Vec<GatheredSample>) {
        gathered.clear();
        let global_position = self.global_position(position);

        for (source_index, source) in self.layers.iter().enumerate() {
            if let Some(range) = source.sample_range(global_position) {
                gathered.extend(range.map(|index| GatheredSample {
                    depth: self.stored_depth_of(source, index),
                    source: source_index,
                    index,
                }));
            }
        }

        if gathered.len() > 1 {
            gathered.sort_by(|a, b| compare_depths(a.depth, b.depth));
        }
    }
}

impl Source<'_> {
    /// The depth and back depth of a sample of this layer. The depth is zero
    /// if this layer has no depth channel, and the back depth is the depth if
    /// this layer has no back depth channel.
    fn depth_samples(&self, sample: usize) -> (Sample, Sample) {
        let value = |channel: usize| {
            self.channels.channels()[channel].sample_data.value_by_flat_index(sample)
        };

        let front = self.depth.map_or(Sample::F32(0.0), value);
        (front, self.back_depth.map_or(front, value))
    }

    /// The samples of this layer at the specified position in the global 2D
    /// space, if this layer covers that position.
    fn sample_range(&self, global_position: Vec2<i64>) -> Option<Range<usize>> {
        let x = global_position.x() - i64::from(self.bounds.position.x());
        let y = global_position.y() - i64::from(self.bounds.position.y());

        let x = usize::try_from(x).ok().filter(|&x| x < self.bounds.size.width())?;
        let y = usize::try_from(y).ok().filter(|&y| y < self.bounds.size.height())?;

        Some(self.channels.sample_range(Vec2(x, y)))
    }

    /// The value of a sample of this layer in the specified output channel,
    /// if this layer contains that channel.
    fn value(&self, output_channel: usize, sample: usize) -> Option<Sample> {
        self.channel_indices[output_channel].map(|channel| {
            self.channels.channels()[channel].sample_data.value_by_flat_index(sample)
        })
    }
}

fn compare_depths(a: (f64, f64), b: (f64, f64)) -> Ordering {
    a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1))
}

/// The smallest rectangle containing all the specified rectangles.
fn union_of_bounds(bounds: impl Iterator<Item = IntegerBounds>) -> Result<IntegerBounds> {
    let mut union: Option<(Vec2<i64>, Vec2<i64>)> = None;

    for bounds in bounds {
        let start = Vec2(i64::from(bounds.position.x()), i64::from(bounds.position.y()));
        let end =
            Vec2(start.x() + bounds.size.width() as i64, start.y() + bounds.size.height() as i64);

        union = Some(match union {
            None => (start, end),
            Some((union_start, union_end)) => (
                Vec2(union_start.x().min(start.x()), union_start.y().min(start.y())),
                Vec2(union_end.x().max(end.x()), union_end.y().max(end.y())),
            ),
        });
    }

    let (start, end) = union.ok_or_else(|| Error::invalid("no deep layers"))?;
    let to_i32 = |value: i64| i32::try_from(value).map_err(|_| Error::invalid("layer bounds"));
    let to_usize = |value: i64| usize::try_from(value).map_err(|_| Error::invalid("layer bounds"));

    Ok(IntegerBounds {
        position: Vec2(to_i32(start.x())?, to_i32(start.y())?),
        size: Vec2(to_usize(end.x() - start.x())?, to_usize(end.y() - start.y())?),
    })
}

/// All pixel positions of an image with the specified size, in row-major
/// order.
fn pixel_positions(size: Vec2<usize>) -> impl Iterator<Item = Vec2<usize>> {
    (0..size.height()).flat_map(move |y| (0..size.width()).map(move |x| Vec2(x, y)))
}

fn zeroes(sample_type: SampleType, count: usize) -> FlatSamples {
    match sample_type {
        SampleType::F16 => FlatSamples::F16(vec![f16::ZERO; count]),
        SampleType::F32 => FlatSamples::F32(vec![0.0; count]),
        SampleType::U32 => FlatSamples::U32(vec![0; count]),
    }
}

/// The exact value of a sample. `f64` can represent every value of every
/// sample type.
fn exact_value(sample: Sample) -> f64 {
    match sample {
        Sample::F16(value) => value.to_f64(),
        Sample::F32(value) => f64::from(value),
        Sample::U32(value) => f64::from(value),
    }
}

/// Convert a sample to the specified type, as when storing it in a channel of
/// that type.
fn convert(sample: Sample, sample_type: SampleType) -> Sample {
    match sample_type {
        SampleType::F16 => Sample::F16(sample.to_f16()),
        SampleType::F32 => Sample::F32(sample.to_f32()),
        SampleType::U32 => Sample::U32(sample.to_u32()),
    }
}

fn set_value(samples: &mut FlatSamples, index: usize, value: Sample) {
    match samples {
        FlatSamples::F16(values) => values[index] = value.to_f16(),
        FlatSamples::F32(values) => values[index] = value.to_f32(),
        FlatSamples::U32(values) => values[index] = value.to_u32(),
    }
}

/// Create a new sample storage containing the samples in the specified order.
fn reorder(samples: &FlatSamples, order: &[usize]) -> FlatSamples {
    match samples {
        FlatSamples::F16(values) => {
            FlatSamples::F16(order.iter().map(|&index| values[index]).collect())
        }
        FlatSamples::F32(values) => {
            FlatSamples::F32(order.iter().map(|&index| values[index]).collect())
        }
        FlatSamples::U32(values) => {
            FlatSamples::U32(order.iter().map(|&index| values[index]).collect())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{image::Encoding, meta::header::LayerAttributes};

    fn layer(
        position: Vec2<i32>,
        size: (usize, usize),
        counts: Vec<usize>,
        z: Vec<f32>,
        rgba: [Vec<f32>; 4],
    ) -> Layer<DeepChannels> {
        let [r, g, b, a] = rgba;
        let to_f16 =
            |values: Vec<f32>| FlatSamples::F16(values.into_iter().map(f16::from_f32).collect());

        let channels = DeepChannels::new(
            size,
            counts,
            vec![
                AnyChannel::new("R", to_f16(r)),
                AnyChannel::new("G", to_f16(g)),
                AnyChannel::new("B", to_f16(b)),
                AnyChannel::new("A", to_f16(a)),
                AnyChannel::new("Z", FlatSamples::F32(z)),
            ],
        )
        .unwrap();

        Layer {
            channel_data: channels,
            attributes: LayerAttributes::default().with_position(position),
            size: size.into(),
            encoding: Encoding::UNCOMPRESSED,
        }
    }

    fn values(layer: &Layer<AnyChannels<FlatSamples>>, name: &str) -> Vec<f32> {
        let channel = layer.channel_data.list.iter().find(|channel| channel.name.eq(name)).unwrap();
        channel.sample_data.values_as_f32().collect()
    }

    #[test]
    fn composite_samples() {
        let samples = [
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5),
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
        ];

        let (color, alpha) = composite_samples_front_to_back(&samples);
        assert_eq!(alpha, 0.75);
        assert_eq!(color, [0.5, 0.25, 0.0]);

        let rgba = flatten_to_rgba(&samples[..1]);
        assert_eq!(rgba, [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(flatten_to_rgba(&[]), [0.0; 4]);

        // samples behind an opaque sample are ignored
        let opaque_in_front = [
            DeepSample::new_premultiplied(1.0, [0.2, 0.2, 0.2], 1.0),
            DeepSample::new_premultiplied(2.0, [1.0, 1.0, 1.0], 1.0),
        ];

        assert_eq!(composite_samples_front_to_back(&opaque_in_front), ([0.2, 0.2, 0.2], 1.0));

        let mut unsorted = [samples[1], samples[0]];
        sort_samples_by_depth(&mut unsorted);
        assert_eq!(unsorted, samples);
    }

    #[test]
    fn sort_layer_samples() {
        let mut deep = layer(
            Vec2(0, 0),
            (2, 1),
            vec![3, 1],
            vec![3.0, 1.0, 2.0, 5.0],
            [vec![0.3, 0.1, 0.2, 0.5], vec![0.0; 4], vec![0.0; 4], vec![1.0; 4]],
        );

        deep.channel_data.sort_samples_by_depth().unwrap();

        let z = &deep.channel_data.channel_by_name("Z").unwrap().sample_data;
        assert_eq!(z.values_as_f32().collect::<Vec<_>>(), vec![1.0, 2.0, 3.0, 5.0]);

        let r = &deep.channel_data.channel_by_name("R").unwrap().sample_data;
        assert_eq!(
            r.values_as_f32().collect::<Vec<_>>(),
            vec![0.1, 0.2, 0.3, 0.5]
                .into_iter()
                .map(|v| f16::from_f32(v).to_f32())
                .collect::<Vec<_>>()
        );

        let samples = deep.channel_data.rgba_samples_at(Vec2(0, 0));
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].depth, 1.0);
        assert_eq!(samples[0].alpha, 1.0);
    }

    #[test]
    fn composite_overlapping_layers() {
        // a 2x1 layer at the origin, with a half transparent red sample at
        // depth 2 in each pixel
        let back = layer(
            Vec2(0, 0),
            (2, 1),
            vec![1, 1],
            vec![2.0, 2.0],
            [vec![0.5, 0.5], vec![0.0, 0.0], vec![0.0, 0.0], vec![0.5, 0.5]],
        );

        // a 2x1 layer shifted by one pixel, with a half transparent green
        // sample at depth 1 in the first pixel, and no samples in the
        // second pixel
        let front = layer(
            Vec2(1, 0),
            (2, 1),
            vec![1, 0],
            vec![1.0],
            [vec![0.0], vec![0.5], vec![0.0], vec![0.5]],
        );

        let flat = composite(vec![&back, &front]).unwrap();
        assert_eq!(flat.size, Vec2(3, 1));
        assert_eq!(flat.attributes.layer_position, Vec2(0, 0));

        // the second pixel is covered by both layers, the front layer is
        // composited first
        assert_eq!(values(&flat, "R"), vec![0.5, 0.25, 0.0]);
        assert_eq!(values(&flat, "G"), vec![0.0, 0.5, 0.0]);
        assert_eq!(values(&flat, "A"), vec![0.5, 0.75, 0.0]);
        assert_eq!(values(&flat, "Z"), vec![2.0, 1.0, 0.0]);

        // the order of the layers does not matter
        let flat_reversed = composite(vec![&front, &back]).unwrap();
        assert_eq!(values(&flat_reversed, "R"), values(&flat, "R"));

        let merged = merge(vec![&back, &front]).unwrap();
        assert_eq!(merged.size, Vec2(3, 1));
        assert_eq!(merged.channel_data.sample_counts().collect::<Vec<_>>(), vec![1, 2, 0]);
        assert_eq!(merged.attributes.deep_image_state, Some(DeepImageState::Sorted));

        let z = &merged.channel_data.channel_by_name("Z").unwrap().sample_data;
        assert_eq!(z.values_as_f32().collect::<Vec<_>>(), vec![2.0, 1.0, 2.0]);

        // flattening the merged layer is equivalent to compositing the original
        // layers
        let flat_merged = flatten(&merged).unwrap();
        for name in ["R", "G", "B", "A", "Z"] {
            assert_eq!(values(&flat_merged, name), values(&flat, name), "channel {name}");
        }
    }

    #[test]
    fn merge_layers_with_different_channels() {
        let with_alpha = layer(
            Vec2(0, 0),
            (1, 1),
            vec![1],
            vec![1.0],
            [vec![1.0], vec![1.0], vec![1.0], vec![1.0]],
        );

        let with_id = Layer {
            channel_data: DeepChannels::new(
                (1, 1),
                vec![2],
                vec![
                    AnyChannel::new("Z", FlatSamples::F32(vec![3.0, 0.5])),
                    AnyChannel::new("id", FlatSamples::U32(vec![7, 9])),
                ],
            )
            .unwrap(),
            attributes: LayerAttributes::default(),
            size: Vec2(1, 1),
            encoding: Encoding::UNCOMPRESSED,
        };

        let merged = merge(vec![&with_alpha, &with_id]).unwrap();
        let names: Vec<String> =
            merged.channel_data.channels().iter().map(|channel| channel.name.to_string()).collect();
        assert_eq!(names, vec!["A", "B", "G", "R", "Z", "id"]);

        let id = &merged.channel_data.channel_by_name("id").unwrap().sample_data;
        assert_eq!(id, &FlatSamples::U32(vec![9, 0, 7]));

        let alpha = &merged.channel_data.channel_by_name("A").unwrap().sample_data;
        assert_eq!(alpha.values_as_f32().collect::<Vec<_>>(), vec![0.0, 1.0, 0.0]);

        // the id of the nearest sample is used when flattening
        let flat = flatten(&merged).unwrap();
        assert_eq!(
            flat.channel_data
                .list
                .iter()
                .find(|channel| channel.name.eq("id"))
                .unwrap()
                .sample_data,
            FlatSamples::U32(vec![9])
        );
    }

    #[test]
    fn missing_back_depth_equals_front_depth() {
        // a volumetric sample with a back depth, and a surface sample at the
        // same depth
        let volume = Layer {
            channel_data: DeepChannels::new(
                (1, 1),
                vec![1],
                vec![
                    AnyChannel::new("Z", FlatSamples::F32(vec![5.0])),
                    AnyChannel::new("ZBack", FlatSamples::F32(vec![6.0])),
                    AnyChannel::new("A", FlatSamples::F32(vec![0.5])),
                    AnyChannel::new("R", FlatSamples::F32(vec![0.5])),
                ],
            )
            .unwrap(),
            attributes: LayerAttributes::default(),
            size: Vec2(1, 1),
            encoding: Encoding::UNCOMPRESSED,
        };

        let surface = layer(
            Vec2(0, 0),
            (1, 1),
            vec![1],
            vec![5.0],
            [vec![0.0], vec![1.0], vec![0.0], vec![1.0]],
        );

        let merged = merge(vec![&volume, &surface]).unwrap();
        let depth = &merged.channel_data.channel_by_name("Z").unwrap().sample_data;
        let back_depth = &merged.channel_data.channel_by_name("ZBack").unwrap().sample_data;

        // the surface sample has no back depth, so it uses its front depth,
        // and is sorted in front of the volumetric sample
        assert_eq!(depth, &FlatSamples::F32(vec![5.0, 5.0]));
        assert_eq!(back_depth, &FlatSamples::F32(vec![5.0, 6.0]));

        let composited = composite(vec![&volume, &surface]).unwrap();
        assert_eq!(values(&composited, "ZBack"), vec![5.0]);
        assert_eq!(values(&composited, "G"), vec![1.0]);

        // flattening the merged layer is equivalent to compositing the original
        // layers
        let flattened = flatten(&merged).unwrap();
        assert_eq!(flattened.channel_data, composited.channel_data);
    }

    #[test]
    fn merge_layers_with_different_depth_sample_types() {
        let half_depth = Layer {
            channel_data: DeepChannels::new(
                (1, 1),
                vec![1],
                vec![
                    AnyChannel::new("Z", FlatSamples::F16(vec![f16::from_f32(5.0)])),
                    AnyChannel::new("ZBack", FlatSamples::F32(vec![5.0025])),
                    AnyChannel::new("A", FlatSamples::F32(vec![0.5])),
                    AnyChannel::new("R", FlatSamples::F32(vec![0.5])),
                ],
            )
            .unwrap(),
            attributes: LayerAttributes::default(),
            size: Vec2(1, 1),
            encoding: Encoding::UNCOMPRESSED,
        };

        // in `f16`, this depth would be rounded to the depth of the other
        // sample
        let full_depth = layer(
            Vec2(0, 0),
            (1, 1),
            vec![1],
            vec![5.001],
            [vec![0.0], vec![0.5], vec![0.0], vec![0.5]],
        );

        let merged = merge(vec![&half_depth, &full_depth]).unwrap();
        let depth = &merged.channel_data.channel_by_name("Z").unwrap().sample_data;
        let back_depth = &merged.channel_data.channel_by_name("ZBack").unwrap().sample_data;

        assert_eq!(depth, &FlatSamples::F32(vec![5.0, 5.001]));
        assert_eq!(back_depth, &FlatSamples::F32(vec![5.0025, 5.001]));

        let composited = composite(vec![&half_depth, &full_depth]).unwrap();
        assert_eq!(values(&composited, "R"), vec![0.5]);
        assert_eq!(values(&composited, "G"), vec![0.25]);

        let flattened = flatten(&merged).unwrap();
        assert_eq!(flattened.channel_data, composited.channel_data);
    }

    #[test]
    fn sort_large_integer_depths_exactly() {
        // these depths are equal when converted to `f32`
        let near = 16_777_216_u32;
        let far = near + 1;

        let with_back_depth = Layer {
            channel_data: DeepChannels::new(
                (1, 1),
                vec![1],
                vec![
                    AnyChannel::new("Z", FlatSamples::U32(vec![near])),
                    AnyChannel::new("ZBack", FlatSamples::U32(vec![near])),
                    AnyChannel::new("A", FlatSamples::F32(vec![0.5])),
                ],
            )
            .unwrap(),
            attributes: LayerAttributes::default(),
            size: Vec2(1, 1),
            encoding: Encoding::UNCOMPRESSED,
        };

        let without_back_depth = Layer {
            channel_data: DeepChannels::new(
                (1, 1),
                vec![1],
                vec![
                    AnyChannel::new("Z", FlatSamples::U32(vec![far])),
                    AnyChannel::new("A", FlatSamples::F32(vec![0.25])),
                ],
            )
            .unwrap(),
            ..with_back_depth.clone()
        };

        let merged = merge(vec![&without_back_depth, &with_back_depth]).unwrap();
        let depth = &merged.channel_data.channel_by_name("Z").unwrap().sample_data;
        let back_depth = &merged.channel_data.channel_by_name("ZBack").unwrap().sample_data;
        assert_eq!(depth, &FlatSamples::U32(vec![near, far]));
        assert_eq!(back_depth, &FlatSamples::U32(vec![near, far]));

        let composited = composite(vec![&without_back_depth, &with_back_depth]).unwrap();
        let nearest_depth =
            composited.channel_data.list.iter().find(|channel| channel.name.eq("Z"));
        assert_eq!(nearest_depth.unwrap().sample_data, FlatSamples::U32(vec![near]));
        assert_eq!(flatten(&merged).unwrap().channel_data, composited.channel_data);

        let mut unsorted = DeepChannels::new(
            (1, 1),
            vec![2],
            vec![AnyChannel::new("Z", FlatSamples::U32(vec![far, near]))],
        )
        .unwrap();

        unsorted.sort_samples_by_depth().unwrap();
        let depth = &unsorted.channel_by_name("Z").unwrap().sample_data;
        assert_eq!(depth, &FlatSamples::U32(vec![near, far]));
    }

    #[test]
    fn reject_invalid_layers() {
        assert!(composite(Vec::<&Layer<DeepChannels>>::new()).is_err());

        let mut wrong_size = layer(
            Vec2(0, 0),
            (1, 1),
            vec![1],
            vec![1.0],
            [vec![1.0], vec![1.0], vec![1.0], vec![1.0]],
        );
        wrong_size.size = Vec2(2, 1);
        assert!(flatten(&wrong_size).is_err());
    }
}
