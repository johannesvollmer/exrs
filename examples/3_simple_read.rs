
// exr imports
extern crate exr;
use exr::prelude::*;

/// Read an image and print information about the image into the console.
/// Uses multi-core decompression where appropriate.
fn main() {
    let image = simple::Image::read_from_file(
        "tests/images/valid/openexr/Beachball/multipart.0004.exr",
        read_options::high()
    ).unwrap();

    println!("image was read: {:#?}", image);
}