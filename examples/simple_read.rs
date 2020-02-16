
// exr imports
extern crate exr;
use exr::prelude::*;

/// Read an image and print information about the image into the console.
/// Uses multicore decompression where appropriate.
fn main() {
    let image = simple::Image::read_from_file(
        "./testout/noisy.exr",
        read_options::high()
    ).unwrap();

    println!("image was read: {:#?}", image);
}