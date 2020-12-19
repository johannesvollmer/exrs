//! Crop away unwanted pixels. Includes automatic detection of bounding rectangle.

use crate::meta::attribute::IntegerBounds;
use crate::math::Vec2;
use crate::image::{Layer, FlatSamples, RgbaChannels, AnyChannels, RgbaPixel};
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
    type Cropped;

    /// Panics for invalid (larger than previously) bounds
    /// Does not necessarily reduce allocation size of the current image
    fn crop(self, bounds: IntegerBounds) -> Self::Cropped;

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
    Cropped (Cropped),
    Empty { original: Old }
}

pub trait CropWhere<Sample>: Sized {
    type Cropped;
    fn crop_where(self, discard_if: impl Fn(Sample) -> bool) -> CropResult<Self::Cropped, Self>;
}

pub trait CropWhereEq<SampleEq>: Sized {
    type Cropped;

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

/*impl<C, S> CropWhereEq<S> for C where C: CropWhere<S>, S: PartialEq {
    type Cropped = <Self as CropWhere<S>>::Cropped;

    fn crop_where_eq(self, discard_color: S) -> CropResult<Self::Cropped, Self> {
        self.crop_where(|sample| sample == discard_color)
    }
}*/


/// A smaller window into an existing rgba storage
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CroppedRgba<Samples> {
    pub offset: Vec2<usize>,
    pub original_storage: Samples
}

impl<Samples> CroppedRgba<Samples> {
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
    fn get_pixel(&self, position: Vec2<usize>) -> RgbaPixel {
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

/*impl<Samples, R,G,B,A> CropWhereEq<(Option<R>, Option<G>, Option<B>, Option<A>)> for Layer<RgbaChannels<Samples>>
    where Samples: GetRgbaPixel, R: Into<Sample>, G: Into<Sample>, B: Into<Sample>, A: Into<Sample>
{
    type Cropped = <Self as Crop>::Cropped;

    fn crop_where_eq(self, (r,g,b,a): (Option<R>, Option<G>, Option<B>, Option<A>)) -> CropResult<Self::Cropped, Self> {
        self.crop_where(|rgba: RgbaPixel| {
            let discard_red = if let Some(r) = r { rgba.red == r.into() } else { false };
            let discard_green = if let Some(g) = g { rgba.green == g.into() } else { false };
            let discard_blue = if let Some(b) = b { rgba.blue == b.into() } else { false };
            let discard_alpha = if let Some(a) = a { rgba.alpha_or_default() == a.into() } else { false };
            discard_red && discard_green && discard_blue && discard_alpha
        })
    }
}*/
impl<Samples> CropWhereEq<RgbaPixel> for Layer<RgbaChannels<Samples>> where Samples: GetRgbaPixel {
    type Cropped = <Self as Crop>::Cropped;

    fn crop_where_eq(self, rgba: RgbaPixel) -> CropResult<Self::Cropped, Self> {
        self.crop_where(|sample| sample == rgba)
    }
}


impl<Samples> InspectSample for Layer<RgbaChannels<Samples>> where Samples: GetRgbaPixel {
    type Sample = RgbaPixel;
    fn inspect_sample(&self, local_index: Vec2<usize>) -> Self::Sample {
        self.channel_data.storage.get_pixel(local_index)
    }
}


// ALGORITHM IDEA: for arbitrary channels, find the least transparent layer,
// and process that first, keeping the processed bounds as starting point for the other layers

/*impl<'s> InspectSample for &'s Layer<AnyChannels<FlatSamples>> {
    type Sample = FlatSampleIterator<'s>;
    fn inspect_sample(&self, local_index: Vec2<usize>) -> Self::Sample {}
}*/

/*impl<I: Iterator<Item=Sample>> CropWhere<I> for Layer<AnyChannels<FlatSamples>>
{
    type Cropped = Self;

    fn crop_where(self, discard_if: impl Fn(I) -> bool) -> CropResult<Self::Cropped, Self> {
        let bounds = try_find_smaller_bounds(
            self.absolute_bounds(),
            |position| !discard_if(self.samples_at(position))
        );

        self.try_crop(bounds)
    }
}*/

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
                let kept_old_lines = samples.chunks_exact(old_width).skip(y_start).take(new_height);
                let trimmed_lines = kept_old_lines.map(|line| &line[x_range.clone()]);
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
    pub fn or_none_if_empty(self) -> Option<Cropped> {
        match self {
            CropResult::Cropped (cropped) => Some(cropped),
            CropResult::Empty { .. } => None,
        }
    }

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
    fn find_bounds1() {
        let pixels = vec![
            vec![ 0, 1, 1, 1, 1, 0 ],
            vec![ 0, 1, 1, 1, 1, 0 ],
            vec![ 0, 1, 1, 1, 1, 0 ],
        ];

        let bounds = try_find_smaller_bounds(
            IntegerBounds::new((0,0), (6,3)),
            |position| pixels[position.y()][position.x()] != 0
        );

        assert_eq!(bounds, Some(IntegerBounds::new((1,0), (4,3))))
    }

    #[test]
    fn find_bounds2() {
        let pixels = vec![
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 1, 1, 1, 0, 0 ],
            vec![ 0, 0, 1, 1, 1, 0, 0 ],
            vec![ 0, 0, 1, 1, 1, 0, 0 ],
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
        ];

        let bounds = try_find_smaller_bounds(
            IntegerBounds::new((0,0), (7,7)),
            |position| pixels[position.y()][position.x()] != 0
        );

        assert_eq!(bounds, Some(IntegerBounds::new((2,2), (3,3))))
    }

    #[test]
    fn find_bounds3() {
        let pixels = vec![
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0, 1, 0, 0 ], // TODO is this upside down??
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0, 0, 0, 0 ],
        ];

        let bounds = try_find_smaller_bounds(
            IntegerBounds::new((0,0), (7,7)),
            |position| pixels[position.y()][position.x()] != 0
        );

        assert_eq!(bounds, Some(IntegerBounds::new((4,2), (1,1))))
    }

    #[test]
    fn find_bounds6() {
        let pixels = vec![
            vec![ 1, 0, 0, 0 ],
            vec![ 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0 ],
        ];

        let bounds = try_find_smaller_bounds(
            IntegerBounds::new((0,0), (4,4)),
            |position| pixels[position.y()][position.x()] != 0
        );

        assert_eq!(bounds, Some(IntegerBounds::new((0,0), (1,1))))
    }

    #[test]
    fn find_bounds8() {
        let pixels = vec![
            vec![ 1, 0, 0, 0 ],
            vec![ 0, 1, 0, 0 ],
            vec![ 0, 0, 0, 0 ],
            vec![ 0, 0, 0, 0 ],
        ];

        let bounds = try_find_smaller_bounds(
            IntegerBounds::new((0,0), (4,4)),
            |position| pixels[position.y()][position.x()] != 0
        );

        assert_eq!(bounds, Some(IntegerBounds::new((0,0), (2,2))))
    }

    #[test]
    fn find_bounds4() {
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




