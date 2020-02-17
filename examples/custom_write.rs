
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;

use std::convert::TryInto;

// exr imports
extern crate exr;
use exr::prelude::*;
use std::io::{BufWriter};
use std::fs::File;
use exr::meta::attributes::{Channel, PixelType, LineOrder, TileDescription, LevelMode};
use exr::meta::{Blocks, MetaData};
use exr::math::RoundingMode;

/// Generate a striped image on the fly and directly write that to a file without allocating the whole image at once.
/// On my machine, this program produces a 3GB file while only ever allocating 4MB memory (takes a while though).
fn main() {

    // pre-compute a list of random values
    let random_values: Vec<f32> = (0..64)
        .map(|_| rand::random::<f32>())
        .collect();

    // resulting resolution (268 megapixels for 3GB files)
    let size = (2048*8, 2048*8);

    // specify output path, and buffer it for better performance
    let file = BufWriter::new(File::create("./testout/3GB.exr").unwrap());

    // define meta data header that will be written
    let header = exr::meta::Header::new(
        "test-image".try_into().unwrap(),
        size,
        smallvec![
            Channel::new("B".try_into().unwrap(), PixelType::F32, true),
            Channel::new("G".try_into().unwrap(), PixelType::F32, true),
            Channel::new("R".try_into().unwrap(), PixelType::F32, true),
        ],
    );

    // define encoding that will be written
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

    // print progress only every 100th time
    let mut count_to_1000_and_then_print = 0;
    let start_time = ::std::time::Instant::now();

    // finally write the image
    exr::image::write_all_lines_to_buffered(
        file,
        meta,

        // fill the image file contents with one of the precomputed random values,
        // picking a different one per channel
        |_meta, line_mut|{
            let chan = line_mut.location.channel;
            line_mut.write_samples(|sample_index| random_values[(sample_index + chan) % random_values.len()])
        },

        // print progress occasionally
        WriteOptions {
            parallel_compression: false,
            pedantic: true,

            on_progress: |progress, bytes| {
                count_to_1000_and_then_print += 1;
                if count_to_1000_and_then_print == 1000 {
                    count_to_1000_and_then_print = 0;

                    let mega_bytes = bytes / 1000000;
                    let percent = (progress * 100.0) as usize;
                    println!("progress: {}%, wrote {} megabytes", percent, mega_bytes);
                }

                Ok(())
            },
        }
    ).unwrap();

    // warning: highly unscientific benchmarks ahead!
    let duration = start_time.elapsed();
    let millis = duration.as_secs() * 1000 + duration.subsec_millis() as u64;
    println!("\nWrote exr file in {:?}s", millis as f32 * 0.001);
}