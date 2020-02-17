
// exr imports
extern crate exr;
use exr::prelude::*;

/// Read an RGBA image and then write it back.
/// Uses multicore compression where appropriate.
fn main() {
    let image = rgba::Image::read_from_file("./testout/written.exr", read_options::high()).unwrap();
    println!("loaded image {:#?}", image);

    image.write_to_file("./testout/written_copy.exr", write_options::high()).unwrap();

    // just a quick check that the images are equivalent:
    assert_eq!(image, rgba::Image::read_from_file("./testout/written_copy.exr", read_options::high()).unwrap());
}