extern crate exr;
extern crate image;

use exr::prelude::*;
use exr::image::simple::*;
use exr::math::Vec2;
use std::cmp::Ordering;

/// For each layer in the exr file,
/// extract each channel as grayscale png,
/// including all multi-resolution levels.
//
// FIXME throws "access denied" sometimes, simply trying again usually works.
//
pub fn main() {
    let path = "tests/images/valid/openexr/BeachBall/multipart.0001.exr";

    let now = ::std::time::Instant::now();

    // load the exr file from disk with multi-core decompression
    let image = Image::read_from_file(path, read_options::high()).unwrap();

    // warning: highly unscientific benchmarks ahead!
    println!("\nloaded file in {:?}s", now.elapsed().as_secs_f32());
    println!("writing images...");

    for (layer_index, layer) in image.layers.iter().enumerate() {
        let layer_name = layer.attributes.name.as_ref()
            .map_or(String::from("main_layer"), attributes::Text::to_string);

        for channel in &layer.channels {
            match &channel.samples {
                Samples::F16(samples) => {
                    let data : Vec<f32> = samples.iter().map(|f16| f16.to_f32()).collect();

                    save_f32_image_as_png(&data, layer.data_size, format!(
                        "tests/images/out/{} ({}) {}_f16_{}x{}.png",
                        layer_index, layer_name, channel.name,
                        layer.data_size.width(), layer.data_size.height(),
                    ))
                },

                Samples::F32(samples) => {
                    save_f32_image_as_png(samples, layer.data_size, format!(
                        "tests/images/out/{} ({}) {}_f32_{}x{}.png",
                        layer_index, layer_name, channel.name,
                        layer.data_size.width(), layer.data_size.height(),
                    ))
                },

                Samples::U32(samples) => {
                    let data : Vec<f32> = samples.iter().map(|value| *value as f32).collect();

                    save_f32_image_as_png(&data, layer.data_size, format!(
                        "tests/images/out/{} ({}) {}_u32_{}x{}.png",
                        layer_index, layer_name, channel.name,
                        layer.data_size.width(), layer.data_size.height(),
                    ))
                },
            }
        }
    }

    /// Save raw float data to a PNG file, doing automatic brightness adjustments per channel
    fn save_f32_image_as_png(data: &[f32], size: Vec2<usize>, name: String) {
        let mut png_buffer = image::GrayImage::new(size.width() as u32, size.height() as u32);
        let mut sorted = Vec::from(data);
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Less));

        // percentile normalization
        let max = sorted[7 * sorted.len() / 8];
        let min = sorted[1 * sorted.len() / 8];

        // primitive tone mapping
        let tone = |v: f32| (v - 0.5).tanh() * 0.5 + 0.5;
        let max_toned = tone(*sorted.last().unwrap());
        let min_toned = tone(*sorted.first().unwrap());

        // for each pixel, tone map the value
        for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
            let v = data[(y as usize * size.0 + x as usize)];
            let v = (v - min) / (max - min);
            let v = tone(v);

            let v = (v - min_toned) / (max_toned - min_toned);

            // TODO does the `image` crate expect gamma corrected data?
            *pixel = image::Luma([(v.max(0.0).min(1.0) * 255.0) as u8]);
        }

        png_buffer.save(&name).unwrap();
    }

    println!("created all images");
}

