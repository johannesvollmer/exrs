extern crate exr;
use exr::prelude::*;

fn main() {
    let generator_image = Image::with_single_layer(
        (2048, 2048),
        RgbaChannels::new(
            // TODO reduce boilerplate!
  RgbaSampleTypes::RGB_F16, // convert values to f16, no alpha

            // generate some color for each pixel position
            &|position: Vec2<usize>| {
                RgbaPixel::rgb(
                    position.x() as f32 / 2048.0, // red
                    position.y() as f32 / 2048.0, // green
                    1.0 - (position.y() as f32 / 2048.0), // blue
                )
            }
        )
    );

    generator_image.write().to_file("tests/images/out/minimal_rgba.exr").unwrap();
    println!("created file minimal_rgba.exr");
}