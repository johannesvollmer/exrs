extern crate exr;

/// `exr` offers a few very simple functions for the most basic use cases.
/// `read_first_rgba_layer_from_file` is a simple function which loads an exr file.
/// To load the image, you need to specify how to create and how to update your image.
fn main() {
    let image = exr::prelude::read_first_rgba_layer_from_file(
        "tests/images/out/generated_rgba.exr", // run the `1_generate_rgba` example to generate this file

        // instantiate your image type with the size of the image file
        |info| {
            let default_pixel = [0.0, 0.0, 0.0, 0.0];
            let empty_line =  vec![ default_pixel; info.resolution.width() ];
            let empty_image =  vec![ empty_line; info.resolution.height() ];
            empty_image
        },

        // transfer the colors from the file to your image type
        |pixel_vector, position, (r,g,b,a)| {
            pixel_vector[position.y()][position.x()] = [
                r.to_f32(), g.to_f32(), b.to_f32(),
                a.map(exr::prelude::Sample::to_f32).unwrap_or(1.0) // alpha channel might not exist in the image, choose 1 as default alpha in this case
            ]
        },

    ).unwrap();

    // printing all pixels might kill the console lol, so only print some meta data about the image
    println!("opened file generated_rgba.exr.exr: {:#?}", image.layer_data.attributes);
}