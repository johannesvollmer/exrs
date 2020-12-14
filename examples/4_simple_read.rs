
// exr imports
extern crate exr;
use exr::prelude::*;

/// Read an image and print information about the image into the console.
/// This example shows how to read an image with multiple layers and arbitrary channels.
/// This example does not include resolution levels (mipmaps or ripmaps).
fn main() {

    let image = read().no_deep_data()
        .largest_resolution_level().all_channels().all_layers()
        .from_file("tests/images/valid/openexr/Beachball/multipart.0004.exr")
        .unwrap();

    // let image = Image::read_from_file(
    //     "tests/images/valid/openexr/Beachball/multipart.0004.exr",
    //     read_options::high() // use multi-core decompression
    // ).unwrap();

    println!("image was read: {:#?}", image);

    // output the average value for each channel of each layer
    for layer in &image.layer_data {
        for channel in &layer.channel_data.list {

            let sample_vec = &channel.sample_data;
            let average = sample_vec.values_as_f32().sum::<f32>() / sample_vec.len() as f32;

            if let Some(layer_name) = &layer.attributes.layer_name {
                println!("Channel `{}` of Layer `{}` has an average value of {}", channel.name, layer_name, average);
            }
            else {
                println!("Channel `{}` has an average value of {}", channel.name, average);
            }
        }
    }
}