
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::meta::attribute::SampleType;
use std::convert::TryInto;

/// Write an RGBA exr file, generating the pixel values on the fly.
/// This streams the generated pixel directly to the file,
/// never allocating the actual total pixel memory of the image.
fn main() {

    // this function can generate a color for any pixel
    let generate_pixels = |position: Vec2<usize>| {

        // generate some arbitrary rgb colors, with varying size per channel
        fn get_sample_f32(position: Vec2<usize>, channel: usize) -> f32 {
            let scale_per_channel = [Vec2(21.1, 14.5), Vec2(23.1, 22.7), Vec2(11.1, 13.3)];
            let scale = scale_per_channel[channel];

            let value = (position.x() as f32 / scale.x()).sin() * 0.5 + 0.5;
            value.powf((position.y() as f32 / scale.y()).sin() * 0.5 + 0.5)
        }

        rgba::Pixel::rgb(
            get_sample_f32(position, 0),
            get_sample_f32(position, 1),
            get_sample_f32(position, 2),
        )
    };


    let mut image_info = rgba::ImageInfo::rgb(
        (2*2048, 2*2048),

        // all generated f32 values are converted to an f16 while writing the file
        SampleType::F16,
    );

    image_info.layer_attributes.owner = Some("Unknown Owner".try_into().unwrap());
    image_info.layer_attributes.comments = Some(
        "This image was generated as part of an example".try_into().unwrap()
    );

    // write it to a file with all cores in parallel
    image_info
        .with_encoding(rgba::Encoding::for_compression(Compression::RLE))
        .write_pixels_to_file(
            "tests/images/out/generated_rgba.exr",
            write_options::high(), // this will actually generate the pixels in parallel on all cores
            &generate_pixels
        ).unwrap();

    println!("created file generated_rgba.exr");
}