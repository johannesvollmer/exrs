
// exr imports
extern crate exr;
use exr::image::simple::*;


#[test]
fn read_image() {
    let image = Image::read_from_file(
        "./testout/noisy.exr",
        ReadOptions::fast()
    ).unwrap();

    println!("image was read: {:#?}", image);
}