
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
        .required("X").required("Y").required("Z") // can also accept a closure with a detailed selection mechanism
        .collect_channels(
            |resolution, _channels| {
                vec![vec![(f16::ZERO, 0.0, 0.0, 0.0); resolution.width()]; resolution.height()]
            },

            // all samples will be converted to f32 (you can also use the enum `Sample` instead of `f32` here to retain the original data type from the file)
            |vec, position, (a,x,y,z): (f16, f32, f32, f32)| {
                vec[position.y()][position.x()] = (a, x,y,z)
            }
        )

            // (Text::from("A"), Text::from("X"), Text::from("Y"), Text::from("Z")), // TODO use &str directly without mentioning text
            // |layer_description| vec![vec![(f16::ZERO, 0.0, 0.0, 0.0); layer_description.resolution.width()]; layer_description.resolution.height()],
            //
            // all samples will be converted to f32 (you can also use a dynamic `Sample` of `f32` instead here)
            // |vec, position, (a, x,y,z): (Option<f16>, f32, f32, f32)| { // TODO infer position type
            //     vec[position.y()][position.x()] = (a.unwrap_or(f16::ONE), x,y,z)
            // }
        // )

        .all_layers()
        .all_attributes()
        .on_progress(|progress| println!("progress: {:.1}", progress*100.0))
        .from_file("tests/images/valid/openexr/Beachball/multipart.0004.exr")
        .unwrap();

    println!("image was read: {:#?}", image);

    // output the average value for each channel of each layer
    for layer in &image.layer_data {
        println!(
            "bottom left color of layer `{}`: (a,x,y,z) = {:?}",
            layer.attributes.layer_name.clone().unwrap_or_default(),
            layer.channel_data.storage.first().unwrap()
        )
    }
}