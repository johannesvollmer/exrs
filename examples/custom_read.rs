
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;

use std::convert::TryInto;
use rand::Rng;

// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image;
use std::io::BufReader;
use std::fs::File;


#[test]
fn analyze_image() {
    let file = BufReader::new(File::open("./testout/noisy.exr").unwrap());

    struct Part {
        name: Option<Text>,
        data_window: IntRect,
        channel_averages: Vec<(Text, f32)>,
    }

    image::read_filtered_lines_from_buffered(
        file, true,
        |header, tile| {
            !header.deep && tile.location.level_index == Vec2(0,0)
        },

        |headers| -> Averages {
            headers.iter()
                .map(|header| Part {
                    name: header.name.clone(),
                    data_window: header.data_window,
                    channel_averages: header.channels.list.iter()
                        .map(|channel| match channel.pixel_type {
                            attributes::PixelType::F16 => Average::F16(f16::ZERO),
                            attributes::PixelType::F32 => Average::F16(f16::ZERO),
                            attributes::PixelType::U32 => Average::F16(f16::ZERO),
                        })
                        .collect()
                })
                .collect()
        },

        |histograms, line| {

            Ok(())
        }
    ).unwrap();



}