extern crate exr;

/// `exr` offers a few very simple functions for the most basic use cases.
/// `read_first_rgba_layer_from_file` is a simple function which loads an exr file.
/// To load the image, you need to specify how to create and how to update your image.
fn main() {
    let image = exr::prelude::read_first_rgba_layer_from_file(
        "tests/images/out/generated_rgba.exr", // run the `1_generate_rgba` example to generate this file

        // instantiate your image type with the size of the image file
        |resolution, _channels| {
            let default_pixel = [0.0, 0.0, 0.0, 0.0];
            let empty_line =  vec![ default_pixel; resolution.width() ];
            let empty_image =  vec![ empty_line; resolution.height() ];
            empty_image
        },

        // transfer the colors from the file to your image type,
        // requesting all values to be converted to f32 numbers (you can also directly use f16 instead)
        // and you could also use `Sample` instead of `f32` to keep the original data type from the file
        |pixel_vector, position, (r,g,b, a): (f32, f32, f32, f32)| {
            pixel_vector[position.y()][position.x()] = [r, g, b, a]
        },

    ).unwrap();

    // printing all pixels might kill the console, so only print some meta data about the image
    println!("opened file generated_rgba.exr: {:#?}", image.layer_data.attributes);
}