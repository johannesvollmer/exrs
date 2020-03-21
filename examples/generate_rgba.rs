
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image::rgba::{SampleIndex};
use exr::meta::attributes::SampleType;

/// Write an RGBA exr file, generating the pixel values on the fly.
fn main() {

    /// Generates pixel values on request
    struct Generator {
        // resize the generated image individually per channel
        scale_per_channel: [Vec2<f32>; 3]
    };

    impl rgba::GetPixels for Generator {

        // generate some arbitrary rgb colors, with varying size per channel
        fn get_sample_f32(image: &rgba::Image<Self>, index: SampleIndex) -> f32 {
            let scale = image.data.scale_per_channel[index.channel];
            let value = (index.position.0 as f32 / scale.0).sin() * 0.5 + 0.5;
            value.powf((index.position.1 as f32 / scale.1).sin() * 0.5 + 0.5)
        }
    }

    rgba::Image
        // create the image with the generator as content
        ::without_alpha(
            Vec2(2*2048, 2*2048),
            rgba::Channel::linear(SampleType::F16), // the generated f32 is converted to an f16 while writing the file
            Generator { scale_per_channel: [ Vec2(21.1, 14.5), Vec2(23.1, 22.7), Vec2(11.1, 13.3), ] }
        )

        // write it to a file with all cores in parallel
        .with_encoding(rgba::Encoding::compress(Compression::RLE))
        .write_to_file("tests/images/out/generated_rgba.exr", write_options::high()).unwrap();
}