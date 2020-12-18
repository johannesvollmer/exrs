
//! The following three stages are used to read an image.
//! 1. `ReadImage` - The specification. Contains everything the user wants to tell us about loading an image.
//!    The data in this structure will be instantiated and might be borrowed.
//! 2. `ImageReader` - The temporary reader. Based on the specification of the blueprint, a reader is instantiated, once for each layer.
//!    This data structure accumulates the image data from the file.
//!    It also owns temporary data and references the blueprint.
//! 3. `Image` - The clean image. The accumulated data from the Reader
//!    is converted to the clean image structure, losing all temporary data.

pub mod options;
pub mod image;
pub mod layers;
pub mod rgba_channels;
pub mod any_channels;
pub mod levels;
pub mod samples;

pub use rgba_channels::*; // TODO put somwehere else??


use crate::error::{Result};
use crate::image::read::samples::{ReadFlatSamples};
use std::path::Path;
use crate::image::{AnyImage, RgbaLayersImage, RgbaImage, AnyChannels, FlatSamples, Image, Layer, FlatImage};
use crate::image::read::image::ReadLayers;

// TODO explain or use these simple functions somewhere

/// All resolution levels, all channels, all layers.
/// Does not support deep data yet. Uses parallel decompression and relaxed error handling.
/// Inspect the source code of this function if you need customization.
pub fn read_all_data_from_file(path: impl AsRef<Path>) -> Result<AnyImage> {
    read()
        .no_deep_data() // TODO deep data
        .all_resolution_levels()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file(path)
}

// FIXME do not throw error on deep data but just skip it!
/// No deep data, no resolution levels, all channels, all layers.
/// Uses parallel decompression and relaxed error handling.
/// Inspect the source code of this function if you need customization.
pub fn read_all_flat_layers_from_file(path: impl AsRef<Path>) -> Result<FlatImage> {
    read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file(path)
}

/// No deep data, no resolution levels, all channels, first layer.
/// Uses parallel decompression and relaxed error handling.
/// Inspect the source code of this function if you need customization.
pub fn read_first_flat_layer_from_file(path: impl AsRef<Path>) -> Result<Image<Layer<AnyChannels<FlatSamples>>>> {
    read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .first_valid_layer()
        .all_attributes()
        .from_file(path)
}

// FIXME rgba with resolution levels!!! should at least not throw an error
/// No deep data, no resolution levels, rgba channels, all layers.
/// Uses parallel decompression and relaxed error handling.
/// `Create` and `Set` can be closures, see the examples for more information.
/// Inspect the source code of this function if you need customization.
// FIXME Set and Create should not need to be static
pub fn read_all_rgba_layers_from_file<Set:'static, Create:'static>(path: impl AsRef<Path>, create: Create, set_pixel: Set)
    -> Result<RgbaLayersImage<Create::Pixels>>
    where Create: CreateRgbaPixels, Set: SetRgbaPixel<Create::Pixels>
{
    read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(create, set_pixel)
        .all_layers()
        .all_attributes()
        .from_file(path)
}

/// No deep data, no resolution levels, rgba channels, first layer.
/// Uses parallel decompression and relaxed error handling.
/// `Create` and `Set` can be closures, see the examples for more information.
/// Inspect the source code of this function if you need customization.
// FIXME Set and Create should not need to be static
pub fn read_first_rgb_layer_from_file<Set:'static, Create:'static, Pixels:'static>(path: impl AsRef<Path>, create: Create, set_pixel: Set)
    // -> Result<RgbaImage<Create::Pixels>>
    -> Result<RgbaImage<Pixels>>
    where Create: CreateRgbaPixels<Pixels=Pixels>,
          Set: SetRgbaPixel<Pixels>
{
    read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(create, set_pixel)
        .first_valid_layer()
        .all_attributes()
        .from_file(path)
}



#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReadBuilder;

pub fn read() -> ReadBuilder { ReadBuilder }
impl ReadBuilder {
    pub fn no_deep_data(self) -> ReadFlatSamples { ReadFlatSamples }

    // pub fn any_resolution_levels() -> ReadBuilder<> {}

    // TODO
    // e. g. `let sum = reader.any_channels_with(|sample, sum| sum += sample)`
    // e. g. `let floats = reader.any_channels_with(|sample, f32_samples| f32_samples[index] = sample as f32)`
    // pub fn no_deep_data_with <S> (self, storage: S) -> FlatSamplesWith<S> {  }

    // pub fn flat_and_deep_data(self) -> ReadAnySamples { ReadAnySamples }
}
