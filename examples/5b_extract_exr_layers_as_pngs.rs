use std::cmp::Ordering;

extern crate exr;

#[path = "../dev-support/tiny_png.rs"]
#[allow(dead_code)]
mod tiny_png;

/// For each layer in the exr file,
/// extract each channel as grayscale png,
/// including all multi-resolution levels.
// FIXME throws "access denied" sometimes, simply trying again usually works.
//
pub fn main() {
    use exr::prelude::*;

    let path = "layers.exr";
    let now = ::std::time::Instant::now();

    // load the exr file from disk with multi-core decompression
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file(path)
        .expect("run example `5a_write_multiple_layers` to generate this image file");

    // warning: highly unscientific benchmarks ahead!
    println!("\nloaded file in {:?}s", now.elapsed().as_secs_f32());
    let _ = std::fs::create_dir_all("pngs/");
    println!("writing images...");

    for (layer_index, layer) in image.layer_data.iter().enumerate() {
        let layer_name = layer
            .attributes
            .layer_name
            .as_ref()
            .map_or(String::from("main_layer"), Text::to_string);

        for channel in &layer.channel_data.list {
            let data: Vec<f32> = channel.sample_data.values_as_f32().collect();
            save_f32_image_as_png(
                &data,
                layer.size,
                format!(
                    "pngs/{} ({}) {}_{}x{}.png",
                    layer_index,
                    layer_name,
                    channel.name,
                    layer.size.width(),
                    layer.size.height(),
                ),
            )
        }
    }

    /// Save raw float data to a PNG file, doing automatic brightness
    /// adjustments per channel
    fn save_f32_image_as_png(data: &[f32], size: Vec2<usize>, name: String) {
        let (width, height) = (size.width() as u32, size.height() as u32);
        let mut png_buffer = vec![0u8; width as usize * height as usize];
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
        for (i, pixel) in png_buffer.iter_mut().enumerate() {
            let v = data[i];
            let v = (v - min) / (max - min);
            let v = tone(v);

            let v = (v - min_toned) / (max_toned - min_toned);

            *pixel = (v.max(0.0).min(1.0) * 255.0) as u8;
        }

        tiny_png::write_gray8(&name, width, height, &png_buffer).unwrap();
    }

    println!("extracted all layers to folder `./pngs/*.png`");
}
