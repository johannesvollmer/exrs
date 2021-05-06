//! Run this example with all features, for example by using `cargo run --package exr --example 0_read_meta --all-features`.

// exr imports
extern crate exr;

/// Print the custom meta data of a file, excluding technical encoding meta data.
/// Prints compression method and tile size, but not purely technical data like chunk count.
fn main() {
    use exr::prelude::*;

    let mut meta_data = MetaData::read_from_file(
        "tests/images/valid/openexr/MultiResolution/Kapaa.exr",
        false // do not throw an error for invalid or missing attributes, skipping them instead
    ).unwrap();

    // remove preview attributes, as they contain very large pixel arrays,
    // which we are not interested in today
    for header in &mut meta_data.headers {
        header.own_attributes.preview.take();
    }

    // write the meta data to the console, as pretty json
    serde_json::to_writer_pretty(std::io::stdout(), &meta_data).unwrap();
}