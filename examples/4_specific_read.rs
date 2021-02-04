
// exr imports
extern crate exr;

/// Read an image and print information about the image into the console.
/// This example shows how to read an image with multiple layers and specific channels.
/// This example does not include resolution levels (mipmaps or ripmaps).
fn main() {
    use exr::prelude::*;

    let image = read().no_deep_data()
        .largest_resolution_level()

        .specific_channels()
        .optional("A", f16::ONE)
        .required("Y") // TODO also accept a closure with a detailed selection mechanism
        .optional("right.Y", 0.0)
        .collect_pixels(
            |resolution, (_a_channel, _y_channel, _y_right_channel)| {
                vec![vec![(f16::ZERO, 0.0, 0.0); resolution.width()]; resolution.height()]
            },

            // all samples will be converted to f32 (you can also use the enum `Sample` instead of `f32` here to retain the original data type from the file)
            |vec, position, (a,y,yr): (f16, f32, f32)| {
                vec[position.y()][position.x()] = (a, y, yr)
            }
        )

        .all_layers()
        .all_attributes()
        .on_progress(|progress| println!("progress: {:.1}", progress*100.0))
        .from_file("tests/images/valid/openexr/MultiView/Fog.exr")
        .unwrap();

    // output a random color of each channel of each layer
    for layer in &image.layer_data {
        println!(
            "bottom left color of layer `{}`: (a,y,yr) = {:?}",
            layer.attributes.layer_name.clone().unwrap_or_default(),
            layer.channel_data.storage.first().unwrap().first().unwrap()
        )
    }
}