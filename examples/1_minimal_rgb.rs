extern crate exr;
use exr::prelude::rgba_image::*;

fn main() {
    // generate an image with 2048*2048 pixels, converting all numbers to f16
    ImageInfo::rgb((2048, 2048), SampleType::F16).write_pixels_to_file(
        "tests/images/out/minimal_rgba.exr",
        write_options::high(), // higher speed, but higher memory usage

        // generate a color for each pixel position
        &|position: Vec2<usize>| {
            Pixel::rgb(
                position.x() as f32 / 2048.0, // red
                position.y() as f32 / 2048.0, // green
                1.0 - (position.y() as f32 / 2048.0), // blue
            )
        }
    ).unwrap();
}