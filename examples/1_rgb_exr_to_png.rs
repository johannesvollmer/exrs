
extern crate image;

// exr imports
extern crate exr;
use exr::prelude::*;

fn main() {

    // read the image from a file and keep only the png buffer
    let (_info, png_buffer) = rgba::ImageInfo::read_pixels_from_file(
        "tests/images/valid/openexr/MultiResolution/Kapaa.exr",
        read_options::high(),

        // how to create an empty png buffer from exr image meta data (used for loading the exr image)
        |info: &exr::image::rgba::ImageInfo| {
            image::ImageBuffer::new(
                info.resolution.width() as u32,
                info.resolution.height() as u32
            )
        },

        // set each pixel in the png buffer from the exr file
        |png_pixels: &mut image::RgbaImage, position: Vec2<usize>, pixel: rgba::Pixel| {
            png_pixels.put_pixel(
                position.x() as u32, position.y() as u32,

                image::Rgba([
                    tone_map(pixel.red.to_f32()),
                    tone_map(pixel.green.to_f32()),
                    tone_map(pixel.blue.to_f32()),
                    (pixel.alpha_or_default().to_f32() * 255.0) as u8,
                ])
            );
        },
    ).unwrap();


    /// compress any possible f32 into the range of [0,1].
    /// and then convert it to an unsigned byte.
    fn tone_map(linear: f32) -> u8 {
        let srgb = linear.powf(1.0/2.2); // the image crate probably expects srgb data, not linear data? TODO check this
        let clamped = (srgb - 0.5).tanh() * 0.5 + 0.5;
        (clamped * 255.0) as u8
    };

    // save the png buffer to a png file
    png_buffer.save("tests/images/out/rgb.png").unwrap();
    println!("Saved PNG image `rgb.png`")
}