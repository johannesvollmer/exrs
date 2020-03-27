
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::meta::attributes::SampleType;

/// Write an RGBA exr file, generating the pixel values on the fly.
fn main() {

    // this function can generate a color for each pixel
    let generate_pixels = |_image: &rgba::Image, position: Vec2<usize>| {

        // generate some arbitrary rgb colors, with varying size per channel
        fn get_sample_f32(position: Vec2<usize>, channel: usize) -> f32 {
            let scale_per_channel = [Vec2(21.1, 14.5), Vec2(23.1, 22.7), Vec2(11.1, 13.3)];
            let scale = scale_per_channel[channel];

            let value = (position.0 as f32 / scale.0).sin() * 0.5 + 0.5;
            value.powf((position.1 as f32 / scale.1).sin() * 0.5 + 0.5)
        }

        rgba::Pixel::rgb(
            get_sample_f32(position, 0),
            get_sample_f32(position, 1),
            get_sample_f32(position, 2),
        )
    };

    let image_info = rgba::Image::rgb(
        Vec2(2*2048, 2*2048),

        // the generated f32 is converted to an f16 while writing the file
        rgba::Channel::linear(SampleType::F16),
    );

    // write it to a file with all cores in parallel
    image_info
        .with_encoding(rgba::Encoding::compress(Compression::RLE))
        .write_to_file(
            "tests/images/out/generated_rgba.exr",
            write_options::high(), &generate_pixels
        ).unwrap();
}