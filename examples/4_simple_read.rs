
// exr imports
extern crate exr;
use exr::prelude::simple_image::*;

/// Read an image and print information about the image into the console.
fn main() {

    let image = Image::read_from_file(
        "tests/images/valid/openexr/Beachball/multipart.0004.exr",
        read_options::high() // use multi-core decompression
    ).unwrap();

    println!("image was read: {:#?}", image);

    // output the average value for each channel of each layer
    for layer in &image.layers {

        for channel in &layer.channels {
            let average = match &channel.samples {
                Samples::F16(f16_vec) => f16_vec.iter().map(|f| f.to_f32()).sum::<f32>() / f16_vec.len() as f32,
                Samples::F32(f32_vec) => f32_vec.iter().sum::<f32>() / f32_vec.len() as f32,
                Samples::U32(u32_vec) => u32_vec.iter().sum::<u32>() as f32 / u32_vec.len() as f32,
            };

            if let Some(layer_name) = &layer.attributes.layer_name {
                println!("Channel `{}` of Layer `{}` has an average value of {}", channel.name, layer_name, average);
            }
            else {
                println!("Channel `{}` has an average value of {}", channel.name, average);
            }
        }
    }
}