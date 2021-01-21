
// exr imports
extern crate exr;

/// Read an image and print information about the image into the console.
/// This example shows how to read an image with multiple layers and specific channels.
/// This example does not include resolution levels (mipmaps or ripmaps).
fn main() {
    use exr::prelude::*;

    let image = read().no_deep_data()
        .largest_resolution_level()
        .specific_channels(
            ("X", "Y", "Z", "A"),
            |info: &ChannelsInfo<_>| vec![vec![(0.0, 0.0, 0.0, f16::ZERO); info.resolution.width()]; info.resolution.height()],

            // all samples will be converted to f32 (you can also use a dynamic `Sample` of `f32` instead here)
            |vec, position, (x,y,z,a): (f32, f32, f32, Option<f16>)| { // TODO infer position type
                vec[position.y()][position.x()] = (x,y,z, a.unwrap_or(f16::ONE))
            }
        )
        .all_layers()
        .all_attributes()
        .on_progress(|progress| println!("progress: {:.1}", progress*100.0))
        .from_file("tests/images/valid/openexr/Beachball/multipart.0004.exr")
        .unwrap();

    println!("image was read: {:#?}", image);

    // output the average value for each channel of each layer
    for layer in &image.layer_data {
        println!(
            "bottom left color of layer `{}`: {:?}",
            layer.attributes.layer_name.clone().unwrap_or_default(),
            layer.channel_data.storage.first().unwrap()
        )
    }
}