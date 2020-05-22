extern crate exr;
use exr::prelude::rgba_image::*;

fn main() {
    // generate a color for each pixel position
    let generate_pixels = |position: Vec2<usize>| Pixel::rgb(
        position.x() as f32 / 2048.0, // red
        position.y() as f32 / 2048.0, // green
        1.0 - (position.y() as f32 / 2048.0), // blue
    );

    let image_info = ImageInfo::rgb(
        (2048, 2048), // pixel resolution
        SampleType::F16, // convert the generated f32 values to f16 while writing
    );

    image_info.write_pixels_to_file(
        "tests/images/out/minimal_rgba.exr",
        write_options::high(), // higher speed, but higher memory usage
        &generate_pixels // pass our pixel generator
    ).unwrap();
}