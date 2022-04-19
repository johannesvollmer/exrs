
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;


// exr imports
extern crate exr;

/// Read an image with channel groups from a file.
/// Some legacy software may group layers that contain a `.` in the layer name.
///
/// Note: This is an OpenEXR legacy strategy. OpenEXR supports layers natively since 2013.
/// Use the natively supported exrs `Layer` types instead, if possible.
///
fn main() {
    use exr::prelude::*;

    let image = read().no_deep_data()
        .largest_resolution_level()

        .rgba_channels(
            |resolution, _| {
                vec![vec![(f16::ZERO, f16::ZERO, f16::ZERO, f16::ZERO); resolution.width()]; resolution.height()]
            },

            // all samples will be converted to f32 (you can also use the enum `Sample` instead of `f32` here to retain the original data type from the file)
            |vec, position, (r,g,b,a): (f16, f16, f16, f16)| {
                vec[position.y()][position.x()] = (r,g,b,a)
            }
        )

        .grouped_channels()
        .first_valid_layer()
        .all_attributes()
        .on_progress(|progress| println!("progress: {:.1}", progress*100.0))
        .from_file("tests/images/valid/openexr/MultiView/Fog.exr")
        .unwrap();

    // output a random color of each channel of each layer
    for layer in &image.layer_data {
        let (r,g,b,a) = layer.channel_data.pixels.first().unwrap().first().unwrap();

        println!(
            "top left color of layer `{}`: (r,g,b,a) = {:?}",
            layer.attributes.layer_name.clone().unwrap_or_default(),
            (r.to_f32(), g.to_f32(), b.to_f32(), a.to_f32())
        )
    }
}