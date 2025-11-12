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

    assert!(result.is_ok(), "Failed to read DWAA F16 image: {:?}", result.err());
    let image = result.unwrap();
    assert!(!image.layer_data.is_empty(), "Should have at least one layer");
    println!("✓ Successfully read DWAA F16 image with {} layers", image.layer_data.len());
}

#[test]
fn test_read_dwab_f16() {
    let result = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/dwab.exr");

    assert!(result.is_ok(), "Failed to read DWAB F16 image: {:?}", result.err());
    let image = result.unwrap();
    assert!(!image.layer_data.is_empty(), "Should have at least one layer");
    println!("✓ Successfully read DWAB F16 image with {} layers", image.layer_data.len());
}

#[test]
fn test_read_dwaa_f32() {
    let result = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f32/dwaa.exr");

    assert!(result.is_ok(), "Failed to read DWAA F32 image: {:?}", result.err());
    let image = result.unwrap();
    assert!(!image.layer_data.is_empty(), "Should have at least one layer");
    println!("✓ Successfully read DWAA F32 image with {} layers", image.layer_data.len());
}
