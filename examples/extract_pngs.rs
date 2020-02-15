extern crate exr;
extern crate image;

use exr::prelude::*;
use exr::image::full::*;
use std::{fs};
use exr::math::Vec2;
use std::cmp::Ordering;

/// For each image part in the exr file,
/// extract each channel as grayscale png,
/// including all multiresolution levels.
//
// FIXME throws "acces denied" sometimes, simply trying again usually works.
//
#[test]
pub fn convert_to_png() {
    let now = ::std::time::Instant::now();

    let path =
        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"


//        "D:/Pictures/openexr/Tiles/Ocean.exr"
//        "D:/Pictures/openexr/MultiResolution/Kapaa.exr"
//        "D:/Pictures/openexr/MultiView/Impact.exr"
//        "D:/Pictures/openexr/MultiResolution/KernerEnvCube.exr"
//        "D:/Pictures/openexr/MultiResolution/Bonita.exr"


//            "D:/Pictures/openexr/MultiResolution/Bonita.exr"

//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//            "D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    let image = Image::read_from_file(path, ReadOptions::high()).unwrap();

    // warning: highly unscientific benchmarks ahead!
    let elapsed = now.elapsed();
    let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;
    println!("\ndecoded exr file in {:?}s", millis as f32 * 0.001);


    /// convert raw float data to a png, doing automatic brightness adjustments
    fn save_f32_image_as_png(data: &[f32], size: Vec2<usize>, name: String) {
        let mut png_buffer = image::GrayImage::new(size.0 as u32, size.1 as u32);
        let mut sorted = Vec::from(data);
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Less));

        // sixth percentile normalization
        let max = sorted[7 * sorted.len() / 8];
        let min = sorted[1 * sorted.len() / 8];

        let tone = |v: f32| (v - 0.5).tanh() * 0.5 + 0.5;
        let max_toned = tone(*sorted.last().unwrap());
        let min_toned = tone(*sorted.first().unwrap());

        for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
            let v = data[(y as usize * size.0 + x as usize)];
            let v = (v - min) / (max - min);
            let v = tone(v);

            let v = (v - min_toned) / (max_toned - min_toned);
            *pixel = image::Luma([(v.max(0.0).min(1.0) * 255.0) as u8]);
        }

        png_buffer.save(&name).unwrap();
    }

    fs::remove_dir_all("testout").unwrap_or_default();
    fs::create_dir("testout").unwrap();

    for (part_index, part) in image.parts.iter().enumerate() {
        for channel in &part.channels {
            match &channel.content {
                ChannelData::F16(levels) => {
                    let levels = levels.as_flat_samples()
                        .expect("deep data to png not supported");

                    for sample_block in levels.as_slice() {
                        let data : Vec<f32> = sample_block.samples.iter().map(|f16| f16.to_f32()).collect();

                        save_f32_image_as_png(&data, sample_block.resolution, format!(
                            "testout/{} ({}) {}_f16_{}x{}.png",
                            part_index,
                            part.name.as_ref().map(attributes::Text::to_string).unwrap_or(String::from("1")),
                            channel.name,
                            sample_block.resolution.0,
                            sample_block.resolution.1,
                        ))
                    }
                },

                ChannelData::F32(levels) => {
                    let levels = levels.as_flat_samples().unwrap();
                    for sample_block in levels.as_slice() {
                        save_f32_image_as_png(&sample_block.samples, sample_block.resolution, format!(
                            "testout/{} ({}) {}_f16_{}x{}.png",
                            part_index,
                            part.name.as_ref().map(attributes::Text::to_string).unwrap_or(String::from("1")),
                            channel.name,
                            sample_block.resolution.0,
                            sample_block.resolution.1,
                        ))
                    }
                },

                ChannelData::U32(levels) => {
                    let levels = levels.as_flat_samples().unwrap();
                    for sample_block in levels.as_slice() {
                        let data : Vec<f32> = sample_block.samples.iter().map(|value| *value as f32).collect();

                        save_f32_image_as_png(&data, sample_block.resolution, format!(
                            "testout/{} ({}) {}_f16_{}x{}.png",
                            part_index,
                            part.name.as_ref().map(attributes::Text::to_string).unwrap_or(String::from("1")),
                            channel.name,
                            sample_block.resolution.0,
                            sample_block.resolution.1,
                        ))
                    }
                },
            }
        }
    }
}

