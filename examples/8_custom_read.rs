
extern crate rand;
extern crate half;

use std::io::{BufReader};
use std::fs::File;

// exr imports
extern crate exr;
use exr::prelude::*;


/// Collects the average pixel value for each channel.
/// Does not load the whole image into memory at once: only processes the image block by block.
/// On my machine, this program analyzes a 3GB file while only allocating 1.1MB.
fn main() {

    // If this file does not exist yet, you can generate it by running the `5_custom_write` example once.
    let file = BufReader::new(File::open("tests/images/out/3GB.exr").unwrap());

    /// Collect averages for each layer
    #[derive(Debug)]
    struct Layer {
        name: Option<Text>,
        data_window: IntegerBounds,

        /// Collect one average float per channel
        channels: Vec<Channel>,
    }

    /// A single channel
    #[derive(Debug)]
    struct Channel {
        name: Text,
        sample_type: SampleType, // f32, u32, or f16
        average: f32,
    }

    // used later for printing the progress occasionally
    // let mut count_to_1000_and_then_print = 0;
    let start_time = ::std::time::Instant::now();


    let averages = exr::block::lines::read_filtered_lines_from_buffered(
        file,

        // create an instance of our resulting image struct from the loaded file meta data
        // that will be filled with information later
        |headers| -> exr::error::Result<Vec<Layer>> { Ok(
            headers.iter()
                // create a layer for each header in the file
                .map(|header| Layer {
                    name: header.own_attributes.layer_name.clone(),
                    data_window: header.data_window(),

                    // create a averaging channel for each channel in the file
                    channels: header.channels.list.iter()
                        .map(|channel| Channel {
                            name: channel.name.clone(),
                            sample_type: channel.sample_type,
                            average: 0.0
                        })
                        .collect()
                })
                .collect()
        ) },

        // specify what parts of the file should be loaded (skips mip maps)
        |_pixels, _header, (_, tile)| {
            // do not worry about multi-resolution levels
            tile.location.is_largest_resolution_level()
        },

        // fill the layers with actual average information
        // `line` contains a few samples from one channel of the image,
        // we will iterate through all samples of it
        |averages, _meta, line| {
            let layer = &mut averages[line.location.layer];
            let channel = &mut layer.channels[line.location.channel];
            let channel_sample_count = layer.data_window.size.area() as f32;

            // now sum the average based on the values in this line section of pixels
            match channel.sample_type {
                SampleType::F16 => for value in line.read_samples::<f16>() {
                    channel.average += value?.to_f32() / channel_sample_count;
                },

                SampleType::F32 => for value in line.read_samples::<f32>() {
                    channel.average += value? / channel_sample_count;
                },

                SampleType::U32 => for value in line.read_samples::<f32>() {
                    channel.average += (value? as f32) / channel_sample_count;
                },
            }

            Ok(())
        },

        // print file processing progress into the console, occasionally (important for large files)
        false,
        false

        /*ReadOptions { TODO FIXME progress callback
            parallel_decompression: false,
            max_pixel_bytes: None,
            pedantic: false,
            on_progress: |progress| {
                count_to_1000_and_then_print += 1;
                if count_to_1000_and_then_print == 1000 {
                    count_to_1000_and_then_print = 0;

                    println!("progress: {}%", (progress * 100.0) as usize);
                }

                Ok(())
            },
        },*/

    ).unwrap();

    println!("average values: {:#?}", averages);

    // warning: highly unscientific benchmarks ahead!
    println!("\nprocessed file in {:?}s", start_time.elapsed().as_secs_f32());
}