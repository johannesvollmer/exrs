
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image::rgba::{SampleIndex, GetPixels};

/// Read an RGBA image, increase the exposure, and then write it back.
/// Uses multicore compression where appropriate.
fn main() {

    /// This is an example of a custom image type.
    /// You use your own image struct here.
    // This is actually not the optimal way to store pixels in an efficient manner.
    #[derive(Debug, PartialEq)]
    struct CustomUserPixels { lines: Vec<Vec<[f16; 4]>> };

    // read the image from a file
    let mut image = {
        impl rgba::CreatePixels for CustomUserPixels {

            // allocate a new pixel storage based on the (still empty) image
            fn new(image: &rgba::Image<()>) -> Self {
                println!("loaded image {:#?}", image);

                let default_pixel = [f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO];
                let default_line = vec![default_pixel; image.resolution.0];
                CustomUserPixels { lines: vec![default_line; image.resolution.1] }
            }

            // set a single value, which is either red, green, blue, or alpha.
            // (this method is also called for f16 or u32 values, if you do not implement the other methods in this trait)
            fn set_sample_f32(image: &mut rgba::Image<Self>, index: SampleIndex, sample: f32) {
                image.data.lines[index.position.1][index.position.0][index.channel] = f16::from_f32(sample); // TODO gamma correction & more?
            }
        }

        rgba::Image::<CustomUserPixels>::read_from_file(
            "tests/images/valid/openexr/Beachball/multipart.0004.exr",
            read_options::high()
        ).unwrap()
    };


    {
        let channel_linearity = [
            image.channels.0.is_linear,
            image.channels.1.is_linear,
            image.channels.2.is_linear
        ];

        // increase exposure of all pixels
        for line in &mut image.data.lines {
            for pixel in line {
                for (channel_index, sample) in (&mut pixel[0..3]).iter_mut().enumerate() { // rgb, but not alpha
                    let sample_32 = sample.to_f32();
                    let is_linear = channel_linearity[channel_index];
                    let linear = if is_linear { sample_32 } else { sample_32.powf(2.2) };

                    let brightened = linear * 3.0;

                    let sample_32 = if is_linear { brightened } else { brightened.powf(1.0/2.2) };
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
        impl GetPixels for CustomUserPixels {
            // query a single sample, which is either red, green, blue, or alpha.
            // (this method is also called for f16 or u32 values, if you do not implement the other methods in this trait)
            fn get_sample_f32(image: &rgba::Image<Self>, index: SampleIndex) -> f32 {
                image.data.lines[index.position.1][index.position.0][index.channel].to_f32()
            }
        }

        image.write_to_file("tests/images/out/written_copy.exr", write_options::high()).unwrap();
    }
}