
extern crate image;

// exr imports
extern crate exr;
use exr::prelude::*;

fn main() {

    /// This buffer can be written to a file by the PNG crate
    /// and can be created from a file by the EXR crate
    struct PngPixels(image::RgbaImage);

    // tell exrs how to load an exr image into our custom png buffer
    impl exr::image::rgba::SetPixels for PngPixels {

        /// set a single pixel with red, green, blue, and optionally and alpha value.
        /// (this method is also called for f16 or u32 values, if you do not implement the other methods in this trait)
        fn set_pixel(&mut self, position: Vec2<usize>, pixel: rgba::Pixel) {

            /// compress any possible f32 into the range of [0,1].
            /// and then convert it to an unsigned byte.
            fn tone_map(raw: f32) -> u8 {
                let clamped = (raw - 0.5).tanh() * 0.5 + 0.5;
                (clamped * 255.0) as u8
            };

            self.0.put_pixel(
                position.x() as u32, position.y() as u32,

                image::Rgba([
                    tone_map(pixel.red.to_f32()),
                    tone_map(pixel.green.to_f32()),
                    tone_map(pixel.blue.to_f32()),
                    (pixel.alpha_or_default().to_f32() * 255.0) as u8,
                ])
            );
        }
    }

    // read the image from a file and keep only the png buffer
    let (_info, PngPixels(png_buffer)) = rgba::ImageInfo::read_pixels_from_file(
        "tests/images/valid/openexr/MultiResolution/Kapaa.exr",
        read_options::high(),

        // how to create an empty png buffer from exr image meta data (used for loading the exr image)
        |image: &exr::image::rgba::ImageInfo| {
            PngPixels(image::ImageBuffer::new(
                image.resolution.width() as u32,
                image.resolution.height() as u32
            ))
        }
    ).unwrap();

    // save the png buffer to a png file
    png_buffer.save("tests/images/out/rgb.png").unwrap();
    println!("Saved PNG image `rgb.png`")
}