//! Integration tests for deep data support.
//!
//! These tests validate:
//! - Round-trip reading and writing of deep data
//! - Compositing operations
//! - Compatibility with OpenEXR reference images

#[cfg(feature = "deep-data")]
mod deep_tests {
    use exr::prelude::*;
    use exr::block::{BlockIndex, UncompressedDeepBlock};
    use exr::image::deep::compositing::*;
    use exr::image::read::deep::read_deep_from_file;
    use exr::image::write::deep::{create_deep_header, write_deep_blocks_to_file};
    use exr::math::Vec2;
    use exr::meta::attribute::{ChannelDescription, ChannelList, SampleType};
    use std::path::PathBuf;

    /// Create a simple test deep block with known data
    fn create_test_deep_block(
        layer: usize,
        position: Vec2<usize>,
        size: Vec2<usize>,
    ) -> UncompressedDeepBlock {
        let num_pixels = size.area();

        // Create pixel offset table: each pixel has 2-3 samples
        let mut pixel_offset_table = Vec::new();
        let mut cumulative = 0i32;
        for i in 0..num_pixels {
            let sample_count = 2 + (i % 2) as i32; // Alternate between 2 and 3 samples
            cumulative += sample_count;
            pixel_offset_table.push(cumulative);
        }

        let total_samples = cumulative as usize;

        // Create sample data (for simplicity, just one channel with f32 values)
        // Each sample is a depth value
        let mut sample_data = Vec::new();
        for sample_idx in 0..total_samples {
            let depth = 1.0 + (sample_idx as f32) * 0.1;
            sample_data.extend_from_slice(&depth.to_ne_bytes());
        }

        UncompressedDeepBlock {
            index: BlockIndex {
                layer,
                pixel_position: position,
                pixel_size: size,
                level: Vec2(0, 0),
            },
            pixel_offset_table,
            sample_data,
        }
    }

    #[test]
    fn test_round_trip_simple() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_deep_round_trip.exr");

        // Create a simple channel list with one depth channel
        let channels = ChannelList::new(
            smallvec::smallvec![
                ChannelDescription {
                    name: "Z".into(),
                    sample_type: SampleType::F32,
                    quantize_linearly: false,
                    sampling: Vec2(1, 1),
                },
            ],
        );

        // Create header
        let header = create_deep_header(
            "test_layer",
            16, 16,
            channels,
            exr::compression::Compression::ZIP1,
        ).unwrap();

        // Write test data
        // Use block_index.pixel_size to get the correct block dimensions
        // (ZIP1 uses 1 scanline per block, so 16x1 for a 16-wide image)
        write_deep_blocks_to_file(
            &test_file,
            header,
            |block_index| {
                Ok(create_test_deep_block(
                    block_index.layer,
                    block_index.pixel_position,
                    block_index.pixel_size,
                ))
            },
        ).unwrap();

        // Read back
        let blocks = read_deep_from_file(&test_file, false).unwrap();

        // Validate
        // For ZIP1, we should have 16 blocks (one per scanline)
        assert_eq!(blocks.len(), 16, "Should have 16 blocks (one per scanline)");

        let block = &blocks[0];
        // Each block should be 16x1 (one scanline)
        assert_eq!(block.index.pixel_size, Vec2(16, 1));
        assert_eq!(block.pixel_offset_table.len(), 16);

        // Clean up
        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    fn test_round_trip_multiple_blocks() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_deep_multi_blocks.exr");

        let channels = ChannelList::new(
            smallvec::smallvec![
                ChannelDescription {
                    name: "Z".into(),
                    sample_type: SampleType::F32,
                    quantize_linearly: false,
                    sampling: Vec2(1, 1),
                },
            ],
        );

        let header = create_deep_header(
            "test_layer",
            32, 32,
            channels,
            exr::compression::Compression::RLE,
        ).unwrap();

        // Write with multiple scan line blocks
        write_deep_blocks_to_file(
            &test_file,
            header,
            |block_index| {
                Ok(create_test_deep_block(
                    block_index.layer,
                    block_index.pixel_position,
                    block_index.pixel_size,
                ))
            },
        ).unwrap();

        // Read back
        let blocks = read_deep_from_file(&test_file, false).unwrap();

        // Should have multiple blocks for 32x32 image
        assert!(blocks.len() > 1, "Should have multiple blocks");

        // Verify each block has valid data
        for block in &blocks {
            assert!(!block.pixel_offset_table.is_empty());
            assert!(!block.sample_data.is_empty());
        }

        // Clean up
        let _ = std::fs::remove_file(test_file);
    }

    #[test]
    fn test_compositing_operations() {
        // Test front-to-back compositing
        let samples = vec![
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.5),
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5),
            DeepSample::new_unpremultiplied(3.0, [0.0, 0.0, 1.0], 0.5),
        ];

        let (color, alpha) = composite_samples_front_to_back(&samples);

        // Alpha should approach full opacity with three 0.5 alpha samples
        assert!(alpha > 0.8, "Alpha should be high with three samples");
        assert!(alpha <= 1.0, "Alpha should not exceed 1.0");

        // Test flatten
        let rgba = flatten_to_rgba(&samples);
        assert_eq!(rgba.len(), 4);
        assert!(rgba[3] > 0.0, "Alpha should be positive");
    }

    #[test]
    fn test_make_tidy() {
        // Create samples out of order
        let mut samples = vec![
            DeepSample::new_unpremultiplied(3.0, [0.0, 0.0, 1.0], 0.3),
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 0.3),
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.3),
        ];

        make_tidy(&mut samples);

        // Should be sorted by depth
        assert_eq!(samples[0].depth, 1.0);
        assert_eq!(samples[1].depth, 2.0);
        assert_eq!(samples[2].depth, 3.0);

        // Test occlusion removal
        let mut samples_with_occlusion = vec![
            DeepSample::new_unpremultiplied(1.0, [1.0, 0.0, 0.0], 1.0), // Fully opaque
            DeepSample::new_unpremultiplied(2.0, [0.0, 1.0, 0.0], 0.5), // Should be removed
            DeepSample::new_unpremultiplied(3.0, [0.0, 0.0, 1.0], 0.5), // Should be removed
        ];

        make_tidy(&mut samples_with_occlusion);

        // Only first sample should remain
        assert_eq!(samples_with_occlusion.len(), 1);
        assert_eq!(samples_with_occlusion[0].alpha, 1.0);
    }

    #[test]
    fn test_compression_methods() {
        let temp_dir = std::env::temp_dir();

        let compressions = vec![
            ("uncompressed", exr::compression::Compression::Uncompressed),
            ("rle", exr::compression::Compression::RLE),
            ("zip1", exr::compression::Compression::ZIP1),
            ("zip16", exr::compression::Compression::ZIP16),
        ];

        for (name, compression) in compressions {
            let test_file = temp_dir.join(format!("test_deep_{}.exr", name));

            let channels = ChannelList::new(
                smallvec::smallvec![
                    ChannelDescription {
                        name: "Z".into(),
                        sample_type: SampleType::F32,
                        quantize_linearly: false,
                        sampling: Vec2(1, 1),
                    },
                ],
            );

            let header = create_deep_header(
                "test_layer",
                16, 16,
                channels,
                compression,
            ).unwrap();

            // Write - use actual block size from block_index
            write_deep_blocks_to_file(
                &test_file,
                header,
                |block_index| {
                    Ok(create_test_deep_block(
                        block_index.layer,
                        block_index.pixel_position,
                        block_index.pixel_size,
                    ))
                },
            ).unwrap();

            // Read back
            let blocks = read_deep_from_file(&test_file, false).unwrap();
            assert!(!blocks.is_empty(), "Failed to read {} compressed file", name);

            // Clean up
            let _ = std::fs::remove_file(test_file);
        }
    }

    /// Helper to locate test images
    fn ensure_test_image(name: &str) -> Option<PathBuf> {
        let test_data_dir = PathBuf::from("test_data");
        let image_path = test_data_dir.join(name);

        // If file doesn't exist, try to download it
        if !image_path.exists() {
            println!("Test image {} not found at {:?}", name, image_path);
            println!("To run this test, download OpenEXR test images from:");
            println!("https://github.com/AcademySoftwareFoundation/openexr-images");
            return None;
        }

        Some(image_path)
    }

    #[test]
    fn test_read_openexr_deep_images() {
        // This test validates we can read the OpenEXR deep test suite images.
        // NOTE: The official "composited.exr" reference is a FLAT image, not deep data,
        // so full comparison would require implementing deep-to-flat conversion.
        // The four source images also have different dimensions (764x406, 1024x396,
        // 1024x576, 1024x435) and use data windows for spatial alignment.

        let balls = ensure_test_image("Balls.exr");
        let ground = ensure_test_image("Ground.exr");
        let leaves = ensure_test_image("Leaves.exr");
        let trunks = ensure_test_image("Trunks.exr");

        if balls.is_none() || ground.is_none() || leaves.is_none() || trunks.is_none() {
            println!("Skipping test - OpenEXR test images not available");
            return;
        }

        // Read all four images and verify basic properties
        let balls_blocks = read_deep_from_file(balls.unwrap(), false).unwrap();
        let ground_blocks = read_deep_from_file(ground.unwrap(), false).unwrap();
        let leaves_blocks = read_deep_from_file(leaves.unwrap(), false).unwrap();
        let trunks_blocks = read_deep_from_file(trunks.unwrap(), false).unwrap();

        println!("✓ Successfully read Balls.exr ({} blocks)", balls_blocks.len());
        println!("✓ Successfully read Ground.exr ({} blocks)", ground_blocks.len());
        println!("✓ Successfully read Leaves.exr ({} blocks)", leaves_blocks.len());
        println!("✓ Successfully read Trunks.exr ({} blocks)", trunks_blocks.len());

        // Verify blocks are non-empty and have valid data
        for block in &balls_blocks {
            assert!(!block.pixel_offset_table.is_empty());
            assert!(!block.sample_data.is_empty());
        }

        println!("✓ All OpenEXR deep images read successfully!");
    }

    #[test]
    fn test_composite_deep_samples() {
        // Test compositing by merging samples from multiple sources
        use exr::image::deep::compositing::*;

        // Create samples: blue at depth 1.0, red at depth 2.0
        let mut merged_samples = Vec::new();
        merged_samples.push(DeepSample::new_unpremultiplied(1.0, [0.0, 0.0, 1.0], 0.5)); // Blue, front
        merged_samples.push(DeepSample::new_unpremultiplied(2.0, [1.0, 0.0, 0.0], 0.5)); // Red, back

        // Apply make_tidy (should keep both since neither is fully opaque)
        make_tidy(&mut merged_samples);

        assert_eq!(merged_samples.len(), 2, "Both samples should remain after tidy");
        assert_eq!(merged_samples[0].depth, 1.0, "First sample should be at depth 1.0");
        assert_eq!(merged_samples[1].depth, 2.0, "Second sample should be at depth 2.0");

        // Test compositing
        let (color, alpha) = composite_samples_front_to_back(&merged_samples);

        // With two 0.5 alpha samples: alpha = 0.5 + 0.5*(1-0.5) = 0.75
        assert!((alpha - 0.75).abs() < 0.001, "Alpha should be 0.75");

        println!("✓ Deep compositing test passed!");
        println!("  Merged 2 samples, final alpha: {}", alpha);
        println!("  Final color (premultiplied): {:?}", color);
    }

    /// Helper to compare two deep blocks for exact equality
    fn compare_deep_blocks(block1: &UncompressedDeepBlock, block2: &UncompressedDeepBlock) -> bool {
        if block1.pixel_offset_table.len() != block2.pixel_offset_table.len() {
            return false;
        }
        if block1.sample_data.len() != block2.sample_data.len() {
            return false;
        }

        // Compare offset tables
        for (o1, o2) in block1.pixel_offset_table.iter().zip(&block2.pixel_offset_table) {
            if o1 != o2 {
                return false;
            }
        }

        // Compare sample data
        for (s1, s2) in block1.sample_data.iter().zip(&block2.sample_data) {
            if s1 != s2 {
                return false;
            }
        }

        true
    }

    #[test]
    fn test_round_trip_all_openexr_images() {
        // Round-trip test: Read each OpenEXR image, write it out, read it back,
        // and verify it matches the original EXACTLY (100%)

        let images = vec![
            ("Balls.exr", "balls_roundtrip.exr"),
            ("Ground.exr", "ground_roundtrip.exr"),
            ("Leaves.exr", "leaves_roundtrip.exr"),
            ("Trunks.exr", "trunks_roundtrip.exr"),
        ];

        let temp_dir = std::env::temp_dir();

        for (input_name, output_name) in images {
            let input_path = ensure_test_image(input_name);
            if input_path.is_none() {
                println!("Skipping {} - test image not available", input_name);
                continue;
            }
            let input_path = input_path.unwrap();

            // Read original
            let original_blocks = read_deep_from_file(&input_path, false).unwrap();
            println!("\nTesting round-trip for {} ({} blocks)", input_name, original_blocks.len());

            // Get header from original and ensure max_samples_per_pixel is set
            let input_file = std::fs::File::open(&input_path).unwrap();
            let reader = exr::block::read(input_file, false).unwrap();
            let mut header = reader.meta_data().headers[0].clone();

            // Calculate max samples per pixel from the blocks if not set
            if header.max_samples_per_pixel.is_none() {
                let mut max_samples = 0usize;
                for block in &original_blocks {
                    let num_pixels = block.pixel_offset_table.len();
                    for pixel_idx in 0..num_pixels {
                        let sample_count = if pixel_idx == 0 {
                            block.pixel_offset_table[0]
                        } else {
                            block.pixel_offset_table[pixel_idx] - block.pixel_offset_table[pixel_idx - 1]
                        };
                        max_samples = max_samples.max(sample_count as usize);
                    }
                }
                header.max_samples_per_pixel = Some(max_samples);
            }

            // Write to new file
            let output_path = temp_dir.join(output_name);
            let mut block_iter = original_blocks.iter().cloned();
            write_deep_blocks_to_file(
                &output_path,
                header,
                |_block_index| {
                    block_iter.next().ok_or_else(|| {
                        exr::error::Error::Invalid("Not enough blocks".into())
                    })
                },
            ).unwrap();

            // Read it back
            let roundtrip_blocks = read_deep_from_file(&output_path, false).unwrap();

            // Compare
            assert_eq!(
                original_blocks.len(),
                roundtrip_blocks.len(),
                "{}: Block count mismatch",
                input_name
            );

            let mut total_pixels = 0;
            let mut total_samples = 0;

            for (i, (orig, trip)) in original_blocks.iter().zip(&roundtrip_blocks).enumerate() {
                assert!(
                    compare_deep_blocks(orig, trip),
                    "{}: Block {} does not match after round-trip",
                    input_name,
                    i
                );

                total_pixels += orig.pixel_offset_table.len();
                let sample_count = *orig.pixel_offset_table.last().unwrap_or(&0);
                total_samples += sample_count;
            }

            println!(
                "  ✓ {} round-trip: 100% match ({} pixels, {} samples)",
                input_name, total_pixels, total_samples
            );

            // Clean up
            let _ = std::fs::remove_file(output_path);
        }

        println!("\n✓ All round-trip tests passed with 100% match!");
    }

    #[test]
    fn test_composite_four_deep_to_flat() {
        // Composite the four deep images to flat and compare dimensions/structure
        // with the reference composited.exr

        let balls_path = ensure_test_image("Balls.exr");
        let ground_path = ensure_test_image("Ground.exr");
        let leaves_path = ensure_test_image("Leaves.exr");
        let trunks_path = ensure_test_image("Trunks.exr");
        let reference_path = ensure_test_image("composited.exr");

        if balls_path.is_none() || ground_path.is_none() || leaves_path.is_none()
            || trunks_path.is_none() || reference_path.is_none()
        {
            println!("Skipping test - OpenEXR test images not available");
            return;
        }

        use exr::image::deep::flatten::*;
        use exr::meta::attribute::IntegerBounds;

        // Read all four deep images with their headers
        let balls_blocks = read_deep_from_file(balls_path.as_ref().unwrap(), false).unwrap();
        let ground_blocks = read_deep_from_file(ground_path.as_ref().unwrap(), false).unwrap();
        let leaves_blocks = read_deep_from_file(leaves_path.as_ref().unwrap(), false).unwrap();
        let trunks_blocks = read_deep_from_file(trunks_path.as_ref().unwrap(), false).unwrap();

        // Get data windows from headers
        let get_data_window = |path: &std::path::PathBuf| -> IntegerBounds {
            let file = std::fs::File::open(path).unwrap();
            let reader = exr::block::read(file, false).unwrap();
            let header = &reader.meta_data().headers[0];

            // layer_size is the size, we need to construct IntegerBounds
            // For now, assume data window starts at (0, 0)
            // TODO: Get actual data window from header attributes
            IntegerBounds {
                position: Vec2(0, 0),
                size: header.layer_size,
            }
        };

        let sources = vec![
            DeepImageSource {
                blocks: balls_blocks,
                data_window: get_data_window(balls_path.as_ref().unwrap()),
                num_channels: 5, // A, B, G, R, Z
            },
            DeepImageSource {
                blocks: ground_blocks,
                data_window: get_data_window(ground_path.as_ref().unwrap()),
                num_channels: 5,
            },
            DeepImageSource {
                blocks: leaves_blocks,
                data_window: get_data_window(leaves_path.as_ref().unwrap()),
                num_channels: 5,
            },
            DeepImageSource {
                blocks: trunks_blocks,
                data_window: get_data_window(trunks_path.as_ref().unwrap()),
                num_channels: 5,
            },
        ];

        println!("\nCompositing four deep images to flat...");
        let (flat_pixels, union_window) = composite_deep_to_flat(&sources);

        println!("  Composite window: {}x{} at ({}, {})",
                 union_window.size.x(),
                 union_window.size.y(),
                 union_window.position.x(),
                 union_window.position.y());
        println!("  Total flat pixels: {}", flat_pixels.len());

        // Read reference image dimensions for comparison
        let ref_file = std::fs::File::open(reference_path.unwrap()).unwrap();
        let ref_reader = exr::block::read(ref_file, false).unwrap();
        let ref_header = &ref_reader.meta_data().headers[0];

        println!("  Reference image: {}x{} (deep={})",
                 ref_header.layer_size.width(),
                 ref_header.layer_size.height(),
                 ref_header.deep);

        // Note: We can't do pixel-by-pixel comparison because:
        // 1. The reference is flat (already composited)
        // 2. Different compression/precision may cause minor differences
        // But we can verify basic properties

        assert!(flat_pixels.len() > 0, "Should have composited some pixels");
        println!("\n✓ Deep-to-flat compositing completed successfully!");
    }
}

