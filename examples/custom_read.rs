
extern crate rand;
extern crate half;

use std::io::BufReader;
use std::fs::File;

// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image;
use exr::meta::attributes::PixelType;


/// Collects the average pixel value for each channel.
/// Does not load the whole image into memory at once: only processes the image block by block.
/// On my machine, this program analyzes a 3GB file while only allocating 5MB.
#[test]
fn analyze_image() {
    let file = BufReader::new(File::open("./testout/noisy.exr").unwrap());

    #[derive(Debug)]
    struct Layer {
        name: Option<Text>,
        data_window: IntRect,
        channels: Vec<Channel>,
    }

    #[derive(Debug)]
    struct Channel {
        name: Text,
        pixel_type: PixelType,
        average: f32,
    }

    let averages = image::read_filtered_lines_from_buffered(
        file, true,
        |_header, tile| {
            // do not worry about multiresolution levels
            tile.location.level_index == Vec2(0,0)
        },

        |headers| -> exr::error::Result<Vec<Layer>> { Ok(
            headers.iter()
                .map(|header| Layer {
                    name: header.name.clone(),
                    data_window: header.data_window,
                    channels: header.channels.list.iter()
                        .map(|channel| Channel {
                            name: channel.name.clone(),
                            pixel_type: channel.pixel_type,
                            average: 0.0
                        })
                        .collect()
                })
                .collect()
        ) },

        |averages, line| {
            let layer = &mut averages[line.location.layer];
            let channel = &mut layer.channels[line.location.channel];
            let channel_sample_count = layer.data_window.size.area() as f32;

            match channel.pixel_type {
                PixelType::F16 => {
                    for value in line.read_samples::<f16>() {
                        channel.average += value?.to_f32() / channel_sample_count;
                    }
                },

                PixelType::F32 => {
                    for value in line.read_samples::<f32>() {
                        channel.average += value? / channel_sample_count;
                    }
                },

                PixelType::U32 => {
                    for value in line.read_samples::<f32>() {
                        channel.average += (value? as f32) / channel_sample_count;
                    }
                },
            }

            Ok(())
        },

        ()
    ).unwrap();

    println!("average values: {:#?}", averages);
}