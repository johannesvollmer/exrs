
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;

use std::convert::TryInto;

// exr imports
extern crate exr;
use exr::prelude::*;
use std::io::{BufWriter, Write};
use std::fs::File;
use exr::meta::attributes::{Channel, PixelType, LineOrder, TileDescription, LevelMode};
use exr::meta::Blocks;
use exr::math::RoundingMode;

/// Generate a striped image on the fly and directly write that to a file without allocating the whole image at once.
/// On my machine, this program produces a 3GB file while only ever allocating 5MB memory (takes a while though).
#[test]
fn write_generated_stripes() {

    let random_values: Vec<f32> = (0..64)
        .map(|_| rand::random::<f32>())
        .collect();

    let size = Vec2(2048*8, 2048*8); // this file will be 3GB on disk, but not in memory. on my machine, running this program uses 5MB memory.
    let file = BufWriter::new(File::create("./testout/noisy.exr").unwrap());

    let header = exr::meta::Header::new(
        "test-image".try_into().unwrap(),
        size,
        smallvec![
            Channel::new("B".try_into().unwrap(), PixelType::F32, true),
            Channel::new("G".try_into().unwrap(), PixelType::F32, true),
            Channel::new("R".try_into().unwrap(), PixelType::F32, true),
        ],
    );

    let header = header.with_encoding(
        Compression::Uncompressed,

        Blocks::Tiles(TileDescription {
            tile_size: Vec2(64, 64),
            level_mode: LevelMode::Singular,
            rounding_mode: RoundingMode::Down
        }),

        LineOrder::Increasing
    );

    let meta = MetaData::new(smallvec![ header ]);

    let stdout = std::io::stdout();
    let mut stdout = stdout.lock(); // do not lock on every progress callback
    let mut count_to_100_and_then_print = 0;

    exr::image::write_all_lines_to_buffered(
        file, false,
        true,meta,
        |line_mut|{
            let chan = line_mut.location.channel;
            line_mut.write_samples(|sample_index| random_values[(sample_index + chan) % random_values.len()])
        },

        |progress, bytes| {
            count_to_100_and_then_print += 1;
            if count_to_100_and_then_print == 100 {
                count_to_100_and_then_print = 0;

                let mega_bytes = bytes / 1000000;
                let percent = (progress * 100.0) as usize;
                stdout.write_all(format!("progress: {}%, wrote {} megabytes\n", percent, mega_bytes).as_bytes()).unwrap();
            }

            Ok(())
        },
    ).unwrap();

    // assert!(exr::image::full::Image::read_from_file("./testout/noisy.exr", exr::image::full::read_options::high()).is_ok())
}