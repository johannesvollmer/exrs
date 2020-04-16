
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image::rgba::{Pixel};

/// Read an RGBA image, increase the exposure, and then write it back.
/// Uses multi-core compression where appropriate.
fn main() {

    /// This is an example of a custom image type.
    /// You use your own image struct here.
    // This struct trades sub-optimal memory-efficiency for clarity,
    // because this is an example, and does not have to be perfectly efficient.
    #[derive(Debug, PartialEq)]
    struct CustomPixels { lines: Vec<Vec<[f16; 4]>> };

    // read the image from a file
    let (mut image_info, mut pixels) = {

        impl rgba::SetPixels for CustomPixels {

            // set a single pixel with red, green, blue, and optionally and alpha value.
            // (this method is also called for f16 or u32 values, if you do not implement the other methods in this trait)
            fn set_pixel(&mut self, position: Vec2<usize>, pixel: rgba::Pixel) {

                // convert all samples, including alpha, to 16-bit floats, and then store them in the array
                // (inserts 1.0 as default alpha if no alpha channel is in the file)
                let pixel_f16_array: [f16; 4] = pixel.into();

                self.lines[position.y()][position.x()] = pixel_f16_array;
            }
        }

        let create_image = |image: &rgba::ImageInfo| {
            println!("loaded image {:#?}", image);

            let default_rgba_pixel = [f16::ZERO, f16::ZERO, f16::ZERO, f16::ONE];
            let default_line = vec![default_rgba_pixel; image.resolution.width()];
            let lines = vec![default_line; image.resolution.height()];
            CustomPixels { lines }
        };

        // actually start reading the file with custom pixels
        rgba::ImageInfo::read_pixels_from_file(
            "tests/images/valid/openexr/MultiResolution/Kapaa.exr",
            read_options::high(),
            create_image
        ).unwrap()
    };


    {   // increase exposure of all pixels

        assert!(
            !image_info.channels.0.is_linear && !image_info.channels.0.is_linear && !image_info.channels.0.is_linear,
            "exposure adjustment is only implemented for srgb data"
        );

        for line in &mut pixels.lines {
            for pixel in line {
                for sample in &mut pixel[0..3] { // only modify rgb, not alpha
                    let linear = sample.to_f32().powf(2.2); // convert srgb to linear rgb

                    let brightened = linear * 3.0;

                    let sample_32 = brightened.powf(1.0/2.2); // convert linear rgb to srgb
                    *sample = f16::from_f32(sample_32);
                }
            }
        }

        // also update meta data after modifying the image
        if let Some(exposure) = &mut image_info.layer_attributes.exposure {
            println!("increased exposure from {}s to {}s", exposure, *exposure * 3.0);
            *exposure *= 3.0;
        }
    }


    {   // write the image to a file

        // expose the pixels of our image (you could also pass a closure capturing our image variable)
        impl rgba::GetPixels for CustomPixels {

            // extract a single pixel with red, green, blue, and optionally and alpha value.
            fn get_pixel(&self, position: Vec2<usize>) -> Pixel {
                let rgba_f16_array: [f16; 4] = self.lines[position.y()][position.x()];
                rgba::Pixel::from(rgba_f16_array)
            }
        }

        image_info.write_pixels_to_file("tests/images/out/exposure_adjusted.exr", write_options::high(), &pixels).unwrap();
    }
}