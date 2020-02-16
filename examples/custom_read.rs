
extern crate rand;
extern crate half;

use std::io::{BufReader, Write};
use std::fs::File;

// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image;
use exr::meta::attributes::PixelType;


/// Collects the average pixel value for each channel.
/// Does not load the whole image into memory at once: only processes the image block by block.
/// On my machine, this program analyzes a 3GB file while only allocating 1.2MB.
fn main() {
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

    let stdout = std::io::stdout();
    let mut stdout = stdout.lock(); // do not lock on every progress callback
    let mut count_to_100_and_then_print = 0;

    let averages = image::read_filtered_lines_from_buffered(
        file, true,
        |_header, tile| {
            // do not worry about multiresolution levels
            tile.location.level_index == Vec2(0,0)
        },

        |headers| -> exr::error::Result<Vec<Layer>> { Ok(
            headers.iter()
                .map(|header| Layer {
                    name: header.own_attributes.name.clone(),
                    data_window: header.data_window(),
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

        |progress| {
            count_to_100_and_then_print += 1;
            if count_to_100_and_then_print == 100 {
                count_to_100_and_then_print = 0;

                let percent = (progress * 100.0) as usize;
                stdout.write_all(format!("progress: {}%\n", percent).as_bytes()).unwrap();
            }

            Ok(())
        },
    ).unwrap();

    println!("average values: {:#?}", averages);
}