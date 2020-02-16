
// exr imports
extern crate exr;
use exr::image::rgba::*;
use exr::math::Vec2;
use exr::meta::Attributes;

/// Read an RGBA image.
/// Uses multicore decompression where appropriate.
#[test]
fn read_image() {

    struct Image {

        /// Resolution of the image.
        size: (usize, usize),

        /// A typical RGBA sample array.
        ///
        /// Stores in order red, green, blue, then alpha components.
        /// All lines of the image are appended one after another, top to bottom.
        rgba: Vec<f32>,
    }

    impl NewImage for Image {
        fn new(size: Vec2<usize>, _attributes: &Attributes) -> Self {
            Self {
                size: (size.0, size.1),
                rgba: vec![ 0.0; size.area() * 4 ]
            }
        }

        fn set_sample(&mut self, index: Vec2<usize>, channel: Channel, value: f32) {
            let channel_offset = match channel {
                Channel::Red => 0,
                Channel::Green => 1,
                Channel::Blue => 2,
                Channel::Alpha => 3,
            };

            let y = self.size.1 - index.1; // invert y coordinate
            let flattened_pixel_index = self.size.0 * y + index.0; // calculate flattened index as `y * width + x`
            self.rgba[flattened_pixel_index * 4 + channel_offset] = value; // four values per pixel requires `*4`
        }
    }

    Image::read_from_file("./testout/noisy.exr", true);
}