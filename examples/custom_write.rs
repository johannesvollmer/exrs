
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
use exr::meta::attributes::{Channel, PixelType, LineOrder};
use exr::meta::Blocks;

/// Generate a noisy image on the fly and directly write that to a file without allocating the whole image at once.
#[test]
fn write_generated_noisy_hdr() {

    /// Just a random high dynamic range f16
    fn generate_f16 () -> f16 {
        let value = 1.0 / rand::random::<f32>() - 1.0;
        let value = if !value.is_normal() || value > 100000.0 { 100000.0 } else { value };
        f16::from_f32(value)
    };

    let size = Vec2(1024, 512);
    let file = BufWriter::new(File::create("./testout/noisy.exr").unwrap());

    let header = exr::meta::Header::new(
        "test-image".try_into().unwrap(),
        IntRect::from_dimensions(size),
        smallvec![
            Channel::new("B".try_into().unwrap(), PixelType::F16, true),
            Channel::new("G".try_into().unwrap(), PixelType::F16, true),
            Channel::new("R".try_into().unwrap(), PixelType::F16, true),
        ],
    );

    let header = header.with_encoding(
        Compression::RLE,
        Blocks::ScanLines,
        LineOrder::Increasing
    );

    let meta = MetaData::new(smallvec![ header ]);

    exr::image::write_all_lines_to_buffered(
        file, true,
        true,meta,
        |line_mut|{
            line_mut.set_samples(|_sample_index| generate_f16()).expect("pixel bytes write error") // TODO without expect
        }
    ).unwrap();

    assert!(exr::image::full::Image::read_from_file("./testout/noisy.exr", exr::image::full::ReadOptions::high()).is_ok())
}