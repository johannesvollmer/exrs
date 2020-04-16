
// exr imports
extern crate exr;
use exr::prelude::*;

/// Read an image and print information about the image into the console.
fn main() {

    let image = simple::Image::read_from_file(
        "tests/images/valid/openexr/Beachball/multipart.0004.exr",
        read_options::high() // use multi-core decompression
    ).unwrap();

    println!("image was read: {:#?}", image);

    // output the average value for each channel of each layer
    for layer in &image.layers {
        for channel in &layer.channels {
            let average = match &channel.samples {
                simple::Samples::F16(f16_vec) => f16_vec.iter().map(|f| f.to_f32()).sum::<f32>() / f16_vec.len() as f32,
                simple::Samples::F32(f32_vec) => f32_vec.iter().sum::<f32>() / f32_vec.len() as f32,
                simple::Samples::U32(u32_vec) => u32_vec.iter().sum::<u32>() as f32 / u32_vec.len() as f32,
            };

            println!(
                "Channel {} of Layer {:?} has an average value of {}",
                channel.name, layer.attributes.name, average
            );
        }
    }
}