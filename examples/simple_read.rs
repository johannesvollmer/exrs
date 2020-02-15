
// exr imports
extern crate exr;
use exr::image::simple::*;

/// Read an image and print information about the image into the console.
/// Uses multicore decompression where appropriate.
#[test]
fn read_image() {
    let image = Image::read_from_file(
        "./testout/noisy.exr",
        ReadOptions::high()
    ).unwrap();

    println!("image was read: {:#?}", image);
}