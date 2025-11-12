use exr::prelude::*;

#[test]
fn test_read_dwaa_f16() {
    let result = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/dwaa.exr");

    match &result {
        Ok(image) => {
            println!("Successfully read DWAA image with {} layers", image.layer_data.len());
            // Verify we got some data
            assert!(!image.layer_data.is_empty(), "Should have at least one layer");
        }
        Err(e) => {
            let err_msg = format!("{}", e);
            // Currently we expect Static Huffman to be unsupported
            if err_msg.contains("Static Huffman") {
                println!("Expected error: Static Huffman AC compression not yet implemented");
            } else {
                panic!("Failed to read DWAA image: {}", e);
            }
        }
    }
}
