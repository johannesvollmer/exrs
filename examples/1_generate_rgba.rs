
// exr imports
extern crate exr;

/// Write an rgba exr file, generating the pixel values on the fly.
/// This streams the generated pixel directly to the file,
/// never allocating the actual total pixel memory of the image.
fn main() {
    use exr::prelude::*;

    // this function can generate a color for any pixel
    let generate_pixels = |position: Vec2<usize>| -> (f32,f32,f32,f16) {

        // generate some arbitrary rgb colors, with varying size per channel
        fn get_sample_f32(position: Vec2<usize>, channel: usize) -> f32 {
            let scale_per_channel = [Vec2(21.1, 14.5), Vec2(23.1, 22.7), Vec2(11.1, 13.3)];
            let scale = scale_per_channel[channel];

            let value = (position.x() as f32 / scale.x()).sin() * 0.5 + 0.5;
            value.powf((position.y() as f32 / scale.y()).sin() * 0.5 + 0.5)
        }

        // return an rgba quadruple
        // use 32 bit color, but alpha with f16 precision
        (
            get_sample_f32(position, 0),
            get_sample_f32(position, 1),
            get_sample_f32(position, 2),
            f16::from_f32(0.8)
        )
    };

    let mut attributes = LayerAttributes::named("generated rgba");
    attributes.comments = Some(Text::from("This image was generated as part of an example"));
    attributes.owner = Some(Text::from("The holy lambda function"));

    let layer = Layer::new(
        (2*2048, 2*2048),
        attributes,
        Encoding::SMALL_FAST_LOSSY, // use fast but lossy compression

        SpecificChannels::rgba(generate_pixels)
    );

    // crop away black and transparent pixels from the border
    let layer = layer
        .crop_where_eq((0.0, 0.0, 0.0, f16::ZERO))
        .or_crop_to_1x1_if_empty();

    let image = Image::from_single_layer(layer);

    // write it to a file with all cores in parallel
    image.write().to_file("tests/images/out/generated_rgba.exr").unwrap();
    println!("created file generated_rgba.exr");
}