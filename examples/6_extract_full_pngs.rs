// FIXME Attention: The API in this example is not ready for usage as it is still being worked on.

extern crate image as png;
use std::cmp::Ordering;

extern crate exr;
use exr::prelude::*;


pub fn main() {
    let path = "tests/images/valid/openexr/BeachBall/multipart.0001.exr";

    let now = ::std::time::Instant::now();

    // load the exr file from disk with multi-core decompression
    let image = read()
        .no_deep_data().all_resolution_levels()
        .all_channels().all_layers()
        .read_from_file(path).unwrap();

    // warning: highly unscientific benchmarks ahead!
    println!("\nloaded file in {:?}s", now.elapsed().as_secs_f32());
    println!("writing images...");

    for (layer_index, layer) in image.layer_data.iter().enumerate() {
        let layer_name = layer.attributes.layer_name.as_ref()
            .map_or(String::from("1"), attribute::Text::to_string);

        for channel in &layer.channel_data {
            for (level, level_size) in layer.levels_with_resolution(&channel.sample_data) {
                let data : Vec<f32> = level.values_as_f32().collect();

                save_f32_image_as_png(&data, level_size, format!(
                    "tests/images/out/{} ({}) {}.{}x{}.png",
                    layer_index, layer_name, channel.name,
                    level_size.width(), level_size.height(),
                ))
            }
/*
            match &channel.sample_data {
                FlatSamples::F16(levels) => {
                    let levels = levels.as_flat_samples()
                        .expect("deep data to png not supported");

                    for sample_block in levels.as_slice() {
                        let data : Vec<f32> = sample_block.samples.iter().map(|f16| f16.to_f32()).collect();

                        save_f32_image_as_png(&data, sample_block.resolution, format!(
                            "tests/images/out/{} ({}) {}_f16_{}x{}.png",
                            layer_index, layer_name, channel.name,
                            sample_block.resolution.width(), sample_block.resolution.height(),
                        ))
                    }
                },

                FlatSamples::F32(levels) => {
                    let levels = levels.as_flat_samples()
                        .expect("deep data to png not supported");

                    for sample_block in levels.as_slice() {
                        save_f32_image_as_png(&sample_block.samples, sample_block.resolution, format!(
                            "tests/images/out/{} ({}) {}_f32_{}x{}.png",
                            layer_index, layer_name, channel.name,
                            sample_block.resolution.width(), sample_block.resolution.height(),
                        ))
                    }
                },

                FlatSamples::U32(levels) => {
                    let levels = levels.as_flat_samples()
                        .expect("deep data to png not supported");

                    for sample_block in levels.as_slice() {
                        let data : Vec<f32> = sample_block.samples.iter().map(|value| *value as f32).collect();

                        save_f32_image_as_png(&data, sample_block.resolution, format!(
                            "tests/images/out/{} ({}) {}_u32_{}x{}.png",
                            layer_index, layer_name, channel.name,
                            sample_block.resolution.width(), sample_block.resolution.height(),
                        ))
                    }
                },
            }*/
        }
    }

    /// Save raw float data to a PNG file, doing automatic brightness adjustments per channel
    fn save_f32_image_as_png(data: &[f32], size: Vec2<usize>, name: String) {
        let mut png_buffer = png::GrayImage::new(size.width() as u32, size.height() as u32);
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
            *pixel = png::Luma([(v.max(0.0).min(1.0) * 255.0) as u8]);
        }

        png_buffer.save(&name).unwrap();
    }

    println!("created all images");
}

