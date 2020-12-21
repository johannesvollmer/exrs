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
        |pixel_vector, position, pixel| {
            pixel_vector[position.y()][position.x()] = pixel.into()
        },

    ).unwrap();

    // printing all pixels might kill the console lol, so only print some meta data about the image
    println!("opened file generated_rgba.exr.exr: {:#?}", image.layer_data.attributes);
}