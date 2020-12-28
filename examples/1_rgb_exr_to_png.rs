
extern crate image as png;

// exr imports
extern crate exr;
use exr::prelude::*;
use exr::prelude as exrs;

fn main() {

    // read from the exr file directly into a new `png::RgbaImage` image without intermediate buffers
    let reader = exrs::read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(
        |layer_info: &exrs::RgbaChannelsInfo| -> png::RgbaImage {
                png::ImageBuffer::new(
                    layer_info.resolution.width() as u32,
                    layer_info.resolution.height() as u32
                )
            },

            // set each pixel in the png buffer from the exr file
            |png_pixels: &mut png::RgbaImage, position: exrs::Vec2<usize>, pixel: exrs::RgbaPixel| {
                png_pixels.put_pixel(
                    position.x() as u32, position.y() as u32,

                    png::Rgba([
                        tone_map(pixel.red.to_f32()),
                        tone_map(pixel.green.to_f32()),
                        tone_map(pixel.blue.to_f32()),
                        (pixel.alpha_or_1().to_f32() * 255.0) as u8,
                    ])
                );
            }
        )
        .first_valid_layer()
        .all_attributes();

    // an image that contains a single layer containing an png rgba buffer
    let image: Image<Layer<RgbaChannels<png::RgbaImage>>> = reader
        .from_file("tests/images/valid/openexr/MultiResolution/Kapaa.exr")
        .unwrap();


    /// compress any possible f32 into the range of [0,1].
    /// and then convert it to an unsigned byte.
    fn tone_map(linear: f32) -> u8 {
        // TODO does the `image` crate expect gamma corrected data?
        let clamped = (linear - 0.5).tanh() * 0.5 + 0.5;
        (clamped * 255.0) as u8
    };

    // save the png buffer to a png file
    let png_buffer = &image.layer_data.channel_data.storage;
    png_buffer.save("tests/images/out/rgb.png").unwrap();
    println!("created image rgb.png")
}