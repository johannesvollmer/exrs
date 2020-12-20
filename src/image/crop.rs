//! Crop away unwanted pixels. Includes automatic detection of bounding rectangle.
//! Currently does not support deep data and resolution levels.

use crate::meta::attribute::IntegerBounds;
use crate::math::Vec2;
use crate::image::{Layer, FlatSamples, RgbaChannels, AnyChannels, RgbaPixel, FlatSamplesPixel};
use crate::image::write::channels::GetRgbaPixel;
use crate::meta::header::LayerAttributes;
use crate::block::samples::Sample;

/// Something that has a two-dimensional rectangular shape
pub trait GetBounds {

    /// The bounding rectangle of this pixel grid.
    fn bounds(&self) -> IntegerBounds;
}

/// Inspect the pixels in this image to determine where to crop some away
pub trait InspectSample: GetBounds {

    /// The type of pixel in this pixel grid.
    type Sample;

    /// Index is not in world coordinates. Position `(0,0)` always represents the bottom left pixel.
    fn inspect_sample(&self, local_index: Vec2<usize>) -> Self::Sample;
}

/// Crop some pixels ways when specifying a smaller rectangle
pub trait Crop: Sized {

    /// The type of  this image after cropping (probably the same as before)
    type Cropped;

    /// Reduce your image to a smaller part, usually to save memory.
    /// Panics for invalid (larger than previously) bounds.
    /// The bounds are specified in absolute coordinates.
    /// Does not necessarily reduce allocation size of the current image:
    /// An rgba image will only be viewed in a smaller window instead of reallocating.
    fn crop(self, bounds: IntegerBounds) -> Self::Cropped;

    /// Reduce your image to a smaller part, usually to save memory.
    /// Crop if bounds are specified, return the original if no bounds are specified.
    fn try_crop(self, bounds: Option<IntegerBounds>) -> CropResult<Self::Cropped, Self> {
        match bounds {
            Some(bounds) => CropResult::Cropped(self.crop(bounds)),
            None => CropResult::Empty { original: self },
        }
    }
}

/// Cropping an image fails if the image is fully transparent.
/// Use [`or_crop_to_1x1_if_empty`] or [`or_none_if_empty`] to obtain a normal image again.
#[must_use]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CropResult<Cropped, Old> {

    /// The image contained some pixels and has been cropped or left untouched
    Cropped (Cropped),

    /// All pixels in the image would be discarded, removing the whole image
    Empty {

        /// The uncropped image after a failed crop operation
        original: Old
    }
}

/// Crop away unwanted pixels from the border if they match the specified rule.
pub trait CropWhere<Sample>: Sized {

    /// The type of the cropped image (probably the same as the original image)
    type Cropped;

    /// Crop away unwanted pixels from the border if they match the specified rule , usually to save memory.
    fn crop_where(self, discard_if: impl Fn(Sample) -> bool) -> CropResult<Self::Cropped, Self>;
}

/// Crop away unwanted pixels from the border if they match the specified color.
pub trait CropWhereEq<SampleEq>: Sized {

    /// The type of the cropped image (probably the same as the original image)
    type Cropped;

    /// Crop away unwanted pixels from the border if they match the specified color , usually to save memory.
    /// If you want discard based on a rule, use `crop_where` with a closure instead.
    fn crop_where_eq(self, discard_color: SampleEq) -> CropResult<Self::Cropped, Self>;
}

impl<T> CropWhere<T::Sample> for T where T: Crop + InspectSample {
    type Cropped = <Self as Crop>::Cropped;

    fn crop_where(self, discard_if: impl Fn(T::Sample) -> bool) -> CropResult<Self::Cropped, Self> {
        let smaller_bounds = {
            let keep_if = |position| !discard_if(self.inspect_sample(position));
            try_find_smaller_bounds(self.bounds(), keep_if)
        };

        self.try_crop(smaller_bounds)
    }
}

/// A smaller window into an existing rgba storage
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CroppedRgba<Samples> {

    /// When cropped, the image pixels are offset by this value
    pub offset: Vec2<usize>,

    /// Your uncropped pixelgrid
    pub original_storage: Samples
}

impl<Samples> CroppedRgba<Samples> {

    /// Wrap a layer in a cropped view with adjusted bounds, but without reallocating your pixels
    pub fn crop_layer(bounds: IntegerBounds, layer: Layer<RgbaChannels<Samples>>) -> Layer<RgbaChannels<CroppedRgba<Samples>>> {
        Layer {
            channel_data: RgbaChannels {
                sample_types: layer.channel_data.sample_types,
                storage: CroppedRgba {
                    original_storage: layer.channel_data.storage,
                    offset: (bounds.position - layer.attributes.layer_position)
                        .to_usize("invalid cropping bounds").unwrap()
                }
            },

            size: bounds.size,

            attributes: LayerAttributes {
                layer_position: bounds.position,
                .. layer.attributes
            },

            encoding: layer.encoding
        }
    }
}

// enable writing the cropped rgba channel contents to a file
impl<Samples> GetRgbaPixel for CroppedRgba<Samples> where Samples: GetRgbaPixel {
    type Pixel = Samples::Pixel;
    fn get_pixel(&self, position: Vec2<usize>) -> Samples::Pixel {
        self.original_storage.get_pixel(position + self.offset)
    }
}

impl<Samples> Crop for Layer<RgbaChannels<Samples>> {
    type Cropped = Layer<RgbaChannels<CroppedRgba<Samples>>>;

    /// Does not actually reallocate, but instead only creates a smaller window into the old storage
    fn crop(self, bounds: IntegerBounds) -> Self::Cropped {
        CroppedRgba::crop_layer(bounds, self)
    }
}

impl<Samples, Pixel> CropWhereEq<Pixel> for Layer<RgbaChannels<Samples>> where Pixel: Into<RgbaPixel>, Samples: GetRgbaPixel {
    type Cropped = <Self as Crop>::Cropped;

    fn crop_where_eq(self, rgba: Pixel) -> CropResult<Self::Cropped, Self> {
        let rgba_pixel: RgbaPixel = rgba.into();
        self.crop_where(|sample| sample.into() == rgba_pixel)
    }
}

impl<Samples> InspectSample for Layer<RgbaChannels<Samples>> where Samples: GetRgbaPixel {
    type Sample = Samples::Pixel;
    fn inspect_sample(&self, local_index: Vec2<usize>) -> Samples::Pixel {
        self.channel_data.storage.get_pixel(local_index)
    }
}

// ALGORITHM IDEA: for arbitrary channels, find the most desired channel,
// and process that first, keeping the processed bounds as starting point for the other layers

// TODO no allocation? should be borrowable
impl CropWhere<FlatSamplesPixel> for Layer<AnyChannels<FlatSamples>> {
    type Cropped = Self;

    fn crop_where(self, discard_if: impl Fn(FlatSamplesPixel) -> bool) -> CropResult<Self::Cropped, Self> {
        let bounds = try_find_smaller_bounds(
            self.bounds(),
            |position| !discard_if(self.sample_vec_at(position))
        );

        self.try_crop(bounds)
    }
}

impl<Slice> CropWhereEq<Slice> for Layer<AnyChannels<FlatSamples>>
    where Slice: AsRef<[Option<Sample>]>
{
    type Cropped = Self;

    fn crop_where_eq(self, discard_color: Slice) -> CropResult<Self::Cropped, Self> {
        let discard_color = discard_color.as_ref();
        assert_eq!(discard_color.len(), self.channel_data.list.len());

        let bounds = try_find_smaller_bounds(
            self.bounds(),
            |position| !(
                self.samples_at(position).map(Option::Some)
                    .eq(discard_color.iter().map(|sample| *sample))
            )
        );

        self.try_crop(bounds)
    }
}

impl Crop for Layer<AnyChannels<FlatSamples>> {
    type Cropped = Self;

    fn crop(mut self, absolute_bounds: IntegerBounds) -> Self::Cropped {
        let bounds = absolute_bounds.with_origin(-self.attributes.layer_position);

        assert!(self.absolute_bounds().contains(absolute_bounds), "bounds not valid for layer dimensions");
        assert!(bounds.size.area() > 0, "the cropped image would be empty");

        let start_x = bounds.position.x() as usize; // safe, because just checked above
        let start_y = bounds.position.y() as usize; // safe, because just checked above

        if bounds.size != self.size {
            fn crop_samples<T: Copy>(samples: &[T], old_width: usize, new_height: usize, x_range: std::ops::Range<usize>, y_start: usize) -> Vec<T> {
                let filtered_lines = samples.chunks_exact(old_width).skip(y_start).take(new_height);
                let trimmed_lines = filtered_lines.map(|line| &line[x_range.clone()]);
                trimmed_lines.flatten().map(|x| *x).collect() // TODO does this use memcpy?
            }

            for channel in &mut self.channel_data.list {
                let samples: &mut FlatSamples = &mut channel.sample_data;
                let x_range = start_x .. start_x + bounds.size.width();

                match samples {
                    FlatSamples::F16(samples) => *samples = crop_samples(samples, self.size.width(), bounds.size.height(), x_range.clone(), start_y),
                    FlatSamples::F32(samples) => *samples = crop_samples(samples, self.size.width(), bounds.size.height(), x_range.clone(), start_y),
                    FlatSamples::U32(samples) => *samples = crop_samples(samples, self.size.width(), bounds.size.height(), x_range.clone(), start_y),
                }
            }

            self.size = bounds.size;
            self.attributes.layer_position = absolute_bounds.position;
        }

        self
    }
}



/// Return the smallest bounding rectangle including all pixels that satisfy the predicate.
/// Worst case: Fully transparent image, visits each pixel once.
/// Best case: Fully opaque image, visits four pixels.
/// Returns `None` if the image is fully transparent.
/// Returns `[(0,0), size]` if the image is fully opaque.
fn try_find_smaller_bounds(current_bounds: IntegerBounds, pixel_at: impl Fn(Vec2<usize>) -> bool) -> Option<IntegerBounds> {
    assert_ne!(current_bounds.size.area(), 0, "cannot find smaller bounds of an image with zero width or height");
    let Vec2(width, height) = current_bounds.size;

    // scans top to bottom (left to right)
    let first_top_left_pixel = (0 .. height).rev()
        .flat_map(|y| (0 .. width).map(move |x| Vec2(x,y)))
        .find(|&position| pixel_at(position));

    let first_top_left_pixel = {
        if let Some(xy) = first_top_left_pixel { xy }
        else { return None }
    };

    // scans bottom to top (right to left)
    let first_bottom_right_pixel = (0 .. first_top_left_pixel.y()) // excluding the top line
        .flat_map(|y| (first_top_left_pixel.x() + 1 .. width).rev().map(move |x| Vec2(x, y))) // excluding some left pixel
        .find(|&position| pixel_at(position))
        .unwrap_or(first_top_left_pixel); // did not inspect but we know top has a pixel

    // now we know exactly how much we can throw away top and bottom
    let top = first_top_left_pixel.y();
    let bottom = first_bottom_right_pixel.y();

    // but we only now some random left and right bounds which we need to refine
    let mut min_left_x = first_top_left_pixel.x();
    let mut max_right_x = first_bottom_right_pixel.x();

    // requires for loop, because bounds change while searching
    for y in (bottom ..= top).rev() {
        // escape the loop if there is nothing left to crop
        if min_left_x == 0 && max_right_x == width - 1 { break; }

        // search from right bound to image center for existing pixels
        if max_right_x != width - 1 {
            max_right_x = (max_right_x + 1 .. width).rev() // excluding current max
                .find(|&x| pixel_at(Vec2(x, y)))
                .unwrap_or(max_right_x);
        }

        // search from left bound to image center for existing pixels
        if min_left_x != 0 {
            min_left_x = (0 .. min_left_x) // excluding current min
                .find(|&x| pixel_at(Vec2(x, y)))
                .unwrap_or(min_left_x);
        }
    }

    let local_start = Vec2(min_left_x, bottom);
    let local_end = Vec2(max_right_x + 1, top + 1);
    Some(IntegerBounds::new(
        current_bounds.position + local_start.to_i32(),
        local_end - local_start
    ))
}

impl<S> GetBounds for Layer<S> {
    fn bounds(&self) -> IntegerBounds {
        self.absolute_bounds()
    }
}

impl<Cropped, Original> CropResult<Cropped, Original> {

    /// If the image was fully empty, return `None`, otherwise return `Some(cropped_image)`.
    pub fn or_none_if_empty(self) -> Option<Cropped> {
        match self {
            CropResult::Cropped (cropped) => Some(cropped),
            CropResult::Empty { .. } => None,
        }
    }

    /// If the image was fully empty, crop to one single pixel of all the transparent pixels instead,
    /// leaving the layer intact while reducing memory usage.
    pub fn or_crop_to_1x1_if_empty(self) -> Cropped where Original: Crop<Cropped=Cropped> + GetBounds {
        match self {
            CropResult::Cropped (cropped) => cropped,
            CropResult::Empty { original } => {
                let bounds = original.bounds();
                if bounds.size == Vec2(0,0) { panic!("rgba layer has width and height of zero") }
                original.crop(IntegerBounds::new(bounds.position, Vec2(1,1)))
            },
        }
    }
}



#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn find_bounds() {
        fn find_bounds(offset: Vec2<i32>, lines: &Vec<Vec<i32>>) -> IntegerBounds {
            if let Some(first_line) = lines.first() {
                assert!(lines.iter().all(|line| line.len() == first_line.len()), "invalid test input");
                IntegerBounds::new(offset, (first_line.len(), lines.len()))
            }
            else {
                IntegerBounds::new(offset, (0,0))
            }
        }

        fn assert_found_smaller_bounds(offset: Vec2<i32>, uncropped_lines: Vec<Vec<i32>>, cropped_lines: Vec<Vec<i32>>) {
            let old_bounds = find_bounds(offset, &uncropped_lines); // TODO offset

            let found_bounds = try_find_smaller_bounds(
                old_bounds, |position| uncropped_lines[position.y()][position.x()] != 0
            ).unwrap();

            // convert bounds to relative index
            let found_bounds = found_bounds.with_origin(-offset);
            for (y, uncropped_line) in uncropped_lines[found_bounds.position.y() as usize .. found_bounds.end().y() as usize].iter().enumerate() {
                for (x, &value) in uncropped_line[found_bounds.position.x() as usize .. found_bounds.end().x() as usize].iter().enumerate() {
                    assert_eq!(value, cropped_lines[y][x])
                }
            }
        }

        assert_found_smaller_bounds(
            Vec2(-3,-3),

            vec![
                vec![ 2, 3, 4 ],
                vec![ 2, 3, 4 ],
            ],

            vec![
                vec![ 2, 3, 4 ],
                vec![ 2, 3, 4 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(-3,-3),

            vec![
                vec![ 2 ],
            ],

            vec![
                vec![ 2 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(-3,-3),

            vec![
                vec![ 0 ],
                vec![ 2 ],
                vec![ 0 ],
                vec![ 0 ],
            ],

            vec![
                vec![ 2 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(-3,-3),

            vec![
                vec![ 0, 0, 0, 3, 0 ],
            ],

            vec![
                vec![ 3 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(3,3),

            vec![
                vec![ 0, 1, 1, 2, 1, 0 ],
                vec![ 0, 1, 3, 1, 1, 0 ],
                vec![ 0, 1, 1, 1, 1, 0 ],
            ],

            vec![
                vec![ 1, 1, 2, 1 ],
                vec![ 1, 3, 1, 1 ],
                vec![ 1, 1, 1, 1 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(1,3),

            vec![
                vec![ 1, 0, 0, 0, ],
                vec![ 0, 0, 0, 0, ],
                vec![ 0, 0, 0, 0, ],
            ],

            vec![
                vec![ 1 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(1,3),

            vec![
                vec![ 0, 0, 0, 0, ],
                vec![ 0, 1, 0, 0, ],
                vec![ 0, 0, 0, 0, ],
            ],

            vec![
                vec![ 1 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(-1,-3),

            vec![
                vec![ 0, 0, 0, 0, ],
                vec![ 0, 0, 0, 1, ],
                vec![ 0, 0, 0, 0, ],
            ],

            vec![
                vec![ 1 ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(-1,-3),

            vec![
                vec![ 0, 0, 0, 0, 0, 0, 0 ],
                vec![ 0, 0, 0, 0, 0, 0, 0 ],
                vec![ 0, 0, 1, 1, 1, 0, 0 ],
                vec![ 0, 0, 1, 1, 1, 0, 0 ],
                vec![ 0, 0, 1, 1, 1, 0, 0 ],
                vec![ 0, 0, 0, 0, 0, 0, 0 ],
                vec![ 0, 0, 0, 0, 0, 0, 0 ],
            ],

            vec![
                vec![ 1, 1, 1 ],
                vec![ 1, 1, 1 ],
                vec![ 1, 1, 1 ],
            ]
        );


        assert_found_smaller_bounds(
            Vec2(-1,-3),

            vec![
                vec![ 0, 0, 1, 0, ],
                vec![ 0, 0, 0, 1, ],
                vec![ 0, 0, 0, 0, ],
            ],

            vec![
                vec![ 1, 0, ],
                vec![ 0, 1, ],
            ]
        );

        assert_found_smaller_bounds(
            Vec2(-1,-3),

            vec![
                vec![ 1, 0, 0, 0, ],
                vec![ 0, 1, 0, 0, ],
                vec![ 0, 0, 0, 0, ],
                vec![ 0, 0, 0, 0, ],
            ],

            vec![
                vec![ 1, 0, ],
                vec![ 0, 1, ],
            ]
        );
    }


    #[test]
    fn find_no_bounds() {
        let pixels = vec![
            vec![ 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0 ],
        ];

        let bounds = try_find_smaller_bounds(
            IntegerBounds::new((0,0), (4,3)),
            |position| pixels[position.y()][position.x()] != 0
        );

        assert_eq!(bounds, None)
    }

}




