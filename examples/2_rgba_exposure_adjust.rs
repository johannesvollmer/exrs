
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image::read::RgbaChannelsInfo;
use exr::image::write::channels::GetRgbaPixel;

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
    let mut image = read().no_deep_data()
        .largest_resolution_level()
        .rgba_channels(
            // create our custom image based on the file info
            |image: &RgbaChannelsInfo| -> CustomPixels {
                println!("loaded image {:#?}", image);

                let default_rgba_pixel = [f16::ZERO, f16::ZERO, f16::ZERO, f16::ONE];
                let default_line = vec![default_rgba_pixel; image.resolution.width()];
                let lines = vec![default_line; image.resolution.height()];
                CustomPixels { lines }
            },

            // set a single pixel with red, green, blue, and optionally and alpha value.
            |image: &mut CustomPixels, position: Vec2<usize>, pixel: RgbaPixel| {

                // convert all samples, including alpha, to four 16-bit floats
                let pixel_f16_array: [f16; 4] = pixel.into();

                // insert the values into out custom image
                image.lines[position.y()][position.x()] = pixel_f16_array;
            }
        )
        .first_valid_layer()
        .all_attributes()
        .from_file("tests/images/valid/openexr/MultiResolution/Kapaa.exr")
        .unwrap();

    let exposure_multiplier = 2.0;

    {   // increase exposure of all pixels
        for line in &mut image.layer_data.channel_data.storage.lines {
            for pixel in line {
                for sample in &mut pixel[0..3] { // only modify rgb, not alpha
                    // no gamma correction necessary because
                    // exposure adjustment should be done in linear color space
                    *sample = f16::from_f32(sample.to_f32() * exposure_multiplier);
                }
            }
        }

        // also update meta data after modifying the image
        if let Some(exposure) = &mut image.layer_data.attributes.exposure {
            println!("increased exposure from {}s to {}s", exposure, *exposure * exposure_multiplier);
            *exposure *= exposure_multiplier;
        }
    }

    // enable writing our custom pixel storage to a file
    // TODO this should be passed as a closure to the `write().rgba_with(|x| y)` call
    impl GetRgbaPixel for CustomPixels {
        fn get_pixel(&self, position: Vec2<usize>) -> RgbaPixel {
            let rgba_f16_array: [f16; 4] = self.lines[position.y()][position.x()];
            RgbaPixel::from(rgba_f16_array)
        }
    }

     // write the image to a file
    image
        .write().to_file("tests/images/out/exposure_adjusted.exr")
        .unwrap();

    println!("created file exposure_adjusted.exr");
}