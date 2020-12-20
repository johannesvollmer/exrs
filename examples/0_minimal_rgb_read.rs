extern crate exr;

fn main() {
    let image = exr::prelude::read_first_rgba_layer_from_file(
        "tests/images/out/generated_rgba.exr", // run the `1_generate_rgba` example to generate this file

        // instantiate the two-dimensional pixel vector with the size of the image file
        |info| {
            let default_pixel = [0.0, 0.0, 0.0, 0.0];
            let empty_line =  vec![ default_pixel; info.resolution.width() ];
            let empty_image =  vec![ empty_line; info.resolution.height() ];
            empty_image
        },

        // transfer the colors from the file to our instantiated two-dimensional pixel vector
        |pixel_vector, position, pixel| {
            pixel_vector[position.y()][position.x()] = pixel.into()
        },

    ).unwrap();

    // printing all pixels might kill the console lol, so only print some meta data about the image
    println!("opened file generated_rgba.exr.exr: {:#?}", image.layer_data.attributes);
}