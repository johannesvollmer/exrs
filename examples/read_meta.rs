

// exr imports
extern crate exr;
use exr::meta::MetaData;

/// Print the custom meta data of a file, excluding technical file meta data.
fn main() {
    let meta_data = MetaData::read_from_file("D:/Pictures/openexr/crowskull/crow_uncompressed.exr").unwrap();

    for image_layer in meta_data.headers {
        println!(
            "custom meta data of layer: {:#?}",
            image_layer.own_attributes
        );
    }
}