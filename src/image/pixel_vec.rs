
//! Provides a predefined pixel storage.
//! Currently only contains a simple flattened vector storage.
//! Use the functions `create_pixel_vec::<YourPixelTuple>` and
//! `set_pixel_in_vec::<YourPixelTuple>` for reading a predefined pixel vector.
//! Use the function `PixelVec::new` to create a pixel vector which can be written to a file.

use super::*;

/// Store all samples in a single array.
/// All samples will be converted to the type `T`.
/// This supports all the sample types, `f16`, `f32`, and `u32`.
///
/// The flattened vector contains all rows one after another.
/// In each row, for each pixel, its red, green, blue, and then alpha
/// samples are stored one after another.
///
/// Use `PixelVec.compute_pixel_index(position)`
/// to compute the flat index of a specific pixel.
#[derive(Eq, PartialEq, Clone)]
pub struct PixelVec<T> {

    /// The resolution of this layer.
    pub resolution: Vec2<usize>,

    /// The flattened vector contains all rows one after another.
    /// In each row, for each pixel, its red, green, blue, and then alpha
    /// samples are stored one after another.
    ///
    /// Use `Flattened::compute_pixel_index(image, position)`
    /// to compute the flat index of a specific pixel.
    pub pixels: Vec<T>,
}

impl<T> PixelVec<T> {

    /* TODO make external functions pixelvec static methods

    /// Create a new flattened pixel storage, checking the length of the provided pixels vector.
    pub fn create_empty<Channels>(resolution: impl Into<Vec2<usize>>, _: Channels) -> Self where T: Default + Clone {
        PixelVec { resolution, pixels: vec![Pixel::default(); resolution.area()] }
    }

    /// Examine a pixel of a `PixelVec<T>` image.
    /// Can usually be used as a function reference instead of calling it directly.
    #[inline]
    pub fn get_pixel(image: &PixelVec<Pixel>, position: Vec2<usize>) -> &Pixel where Pixel: Sync {
        &image.pixels[image.compute_pixel_index(position)]
    }

    /// Update a pixel of a `PixelVec<T>` image.
    /// Can usually be used as a function reference instead of calling it directly.
    #[inline]
    pub fn set_pixel_in_vec<Pixel>(image: &mut PixelVec<Pixel>, position: Vec2<usize>, pixel: Pixel) {
        let index = image.compute_pixel_index(position);
        image.pixels[index] = pixel;
    }*/

    /// Create a new flattened pixel storage, checking the length of the provided pixels vector.
    pub fn new(resolution: impl Into<Vec2<usize>>, pixels: Vec<T>) -> Self {
        let size = resolution.into();
        assert_eq!(size.area(), pixels.len(), "expected {} samples, but vector length is {}", size.area(), pixels.len());
        Self { resolution: size, pixels }
    }

    /// Compute the flat index of a specific pixel. Returns a range of either 3 or 4 samples.
    /// The computed index can be used with `PixelVec.samples[index]`.
    /// Panics for invalid sample coordinates.
    #[inline]
    pub fn compute_pixel_index(&self, position: Vec2<usize>) -> usize {
        position.flat_index_for_size(self.resolution)
    }
}

use crate::image::validate_results::ValidateValueResult;

impl<Px> ValidateValueResult for PixelVec<Px> where Px: ValidateValueResult {
    fn validate_value_result(&self, lossy_self: &Self, max_difference: Option<f32>, nan_to_zero: bool) -> bool {
        self.resolution == lossy_self.resolution
            && self.pixels.as_slice().validate_value_result(&lossy_self.pixels.as_slice(), max_difference, nan_to_zero)
    }
}

impl<Px> GetPixel for PixelVec<Px> where Px: Clone + Sync {
    type Pixel = Px;
    fn get_pixel(&self, position: Vec2<usize>) -> Self::Pixel {
        get_pixel_from_vec(self, position).clone()
    }
}

/// Create a new `PixelVec<T>`, given the pixel resolution of the image.
/// Can usually be used as a function reference instead of calling it directly.
#[inline] pub fn create_pixel_vec<Pixel: Clone + Default, Channels>(resolution: Vec2<usize>, _: &Channels) -> PixelVec<Pixel> {
    PixelVec { resolution, pixels: vec![Pixel::default(); resolution.area()] }
}

/// Examine a pixel of a `PixelVec<T>` image.
/// Can usually be used as a function reference instead of calling it directly.
#[inline]
pub fn get_pixel_from_vec<Pixel>(image: &PixelVec<Pixel>, position: Vec2<usize>) -> &Pixel where Pixel: Sync {
    &image.pixels[image.compute_pixel_index(position)]
}

/// Update a pixel of a `PixelVec<T>` image.
/// Can usually be used as a function reference instead of calling it directly.
#[inline]
pub fn set_pixel_in_vec<Pixel>(image: &mut PixelVec<Pixel>, position: Vec2<usize>, pixel: Pixel) {
    let index = image.compute_pixel_index(position);
    image.pixels[index] = pixel;
}

use std::fmt::*;

impl<T> Debug for PixelVec<T> {
    #[inline] fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}; {}]", std::any::type_name::<T>(), self.pixels.len())
    }
}

