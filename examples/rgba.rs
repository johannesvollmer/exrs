
// exr imports
extern crate exr;
use exr::prelude::*;

/// Read an RGBA image, increase the exposure, and then write it back.
/// Uses multi-core compression where appropriate.
fn main() {

    /// This is an example of a custom image type.
    /// You use your own image struct here.
    // This is actually not the optimal way to store pixels in an efficient manner.
    #[derive(Debug, PartialEq)]
    struct CustomUserPixels { lines: Vec<Vec<[f16; 4]>> };

    // read the image from a file
    let (mut image, mut pixels) = {

        // allocate a new pixel storage based on the image
        fn create_pixels(image: &rgba::Image) -> CustomUserPixels {
            println!("loaded image {:#?}", image);

            let default_rgba_pixel = [f16::ZERO, f16::ZERO, f16::ZERO, f16::ONE];
            let default_line = vec![default_rgba_pixel; image.resolution.0];
            CustomUserPixels { lines: vec![default_line; image.resolution.1] }
        }

        impl rgba::SetPixels for CustomUserPixels {

            // set a single value, which is either red, green, blue, or alpha.
            // (this method is also called for f16 or u32 values, if you do not implement the other methods in this trait)
            fn set_pixel(&mut self, _: &rgba::Image, position: Vec2<usize>, pixel: rgba::Pixel) {
                self.lines[position.1][position.0] = [
                    pixel.red.to_f16(), pixel.green.to_f16(), pixel.blue.to_f16(),
                    pixel.alpha.map(|a| a.to_f16()).unwrap_or(f16::ONE),
                ];
            }
        }

        rgba::Image::read_from_file(
            "tests/images/valid/openexr/MultiResolution/Kapaa.exr",
            read_options::high(),
            create_pixels
        ).unwrap()
    };


    {   // increase exposure of all pixels
        assert!(
            !image.channels.0.is_linear && !image.channels.0.is_linear && !image.channels.0.is_linear,
            "exposure adjustment is only implemented for srgb data"
        );

        for line in &mut pixels.lines {
            for pixel in line {
                for sample in &mut pixel[0..3] { // only modify rgb, not alpha
                    let linear = sample.to_f32().powf(2.2);

                    let brightened = linear * 3.0;

                    let sample_32 = brightened.powf(1.0/2.2);
                    *sample = f16::from_f32(sample_32);
                }
            }
        }

        // also update meta data after modifying the image
        if let Some(exposure) = &mut image.layer_attributes.exposure {
            *exposure *= 3.0;
        }
    }


    {   // write the image to a file
        // query a single sample, which is either red, green, blue, or alpha.
        // (this method is also called for f16 or u32 values, if you do not implement the other methods in this trait)
        let get_pixel = |_image: &rgba::Image, position: Vec2<usize>| {
            let [r, g, b, a] = pixels.lines[position.1][position.0];
            rgba::Pixel::rgba(r, g, b, a)
        };

        image.write_to_file("tests/images/out/exposure_adjusted.exr", write_options::high(), &get_pixel).unwrap();
    }
}