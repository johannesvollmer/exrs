
// exr imports
extern crate exr;
use exr::prelude::*;

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

        // return an rgba quadruple
        (
            get_sample_f32(position, 0),
            get_sample_f32(position, 1),
            get_sample_f32(position, 2),
            0.8
        )
    };

    let mut attributes = LayerAttributes::named("generated rgba");
    attributes.comments = Some(Text::from("This image was generated as part of an example"));
    attributes.owner = Some(Text::from("The holy lambda function"));

    let layer = Layer::new(
        (2*2048, 2*2048),
        attributes,
        Encoding::SMALL_FAST_LOSSY,

        RgbaChannels::new(
            // all generated f32 values are converted to an f16 while writing the file
            RgbaSampleTypes::RGBA_F16,
            generate_pixels
        )
    );

    // crop away invisible pixels
    let layer = layer
        .crop_where(|(_r, _g, _b, alpha)| alpha == 0.0)
        .or_crop_to_1x1_if_empty();

    let image = Image::from_single_layer(layer);

    // write it to a file with all cores in parallel
    image.write().to_file("tests/images/out/generated_rgba.exr").unwrap();
    println!("created file generated_rgba.exr");
}