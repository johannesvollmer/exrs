extern crate exr;

#[path = "../dev-support/tiny_png.rs"]
#[allow(dead_code)]
mod tiny_png;

/// A flat RGBA8 pixel buffer, standing in for `image::RgbaBuffer`.
struct RgbaBuffer {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 4]>,
}

impl RgbaBuffer {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![[0, 0, 0, 0]; width as usize * height as usize],
        }
    }

    fn put_pixel(&mut self, x: u32, y: u32, pixel: [u8; 4]) {
        self.pixels[y as usize * self.width as usize + x as usize] = pixel;
    }
}

/// Converts one rgba exr with one layer to one png, or fail.
fn main() {
    use exr::{prelude as exrs, prelude::*};

    // read from the exr file directly into a new `RgbaBuffer` image without
    // intermediate buffers
    let reader = exrs::read()
        .no_deep_data()
        .largest_resolution_level()
        .rgba_channels(
            |resolution, _channels: &RgbaChannels| -> RgbaBuffer {
                RgbaBuffer::new(resolution.width() as u32, resolution.height() as u32)
            },
            // set each pixel in the png buffer from the exr file
            |png_pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                // TODO implicit argument types!
                png_pixels.put_pixel(
                    position.x() as u32,
                    position.y() as u32,
                    [tone_map(r), tone_map(g), tone_map(b), (a * 255.0) as u8],
                );
            },
        )
        .first_valid_layer()
        .all_attributes();

    // an image that contains a single layer containing an rgba buffer
    let image: Image<Layer<SpecificChannels<RgbaBuffer, RgbaChannels>>> = reader
        .from_file("generated_rgba.exr")
        .expect("run the `1_write_rgba` example to generate the required file");

    /// compress any possible f32 into the range of [0,1].
    /// and then convert it to an unsigned byte.
    fn tone_map(linear: f32) -> u8 {
        // TODO does the `image` crate expect gamma corrected data?
        let clamped = (linear - 0.5).tanh() * 0.5 + 0.5;
        (clamped * 255.0) as u8
    }

    // save the png buffer to a png file
    let png_buffer = &image.layer_data.channel_data.pixels;
    tiny_png::write_rgba8("rgb.png", png_buffer.width, png_buffer.height, &png_buffer.pixels)
        .unwrap();
    println!("created image rgb.png")
}
