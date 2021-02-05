extern crate exr;

use exr::prelude::f16;

/// `exr` offers a few very simple functions for the most basic use cases.
/// `write_rgb_f32_file` is a simple function which writes a simple exr file.
/// To write the image, you need to specify how to retrieve a single pixel from it.
/// The closure may capture variables or generate data on the fly.
fn main() {
    // write a file, with 32-bit float precision per color channel, and 16-bit float alpha
    exr::prelude::write_rgba_file(
        "tests/images/out/minimal_rgb.exr",
        2048, 2048, // write an image with 2048x2048 pixels
        |x,y| ( // generate (or lookup in your own image) an f32 rgb color for each of the 2048x2048 pixels
            x as f32 / 2048.0, // red
            y as f32 / 2048.0, // green
            1.0 - (y as f32 / 2048.0), // blue
            f16::from_f32(0.8) // 16-bit alpha
        )
    ).unwrap();

    println!("created file minimal_rgb.exr");
}