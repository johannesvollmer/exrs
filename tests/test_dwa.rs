use exr::prelude::*;

#[test]
fn debug_dwaa_pixel_values() {
    // Read compressed DWAA image
    let compressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/dwaa.exr")
        .expect("Failed to read DWAA F16 compressed image");

    // Read reference
    let reference = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/decompressed_dwaa.exr")
        .expect("Failed to read DWAA F16 reference image");

    println!("\n=== Compressed image channels ===");
    for chan in &compressed.layer_data[0].channel_data.list {
        let name: String = chan.name.clone().into();
        match &chan.sample_data {
            FlatSamples::F16(samples) => {
                let first_10: Vec<f32> = samples[..10.min(samples.len())].iter().map(|s| s.to_f32()).collect();
                println!("  {}: {} pixels, first 10: {:?}", name, samples.len(), first_10);
            }
            FlatSamples::F32(samples) => {
                println!("  {}: {} pixels, first 10: {:?}", name, samples.len(), &samples[..10.min(samples.len())]);
            }
            _ => {}
        }
    }

    println!("\n=== Reference image channels ===");
    for chan in &reference.layer_data[0].channel_data.list {
        let name: String = chan.name.clone().into();
        match &chan.sample_data {
            FlatSamples::F16(samples) => {
                let first_10: Vec<f32> = samples[..10.min(samples.len())].iter().map(|s| s.to_f32()).collect();
                println!("  {}: {} pixels, first 10: {:?}", name, samples.len(), first_10);
            }
            FlatSamples::F32(samples) => {
                println!("  {}: {} pixels, first 10: {:?}", name, samples.len(), &samples[..10.min(samples.len())]);
            }
            _ => {}
        }
    }
}

/// Compare pixel data between two images with lossy tolerance.
/// This only compares the actual pixel values, not metadata/encoding.
/// DWAA/DWAB uses epsilon=0.06 and max_diff=0.1 (same as exr's lossy validation).
fn assert_pixels_match_lossy<L>(image1: &Image<L>, image2: &Image<L>, test_name: &str)
where
    L: std::ops::Deref<Target = [Layer<AnyChannels<FlatSamples>>]>,
{
    assert_eq!(
        image1.layer_data.len(),
        image2.layer_data.len(),
        "{}: Layer count mismatch",
        test_name
    );

    for (layer_idx, (layer1, layer2)) in image1
        .layer_data
        .iter()
        .zip(image2.layer_data.iter())
        .enumerate()
    {
        assert_eq!(
            layer1.size, layer2.size,
            "{}: Layer {} size mismatch",
            test_name, layer_idx
        );

        assert_eq!(
            layer1.channel_data.list.len(),
            layer2.channel_data.list.len(),
            "{}: Layer {} channel count mismatch",
            test_name,
            layer_idx
        );

        for (chan1, chan2) in layer1
            .channel_data
            .list
            .iter()
            .zip(layer2.channel_data.list.iter())
        {
            let chan_name: String = chan1.name.clone().into();

            match (&chan1.sample_data, &chan2.sample_data) {
                (FlatSamples::F16(samples1), FlatSamples::F16(samples2)) => {
                    assert_eq!(
                        samples1.len(),
                        samples2.len(),
                        "{}: Layer {} channel {} pixel count mismatch",
                        test_name,
                        layer_idx,
                        chan_name
                    );

                    let epsilon = 0.06f32;
                    let max_difference = 0.1f32;

                    for (idx, (&s1, &s2)) in samples1.iter().zip(samples2.iter()).enumerate() {
                        let v1 = s1.to_f32();
                        let v2 = s2.to_f32();

                        // Handle NaN/Inf
                        if v1.is_nan() && v2.is_nan() {
                            continue;
                        }
                        if v1.is_infinite() && v2.is_infinite() && v1.signum() == v2.signum() {
                            continue;
                        }

                        let diff = (v1 - v2).abs();
                        let relative_diff = if v1.abs() > epsilon {
                            diff / v1.abs()
                        } else {
                            diff
                        };

                        assert!(
                            relative_diff <= epsilon || diff <= max_difference,
                            "{}: Layer {} channel {} pixel {}: values differ too much (v1={}, v2={}, diff={}, rel_diff={})",
                            test_name, layer_idx, chan_name, idx, v1, v2, diff, relative_diff
                        );
                    }
                }
                (FlatSamples::F32(samples1), FlatSamples::F32(samples2)) => {
                    assert_eq!(
                        samples1.len(),
                        samples2.len(),
                        "{}: Layer {} channel {} pixel count mismatch",
                        test_name,
                        layer_idx,
                        chan_name
                    );

                    let epsilon = 0.06f32;
                    let max_difference = 0.1f32;

                    for (idx, (&v1, &v2)) in samples1.iter().zip(samples2.iter()).enumerate() {
                        // Handle NaN/Inf
                        if v1.is_nan() && v2.is_nan() {
                            continue;
                        }
                        if v1.is_infinite() && v2.is_infinite() && v1.signum() == v2.signum() {
                            continue;
                        }

                        let diff = (v1 - v2).abs();
                        let relative_diff = if v1.abs() > epsilon {
                            diff / v1.abs()
                        } else {
                            diff
                        };

                        assert!(
                            relative_diff <= epsilon || diff <= max_difference,
                            "{}: Layer {} channel {} pixel {}: values differ too much (v1={}, v2={}, diff={}, rel_diff={})",
                            test_name, layer_idx, chan_name, idx, v1, v2, diff, relative_diff
                        );
                    }
                }
                _ => panic!(
                    "{}: Layer {} channel {} sample type mismatch",
                    test_name, layer_idx, chan_name
                ),
            }
        }
    }

    println!("✓ {} pixel data validation passed", test_name);
}

#[test]
fn test_dwaa_f16_vs_reference() {
    // Read compressed DWAA image
    let compressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/dwaa.exr")
        .expect("Failed to read DWAA F16 compressed image");

    // Read reference decompressed image
    let reference = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/decompressed_dwaa.exr")
        .expect("Failed to read DWAA F16 reference image");

    // Compare pixel data with lossy tolerance
    assert_pixels_match_lossy(&reference, &compressed, "DWAA F16 vs Reference");
}

#[test]
fn test_dwab_f16_vs_reference() {
    // Read compressed DWAB image
    let compressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/dwab.exr")
        .expect("Failed to read DWAB F16 compressed image");

    // Read reference decompressed image
    let reference = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/decompressed_dwab.exr")
        .expect("Failed to read DWAB F16 reference image");

    // Compare pixel data with lossy tolerance
    assert_pixels_match_lossy(&reference, &compressed, "DWAB F16 vs Reference");
}

#[test]
fn test_dwaa_f32_vs_reference() {
    // Read compressed DWAA image
    let compressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f32/dwaa.exr")
        .expect("Failed to read DWAA F32 compressed image");

    // Read reference decompressed image
    let reference = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f32/decompressed_dwaa.exr")
        .expect("Failed to read DWAA F32 reference image");

    // Compare pixel data with lossy tolerance
    assert_pixels_match_lossy(&reference, &compressed, "DWAA F32 vs Reference");
}

#[test]
fn test_dwab_f32_vs_reference() {
    // Read compressed DWAB image
    let compressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f32/dwab.exr")
        .expect("Failed to read DWAB F32 compressed image");

    // Read reference decompressed image
    let reference = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f32/decompressed_dwab.exr")
        .expect("Failed to read DWAB F32 reference image");

    // Compare pixel data with lossy tolerance
    assert_pixels_match_lossy(&reference, &compressed, "DWAB F32 vs Reference");
}

#[test]
fn test_dwaa_f16_vs_uncompressed() {
    // Read DWAA compressed image
    let compressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/dwaa.exr")
        .expect("Failed to read DWAA F16 image");

    // Read uncompressed reference
    let uncompressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f16/uncompressed.exr")
        .expect("Failed to read uncompressed F16 reference");

    // Compare pixel data with lossy tolerance
    assert_pixels_match_lossy(&uncompressed, &compressed, "DWAA F16 vs Uncompressed");
}

#[test]
fn test_dwaa_f32_vs_uncompressed() {
    // Read DWAA compressed image
    let compressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f32/dwaa.exr")
        .expect("Failed to read DWAA F32 image");

    // Read uncompressed reference
    let uncompressed = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_file("tests/images/valid/custom/compression_methods/f32/uncompressed.exr")
        .expect("Failed to read uncompressed F32 reference");

    // Compare pixel data with lossy tolerance
    assert_pixels_match_lossy(&uncompressed, &compressed, "DWAA F32 vs Uncompressed");
}
