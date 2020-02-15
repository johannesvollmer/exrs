

// exr imports
extern crate exr;
use exr::prelude::*;
use exr::meta::Header;

/// Print the custom meta data of a file, excluding technical file meta data.
#[test]
fn print_custom_meta() {
    let meta_data = MetaData::read_from_file("D:/Pictures/openexr/crowskull/crow_uncompressed.exr").unwrap();

    for image_layer in meta_data.headers {
        println!(
            "custom meta data of layer `{}`: {:#?}",
            image_layer.name.map_or(String::new(), |text| text.to_string()),
            image_layer.custom_attributes
        );
    }
}