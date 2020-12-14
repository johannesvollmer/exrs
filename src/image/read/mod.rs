
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


use std::io::{Seek, BufReader};
use crate::meta::header::{Header};
use crate::error::{Result, UnitResult};
use crate::block::UncompressedBlock;
use crate::image::read::options::{ReadPedantic, ReadNonParallel};
use crate::image::read::samples::{ReadFlatSamples};
use crate::io::Read;
use std::path::Path;
use crate::block::chunk::TileCoordinates;
use crate::image::{AnyImage, RgbaLayersImage, RgbaImage, AnyChannels, FlatSamples, Image, Layer, FlatImage};

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
        .from_file(path)
}

// FIXME rgba with resolution levels!!! should at least not throw an error
/// No deep data, no resolution levels, rgba channels, all layers.
/// Uses parallel decompression and relaxed error handling.
/// `Create` and `Set` can be closures, see the examples for more information.
/// Inspect the source code of this function if you need customization.
pub fn read_all_rgba_layers_from_file<Set, Create>(path: impl AsRef<Path>, create: Create, set_pixel: Set)
    -> Result<RgbaLayersImage<Create::Pixels>>
    where Create: CreateRgbaPixels, Set: SetRgbaPixel<Create::Pixels>
{
    read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(create, set_pixel)
        .all_layers()
        .from_file(path)
}

/// No deep data, no resolution levels, rgba channels, first layer.
/// Uses parallel decompression and relaxed error handling.
/// `Create` and `Set` can be closures, see the examples for more information.
/// Inspect the source code of this function if you need customization.
pub fn read_first_rgb_layer_from_file<Set, Create>(path: impl AsRef<Path>, create: Create, set_pixel: Set)
    -> Result<RgbaImage<Create::Pixels>>
    where Create: CreateRgbaPixels, Set: SetRgbaPixel<Create::Pixels>
{
    read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(create, set_pixel)
        .first_valid_layer()
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


pub trait ReadImage<'s> {
    type Reader: ImageReader;
    fn create_image_reader(&'s self, headers: &[Header]) -> Result<Self::Reader>;

    // define default settings here, as this is the mandatory base image reader
    fn is_sequential(&self) -> bool { false }
    fn is_pedantic(&self) -> bool { true }

    // fn validate_image(&[Header])


    /// Read the exr image from a file.
    /// Use `read_from_unbuffered` instead, if you do not have a file.
    #[inline]
    #[must_use]
    fn from_file(&'s self, path: impl AsRef<Path>) -> Result<<<Self as ReadImage<'s>>::Reader as ImageReader>::Image> {
        self.from_unbuffered(std::fs::File::open(path)?)
    }

    /// Buffer the reader and then read the exr image from it.
    /// Use `read_from_buffered` instead, if your reader is an in-memory reader.
    /// Use `read_from_file` instead, if you have a file path.
    #[inline]
    #[must_use]
    fn from_unbuffered(&'s self, unbuffered: impl Read + Seek + Send) -> Result<<<Self as ReadImage<'s>>::Reader as ImageReader>::Image> {
        self.from_buffered(BufReader::new(unbuffered))
    }

    /// Read the exr image from a buffered reader.
    /// Use `read_from_file` instead, if you have a file path.
    /// Use `read_from_unbuffered` instead, if this is not an in-memory reader.
    #[must_use]
    fn from_buffered(&'s self, read: impl Read + Seek + Send) -> Result<<<Self as ReadImage<'s>>::Reader as ImageReader>::Image> {
        run_reader_from_buffered_source(self, read)
    }
}

pub trait ImageReader {
    type Image;
    fn filter_block(&self, header: (usize, &Header), tile: (usize, &TileCoordinates)) -> bool;
    fn read_block(&mut self, headers: &[Header], block: UncompressedBlock) -> UnitResult;
    fn into_image(self) -> Self::Image;
}


pub trait ReadImageWithOptions: Sized {
    fn pedantic(self) -> ReadPedantic<Self>;
    fn non_parallel(self) -> ReadNonParallel<Self>;
    // fn on_progress<F>(self, on_progress: F) -> ReadOnProgress<F, Self> where F: FnMut(f64);
}

impl<'s, T> ReadImageWithOptions for T where T: ReadImage<'s> {
    fn pedantic(self) -> ReadPedantic<Self> { ReadPedantic { reader: self } }
    fn non_parallel(self) -> ReadNonParallel<Self> { ReadNonParallel { reader: self } }
    /*fn on_progress<F>(self, on_progress: F) -> ReadOnProgress<F, Self> where F: FnMut(f64) {
        ReadOnProgress { on_progress, read_image: self }
    }*/
}



/*impl<'s, T: 's> ReadImageFromSource<'s> for T where T: ReadImage<'s> {
    type Image = <<T as ReadImage<'s>>::Reader as ImageReader>::Image;

    fn read_from_buffered(&'s self, read: impl Read + Seek + Send) -> Result<Self::Image> {
        run_reader_from_buffered_source(self, read)
    }
}*/

pub fn run_reader_from_buffered_source<'r, R:?Sized>(reader: &'r R, buffered: impl Read + Seek + Send)
    -> Result<<<R as ReadImage<'r>>::Reader as ImageReader>::Image>
    where R: ReadImage<'r>
{
    let pedantic = reader.is_pedantic();
    let parallel = !reader.is_sequential();

    let reader = crate::block::read_filtered_blocks_from_buffered(
        buffered,

        move |headers| reader.create_image_reader(headers),

        |reader, header, (tile_index, tile)| {
            reader.filter_block(header, (tile_index, &tile.location)) // TODO pass TileIndices directly!
        },

        |reader, headers, block| {
            reader.read_block(headers, block)
        },

        pedantic, parallel
    )?;

    Ok(reader.into_image())
}



#[cfg(test)]
mod test {
    // use crate::prelude::*;
    // use crate::image::read::rgba_channels::RgbaChannelsInfo;

    /*#[test]
    fn compiles() {
        struct MyRgbaPixels {
            size: Vec2<usize>,
            data: Vec<RgbaPixel>,
        }

        let ref read_simple = read()
            .no_deep_data()
            .any_channels()
            .first_valid_layer();

        let image: Image<Layer<AnyChannels<FlatSamples>>> = read_simple
            .pedantic()
            .read_from_file("my_file.exr")
            .unwrap();

        let read_rgba = read()
            .no_deep_data()
            .rgba_channels(
                |info: &RgbaChannelsInfo| MyRgbaPixels {
                    size: info.resolution,
                    data: vec![RgbaPixel::rgba(1,1,1,0); info.resolution.area() ],
                },

                |pixels: &mut MyRgbaPixels, position: Vec2<usize>, pixel: RgbaPixel| {
                    pixels.data[position.1 * pixels.size.0 + position.0] = pixel
                }
            );

        let read = read_rgba // (&read_rgba as &impl ReadChannels)
            .first_valid_layer()
            .non_parallel()

            .pedantic()
            // .max_pixel_gigabytes(1/*GB*/) // TODO instead:    .validate(|meta|meta.pixel_bytes < 1GB)
            // .on_progress(|progress| println!("{}", progress)); TODO
            ;

        let image: Image<Layer<RgbaChannels<MyRgbaPixels>>> = read
            .read_from_file("my_file.exr")
            .unwrap();
    }*/
}
