//! JavaScript integration tests for exrs-wasm.
//!
//! These tests run in Node.js or a headless browser via wasm-bindgen-test.
//! Run with: wasm-pack test --node
//! Or:       wasm-pack test --headless --chrome

use wasm_bindgen_test::*;
use exrs_wasm::*;

/// Test basic RGBA roundtrip in JavaScript environment
#[wasm_bindgen_test]
fn test_js_rgba_roundtrip() {
    let width = 4u32;
    let height = 4u32;
    let pixel_count = (width * height) as usize;

    // Create test RGBA data
    let mut pixels = Vec::with_capacity(pixel_count * 4);
    for i in 0..pixel_count {
        pixels.push(i as f64 / pixel_count as f64); // R
        pixels.push(0.5); // G
        pixels.push(0.25); // B
        pixels.push(1.0); // A
    }

    // Write using convenience function
    let bytes = write_exr_rgba(
        width,
        height,
        "test",
        &pixels,
        SamplePrecision::F32,
        CompressionMethod::None,
    ).expect("Failed to write EXR");

    assert!(!bytes.is_empty(), "EXR bytes should not be empty");

    // Verify magic number
    assert_eq!(&bytes[0..4], &[0x76, 0x2f, 0x31, 0x01], "Should have EXR magic number");

    // Read back
    let result = read_exr(&bytes).expect("Failed to read EXR");

    assert_eq!(result.width(), width);
    assert_eq!(result.height(), height);
    assert_eq!(result.layer_count(), 1);

    // Get RGBA data back
    let rgba_data = result.get_rgba_data(0).expect("Should have RGBA data");
    assert_eq!(rgba_data.len(), pixel_count * 4);

    // Verify values match
    for (i, (original, read)) in pixels.iter().zip(rgba_data.iter()).enumerate() {
        assert!(
            (original - read).abs() < 0.001,
            "Mismatch at index {}: {} vs {}",
            i, original, read
        );
    }
}

/// Test RGB roundtrip
#[wasm_bindgen_test]
fn test_js_rgb_roundtrip() {
    let width = 8u32;
    let height = 8u32;
    let pixel_count = (width * height) as usize;

    let pixels: Vec<f64> = (0..pixel_count * 3)
        .map(|i| (i as f64) / 100.0)
        .collect();

    let bytes = write_exr_rgb(
        width,
        height,
        "normals",
        &pixels,
        SamplePrecision::F32,
        CompressionMethod::Rle,
    ).expect("Failed to write RGB EXR");

    let result = read_exr(&bytes).expect("Failed to read RGB EXR");

    assert_eq!(result.width(), width);
    assert_eq!(result.height(), height);

    let rgb_data = result.get_rgb_data(0).expect("Should have RGB data");
    assert_eq!(rgb_data.len(), pixel_count * 3);

    for (original, read) in pixels.iter().zip(rgb_data.iter()) {
        assert!((original - read).abs() < 0.001);
    }
}

/// Test single channel (depth) roundtrip
#[wasm_bindgen_test]
fn test_js_depth_roundtrip() {
    let width = 16u32;
    let height = 16u32;
    let pixel_count = (width * height) as usize;

    let pixels: Vec<f64> = (0..pixel_count)
        .map(|i| i as f64)
        .collect();

    let bytes = write_exr_single_channel(
        width,
        height,
        "depth",
        "Z",
        &pixels,
        SamplePrecision::F32,
        CompressionMethod::Piz,
    ).expect("Failed to write depth EXR");

    let result = read_exr(&bytes).expect("Failed to read depth EXR");

    let z_data = result.get_channel_data(0, "Z").expect("Should have Z channel");
    assert_eq!(z_data.len(), pixel_count);

    for (original, read) in pixels.iter().zip(z_data.iter()) {
        assert!((original - read).abs() < 0.001);
    }
}

/// Test multi-layer EXR using builder API
#[wasm_bindgen_test]
fn test_js_multi_layer() {
    let width = 4u32;
    let height = 4u32;
    let pixel_count = (width * height) as usize;

    let rgba_pixels = vec![0.8f64; pixel_count * 4];
    let rgb_pixels = vec![0.5f64; pixel_count * 3];
    let depth_pixels = vec![1.0f64; pixel_count];

    let mut exr = ExrEncoder::new(width, height);

    exr.add_rgba_layer("beauty", &rgba_pixels, SamplePrecision::F32, None)
        .expect("Failed to add RGBA layer");
    exr.add_rgb_layer("normals", &rgb_pixels, SamplePrecision::F32, None)
        .expect("Failed to add RGB layer");
    exr.add_single_channel_layer("depth", "Z", &depth_pixels, SamplePrecision::F32, None)
        .expect("Failed to add depth layer");

    assert_eq!(exr.layer_count(), 3);

    let bytes = exr.to_bytes().expect("Failed to encode multi-layer EXR");

    let result = read_exr(&bytes).expect("Failed to read multi-layer EXR");

    assert_eq!(result.layer_count(), 3);
    assert_eq!(result.width(), width);
    assert_eq!(result.height(), height);

    // Verify each layer
    let beauty = result.get_rgba_data(0).expect("Should have beauty layer");
    assert_eq!(beauty.len(), pixel_count * 4);

    let normals = result.get_rgb_data(1).expect("Should have normals layer");
    assert_eq!(normals.len(), pixel_count * 3);

    let depth = result.get_channel_data(2, "Z").expect("Should have depth layer");
    assert_eq!(depth.len(), pixel_count);
}

/// Test F16 precision
#[wasm_bindgen_test]
fn test_js_f16_precision() {
    let width = 4u32;
    let height = 4u32;
    let pixel_count = (width * height) as usize;

    let pixels = vec![0.5f64; pixel_count * 4];

    let bytes = write_exr_rgba(
        width,
        height,
        "test_f16",
        &pixels,
        SamplePrecision::F16,
        CompressionMethod::None,
    ).expect("Failed to write F16 EXR");

    let result = read_exr(&bytes).expect("Failed to read F16 EXR");

    let rgba = result.get_rgba_data(0).expect("Should have RGBA data");

    // F16 has lower precision, allow larger epsilon
    for (original, read) in pixels.iter().zip(rgba.iter()) {
        assert!((original - read).abs() < 0.01, "F16 precision mismatch");
    }
}

/// Test all compression methods
#[wasm_bindgen_test]
fn test_js_compression_methods() {
    let width = 8u32;
    let height = 8u32;
    let pixel_count = (width * height) as usize;

    let pixels = vec![0.5f64; pixel_count * 4];

    let compressions = [
        CompressionMethod::None,
        CompressionMethod::Rle,
        CompressionMethod::Zip,
        CompressionMethod::Zip16,
        CompressionMethod::Piz,
        CompressionMethod::Pxr24,
    ];

    for compression in compressions {
        let bytes = write_exr_rgba(
            width,
            height,
            "test",
            &pixels,
            SamplePrecision::F32,
            compression,
        ).expect(&format!("Failed to write with {:?}", compression));

        let result = read_exr(&bytes).expect(&format!("Failed to read with {:?}", compression));
        assert_eq!(result.width(), width);
        assert_eq!(result.height(), height);
    }
}

/// Test error handling for invalid data size
#[wasm_bindgen_test]
fn test_js_invalid_data_size() {
    let result = write_exr_rgba(
        4, 4,
        "test",
        &[0.0f64; 10], // Wrong size - should be 64 (4*4*4)
        SamplePrecision::F32,
        CompressionMethod::None,
    );

    assert!(result.is_err(), "Should fail with invalid data size");
}

/// Test reading layer names
#[wasm_bindgen_test]
fn test_js_layer_names() {
    let width = 2u32;
    let height = 2u32;

    let mut exr = ExrEncoder::new(width, height);
    exr.add_rgba_layer("my_beauty", &vec![0.5f64; 16], SamplePrecision::F32, None).unwrap();
    exr.add_rgb_layer("my_normals", &vec![0.5f64; 12], SamplePrecision::F32, None).unwrap();

    let bytes = exr.to_bytes().unwrap();
    let result = read_exr(&bytes).unwrap();

    assert_eq!(result.get_layer_name(0), Some("my_beauty".to_string()));
    assert_eq!(result.get_layer_name(1), Some("my_normals".to_string()));
    assert_eq!(result.get_layer_name(99), None); // Out of bounds
}

/// Test getting channel names
#[wasm_bindgen_test]
fn test_js_channel_names() {
    let width = 2u32;
    let height = 2u32;

    let bytes = write_exr_rgba(
        width, height,
        "test",
        &vec![0.5f64; 16],
        SamplePrecision::F32,
        CompressionMethod::None,
    ).unwrap();

    let result = read_exr(&bytes).unwrap();
    let channels = result.get_channel_names(0);

    // RGBA layer should have A, B, G, R channels (sorted alphabetically by exrs)
    assert_eq!(channels.len(), 4);
    assert!(channels.contains(&"R".to_string()));
    assert!(channels.contains(&"G".to_string()));
    assert!(channels.contains(&"B".to_string()));
    assert!(channels.contains(&"A".to_string()));
}
