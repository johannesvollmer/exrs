
extern crate rand;
extern crate half;

use std::io::{BufReader};
use std::fs::File;
use exr::block::ChunksReader;

// exr imports
extern crate exr;


/// Collects the average pixel value for each channel.
/// Does not load the whole image into memory at once: only processes the image block by block.
/// On my machine, this program analyzes a 3GB file while only allocating 1.1MB.
fn main() {
    use exr::prelude::*;

    // TODO implement this example using the new API and not the raw function interface.

    // If this file does not exist yet, you can generate it by running the `7_custom_write` example once.
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
    let start_time = ::std::time::Instant::now();

    // start reading the file, extracting the meta data of the image
    let reader = exr::block::MetaDataReader::read_from_buffered(file, true).unwrap();

    // create the empty data structure that will collect the analyzed results,
    // based on the extracted meta data of the file
    let mut averages = reader.headers().iter()
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
        .collect::<Vec<_>>();

    // create a reader that only processes relevant chunks, and also prints something on progress
    let reader = reader

        // do not worry about multi-resolution levels or deep data
        .filter_chunks(true, |(_header_index, header), (_, tile)| {
            !header.deep && tile.location.is_largest_resolution_level()
        }).unwrap()

        .on_progress(|progress|{
            println!("progress: {:.2}%", progress*100.0);
        });

    // read all pixel blocks from the image, decompressing in parallel
    reader.decompress_parallel(true, |meta_data, block|{

        let header = &meta_data.headers[block.index.layer];

        // collect all pixel values from the pixel block
        for line in block.lines(&header.channels) {
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
        }

        Ok(())
    }).unwrap();


    /*let averages = exr::block::lines::read_filtered_lines_from_buffered(
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
        |_pixels, (_header_index, header), (_, tile)| {
            // do not worry about multi-resolution levels
            !header.deep() && tile.location.is_largest_resolution_level()
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

        // print file processing progress into the console
        |progress|{
            println!("progress: {:.2}%", progress*100.0);
        },

        false,
        false
    ).unwrap();*/

    println!("average values: {:#?}", averages);

    // warning: highly unscientific benchmarks ahead!
    println!("\nprocessed file in {:?}s", start_time.elapsed().as_secs_f32());
}