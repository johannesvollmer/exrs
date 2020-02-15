
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;

use std::convert::TryInto;
use rand::Rng;

// exr imports
extern crate exr;
use exr::prelude::*;
use std::io::{BufWriter};
use std::fs::File;
use exr::meta::attributes::{Channel, PixelType, LineOrder};
use exr::io::Data;
use exr::meta::Blocks;


#[test]
fn write_noisy_hdr() {
    fn generate_f16s(length: usize) -> impl Iterator<Item = f16> {
        let mut values = vec![ f16::from_f32(0.5); length ];

        for _ in 0..(length / 4) {
            let index = rand::thread_rng().gen_range(0, values.len());
            let value = 1.0 / rand::random::<f32>() - 1.0;
            let value = if !value.is_normal() || value > 1000.0 { 1000.0 } else { value };
            values[index] = f16::from_f32(value);
        }

        values.into_iter()
    }

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

    exr::image::write_all_lines_to_buffered(file, true, meta, |line, write|{
        for value in generate_f16s(line.width) {
            f16::write(value, write).expect("collect pixel error");
        }
    }).unwrap();

    assert!(exr::image::full::Image::read_from_file("./testout/noisy.exr", exr::image::full::ReadOptions::high()).is_ok())
}