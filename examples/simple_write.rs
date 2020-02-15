
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;

use std::convert::TryInto;
use rand::Rng;

// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image::simple::*;


#[test]
fn write_noisy_hdr() {
    fn generate_f16_vector(size: Vec2<usize>) -> Vec<f16> {
        let mut values = vec![ f16::from_f32(0.5); size.area() ];

        for _ in 0..(1024*1024/3)/4 {
            let index = rand::thread_rng().gen_range(0, values.len());
            let value = 1.0 / rand::random::<f32>() - 1.0;
            let value = if !value.is_normal() || value > 1000.0 { 1000.0 } else { value };
            values[index] = f16::from_f32(value);
        }

        values
    }

    let size = Vec2(1024, 512);

    let r = Channel::new_linear(
        "R".try_into().unwrap(),
        Samples::F16(generate_f16_vector(size))
    );

    let g = Channel::new_linear(
        "G".try_into().unwrap(),
        Samples::F16(generate_f16_vector(size))
    );

    let b = Channel::new_linear(
        "B".try_into().unwrap(),
        Samples::F32(generate_f16_vector(size).into_iter().map(f16::to_f32).collect())
    );

    let part = simple::Layer::new(
        "test-image".try_into().unwrap(),
        IntRect::from_dimensions(size),
        smallvec![ r, g, b ],
    );

    let part = part.with_compression(Compression::RLE)
        .with_block_format(None, attributes::LineOrder::Increasing); // apparently, some software only supports increasing line order

    let image = Image::new_from_single_part(part);

    println!("writing image {:#?}", image);
    image.write_to_file("./testout/noisy.exr", WriteOptions::high()).unwrap(); // FIXME parallel produces invalid files

    assert!(Image::read_from_file("./testout/noisy.exr", ReadOptions::high()).is_ok())
}