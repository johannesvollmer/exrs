
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


/// Generate a noisy image and write it to a file.
fn main() {
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

    let size = (1024, 512);

    let r = Channel::color_data(
        "R".try_into().unwrap(),
        Samples::F16(generate_f16_vector(size.into()))
    );

    let g = Channel::color_data(
        "G".try_into().unwrap(),
        Samples::F16(generate_f16_vector(size.into()))
    );

    let b = Channel::color_data(
        "B".try_into().unwrap(),
        Samples::F32(generate_f16_vector(size.into()).into_iter().map(f16::to_f32).collect())
    );

    let layer = Layer::new(
        "test-image".try_into().unwrap(),
        size,
        smallvec![ r, g, b ],
    );

    let mut layer = layer.with_compression(Compression::RLE)
        .with_block_format(None, attributes::LineOrder::Increasing);

    layer.attributes.owner = Some("It's you!".try_into().unwrap());
    layer.attributes.comments = Some("This image was procedurally generated".try_into().unwrap());

    let image = Image::new_from_single_layer(layer);

    println!("writing image {:#?}", image);
    image.write_to_file("tests/images/out/noisy.exr", write_options::high()).unwrap();
}