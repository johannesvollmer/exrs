
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;


// exr imports
extern crate exr;
use exr::prelude::simple_image::*;


/// Generate an image with channel groups and write it to a file.
/// Note: This is an OpenEXR legacy strategy. OpenEXR supports layers natively since 2013.
/// Use the natively supported exrs `Layer` types instead, if possible.
///
/// The channels have the following structure:
///
/// - Object
///     - Red
///     - Green
///     - Blue
///     - Alpha
/// - Background
///     - Red
///     - Green
///     - Blue
fn main() {
    let size = Vec2(512, 512);

    let create_channel = |name: &str| -> Channel {
        let color: f16 = f16::from_bits(rand::random::<u16>());

        Channel::color_data(
            name.try_into().unwrap(),
            Samples::F16(vec![color; size.area() ])
        )
    };

    // layers that contain a `.` will be grouped, as seen in the following example:
    let foreground_r = create_channel("Object.R");
    let foreground_g = create_channel("Object.G");
    let foreground_b = create_channel("Object.B");
    let foreground_a = create_channel("Object.A");

    let background_r = create_channel("Background.R");
    let background_g = create_channel("Background.G");
    let background_b = create_channel("Background.B");

    let layer = Layer::new(
        "test-image".try_into().unwrap(),
        size,
        smallvec![ // the order does not actually matter
            foreground_r, foreground_g, foreground_b, foreground_a,
            background_r, background_g, background_b
        ],
    );

    let image = Image::new_from_single_layer(layer);

    println!("writing image {:#?}", image);
    image.write_to_file("tests/images/out/groups.exr", write_options::high()).unwrap();

    println!("created file groups.exr");
}