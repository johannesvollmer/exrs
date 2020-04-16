

// exr imports
extern crate exr;
use exr::meta::MetaData;

/// Print the custom meta data of a file, excluding technical encoding meta data.
/// Prints compression method and tile size, but not chunk count.
fn main() {
    let meta_data = MetaData::read_from_file("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();

    for image_layer in meta_data.headers {
        println!(
            "custom meta data of layer: {:#?}",
            image_layer.own_attributes
        );
    }
}