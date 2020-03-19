
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image::rgba::{SampleIndex, GetPixels};

/// Read an RGBA image and then write it back.
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


    {   // brighten up the line in the middle
        let y = image.resolution.1 / 2;
        let channel_index = 2; // [r,g,b,a] [2]: blue channel

        for x in 0..image.resolution.0 {
            let sample = image.data.lines[y][x][channel_index].to_f32();
            let new_sample = sample * 3.0;

            image.data.lines[y][x][channel_index] = f16::from_f32(new_sample);
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