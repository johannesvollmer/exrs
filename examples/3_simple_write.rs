
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;

use rand::Rng;

// exr imports
extern crate exr;
use exr::prelude::*;


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

    let r = AnyChannel::luminance_based(
        "R".try_into().unwrap(),
        FlatSamples::F16(generate_f16_vector(size.into()))
    );

    let g = AnyChannel::luminance_based(
        "G".try_into().unwrap(),
        FlatSamples::F16(generate_f16_vector(size.into()))
    );

    let b = AnyChannel::luminance_based(
        "B".try_into().unwrap(),
        FlatSamples::F32(generate_f16_vector(size.into()).into_iter().map(f16::to_f32).collect())
    );

    let mut layer_attributes = LayerAttributes::named("test-image".try_into().unwrap());
    layer_attributes.owner = Some("It's you!".try_into().unwrap());
    layer_attributes.comments = Some("This image was procedurally generated".try_into().unwrap());

    let layer = Layer::new(
        size,
        layer_attributes,
        Encoding::default(),
        smallvec![ r, g, b ],
    );

    let image = Image::from_single_layer(layer);
    // FIXME image.remove_excess(); // crop the image by removing the transparent pixels from the border

    println!("writing image {:#?}", image);

    image.write().to_file("tests/images/out/noisy.exr").unwrap();

    println!("created file noisy.exr");
}