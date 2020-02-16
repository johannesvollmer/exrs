extern crate exr;
extern crate image;

use exr::prelude::*;
use exr::image::full::*;
use std::{fs};
use exr::math::Vec2;
use std::cmp::Ordering;
use std::io::ErrorKind;

/// For each layer in the exr file,
/// extract each channel as grayscale png,
/// including all multiresolution levels.
//
// FIXME throws "acces denied" sometimes, simply trying again usually works.
//
pub fn main() {
    let path = "D:/Pictures/openexr/BeachBall/multipart.0001.exr";

    let now = ::std::time::Instant::now();

    // load the exr file from disk with multicore decompression
    let image = Image::read_from_file(path, read_options::high()).unwrap();

    // warning: highly unscientific benchmarks ahead!
    let elapsed = now.elapsed();
    let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;
    println!("\nDecoded exr file in {:?}s", millis as f32 * 0.001);

    {   // clear output directory
        if let Err(error) = fs::remove_dir_all("testout") {
            if error.kind() != ErrorKind::NotFound {
                println!("{}", error);
            }
        }

        fs::create_dir("testout").unwrap();
    }

    println!("Writing PNG images...");

    for (layer_index, layer) in image.layers.iter().enumerate() {
        let layer_name = layer.attributes.name.as_ref()
            .map_or(String::from("1"), attributes::Text::to_string);

        for channel in &layer.channels {
            match &channel.content {
                ChannelData::F16(levels) => {
                    let levels = levels.as_flat_samples()
                        .expect("deep data to png not supported");

                    for sample_block in levels.as_slice() {
                        let data : Vec<f32> = sample_block.samples.iter().map(|f16| f16.to_f32()).collect();

                        save_f32_image_as_png(&data, sample_block.resolution, format!(
                            "testout/{} ({}) {}_f16_{}x{}.png",
                            layer_index, layer_name, channel.name,
                            sample_block.resolution.0, sample_block.resolution.1,
                        ))
                    }
                },

                ChannelData::F32(levels) => {
                    let levels = levels.as_flat_samples()
                        .expect("deep data to png not supported");

                    for sample_block in levels.as_slice() {
                        save_f32_image_as_png(&sample_block.samples, sample_block.resolution, format!(
                            "testout/{} ({}) {}_f32_{}x{}.png",
                            layer_index, layer_name, channel.name,
                            sample_block.resolution.0, sample_block.resolution.1,
                        ))
                    }
                },

                ChannelData::U32(levels) => {
                    let levels = levels.as_flat_samples()
                        .expect("deep data to png not supported");

                    for sample_block in levels.as_slice() {
                        let data : Vec<f32> = sample_block.samples.iter().map(|value| *value as f32).collect();

                        save_f32_image_as_png(&data, sample_block.resolution, format!(
                            "testout/{} ({}) {}_u32_{}x{}.png",
                            layer_index, layer_name, channel.name,
                            sample_block.resolution.0, sample_block.resolution.1,
                        ))
                    }
                },
            }
        }
    }

    /// Save raw float data to a PNG file, doing automatic brightness adjustments per channel
    fn save_f32_image_as_png(data: &[f32], size: Vec2<usize>, name: String) {
        let mut png_buffer = image::GrayImage::new(size.0 as u32, size.1 as u32);
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
            *pixel = image::Luma([(v.max(0.0).min(1.0) * 255.0) as u8]);
        }

        png_buffer.save(&name).unwrap();
    }

    println!("Saved PNG images.");
}

