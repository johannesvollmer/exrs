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

        let (_color, alpha) = composite_samples_front_to_back(&samples);

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
        // Composite the four deep images to flat and compare pixel-by-pixel with reference
        // This must match 100% (within floating point epsilon)

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

        // Helper to get data window from file using the proper header method
        let get_data_window = |path: &std::path::PathBuf| {
            let file = std::fs::File::open(path).unwrap();
            let reader = exr::block::read(file, false).unwrap();
            reader.meta_data().headers[0].data_window()
        };

        // Read all four deep images with their data windows
        let balls_blocks = read_deep_from_file(balls_path.as_ref().unwrap(), false).unwrap();
        let ground_blocks = read_deep_from_file(ground_path.as_ref().unwrap(), false).unwrap();
        let leaves_blocks = read_deep_from_file(leaves_path.as_ref().unwrap(), false).unwrap();
        let trunks_blocks = read_deep_from_file(trunks_path.as_ref().unwrap(), false).unwrap();

        // Debug: Check channel order in the first image
        let balls_file = std::fs::File::open(balls_path.as_ref().unwrap()).unwrap();
        let balls_reader = exr::block::read(balls_file, false).unwrap();
        let balls_header = &balls_reader.meta_data().headers[0];
        println!("\nChannel order in Balls.exr:");
        for (idx, channel) in balls_header.channels.list.iter().enumerate() {
            println!("  [{}] {} ({:?})", idx, channel.name, channel.sample_type);
        }


        // Get channel types from header (all sources have same channel structure)
        let channel_types: Vec<SampleType> = balls_header.channels.list.iter()
            .map(|c| c.sample_type)
            .collect();

        let sources = vec![
            DeepImageSource {
                blocks: balls_blocks,
                data_window: get_data_window(balls_path.as_ref().unwrap()),
                channel_types: channel_types.clone(),
            },
            DeepImageSource {
                blocks: ground_blocks,
                data_window: get_data_window(ground_path.as_ref().unwrap()),
                channel_types: channel_types.clone(),
            },
            DeepImageSource {
                blocks: leaves_blocks,
                data_window: get_data_window(leaves_path.as_ref().unwrap()),
                channel_types: channel_types.clone(),
            },
            DeepImageSource {
                blocks: trunks_blocks,
                data_window: get_data_window(trunks_path.as_ref().unwrap()),
                channel_types: channel_types.clone(),
            },
        ];

        println!("\nCompositing four deep images to flat...");
        println!("  Balls data window: {}x{} at ({}, {})",
                 sources[0].data_window.size.x(), sources[0].data_window.size.y(),
                 sources[0].data_window.position.x(), sources[0].data_window.position.y());
        println!("  Ground data window: {}x{} at ({}, {})",
                 sources[1].data_window.size.x(), sources[1].data_window.size.y(),
                 sources[1].data_window.position.x(), sources[1].data_window.position.y());
        println!("  Leaves data window: {}x{} at ({}, {})",
                 sources[2].data_window.size.x(), sources[2].data_window.size.y(),
                 sources[2].data_window.position.x(), sources[2].data_window.position.y());
        println!("  Trunks data window: {}x{} at ({}, {})",
                 sources[3].data_window.size.x(), sources[3].data_window.size.y(),
                 sources[3].data_window.position.x(), sources[3].data_window.position.y());

        // Get the reference image's data window first - we'll composite to match it
        let ref_check_file = std::fs::File::open(reference_path.as_ref().unwrap()).unwrap();
        let ref_check_reader = exr::block::read(ref_check_file, false).unwrap();
        let ref_data_win = ref_check_reader.meta_data().headers[0].data_window();
        println!("  Reference data window: {}x{} at ({}, {})",
                 ref_data_win.size.x(), ref_data_win.size.y(),
                 ref_data_win.position.x(), ref_data_win.position.y());

        // Debug: Extract and print samples for first pixel to verify data extraction
        use exr::image::deep::merge::extract_pixel_samples_typed;
        println!("\n  Debug: Leaves blocks info:");
        for (i, block) in sources[2].blocks.iter().take(3).enumerate() {
            println!("    Block {}: pos=({}, {}) size=({}, {})",
                     i,
                     block.index.pixel_position.x(),
                     block.index.pixel_position.y(),
                     block.index.pixel_size.x(),
                     block.index.pixel_size.y());
        }

        // Debug: Check multiple pixels from Leaves.exr to see the pattern
        println!("\n  Debug: Checking first 20 pixels of Leaves Y=1:");
        if let Some(block_1) = sources[2].blocks.get(1) {
            for pixel_idx in 0..20.min(block_1.pixel_offset_table.len()) {
                let pixel_samples = extract_pixel_samples_typed(block_1, pixel_idx, &channel_types);
                if !pixel_samples.is_empty() {
                    let sample = &pixel_samples[0];
                    if sample.len() >= 5 {
                        println!("    X={}: A={:.3} B={:.3} G={:.3} R={:.3} Z={:.3}",
                                 pixel_idx, sample[0], sample[1], sample[2], sample[3], sample[4]);
                    }
                }
            }
        }

        // Also check what ALL source images contribute at multiple reference pixels
        for check_x in [1, 13] {
            println!("\n  Debug: Checking all sources at pixel ({},1):", check_x);
            for (src_idx, source) in sources.iter().enumerate() {
                let src_name = ["Balls", "Ground", "Leaves", "Trunks"][src_idx];
                let global_x = check_x;
                let global_y = 1;

            // Check if this pixel is within this source's data window
            let local_x = global_x - source.data_window.position.x();
            let local_y = global_y - source.data_window.position.y();

            if local_x >= 0 && local_y >= 0
                && (local_x as usize) < source.data_window.size.x()
                && (local_y as usize) < source.data_window.size.y()
            {
                if let Some(block) = source.blocks.iter().find(|b| {
                    let block_y_start = b.index.pixel_position.y();
                    let block_y_end = block_y_start + b.index.pixel_size.y();
                    global_y as usize >= block_y_start && (global_y as usize) < block_y_end
                }) {
                    let block_y_offset = (global_y as usize) - block.index.pixel_position.y();
                    let block_width = block.index.pixel_size.x();
                    let pixel_idx = block_y_offset * block_width + (local_x as usize);

                    let pixel_samples = extract_pixel_samples_typed(block, pixel_idx, &channel_types);
                    if !pixel_samples.is_empty() {
                        println!("    {}: {} samples", src_name, pixel_samples.len());
                        for sample in pixel_samples.iter().take(1) {
                            if sample.len() >= 5 {
                                println!("      A={:.3} B={:.3} G={:.3} R={:.3} Z={:.3}",
                                         sample[0], sample[1], sample[2], sample[3], sample[4]);
                            }
                        }
                    } else {
                        println!("    {}: 0 samples (empty pixel)", src_name);
                    }
                } else {
                    println!("    {}: no block found for Y={}", src_name, global_y);
                }
            } else {
                println!("    {}: outside data window", src_name);
            }
            }
        }

        let (our_pixels, union_window) = composite_deep_to_flat(&sources);

        println!("  Our composite union: {}x{} at ({}, {})",
                 union_window.size.x(),
                 union_window.size.y(),
                 union_window.position.x(),
                 union_window.position.y());

        // Read reference flat image - use specific channels API for simplicity
        use exr::prelude::pixel_vec::PixelVec;

        let ref_image = read()
            .no_deep_data()
            .largest_resolution_level()
            .specific_channels()
            .required("R")
            .required("G")
            .required("B")
            .required("A")
            .collect_pixels(PixelVec::<(f32, f32, f32, f32)>::constructor, PixelVec::set_pixel)
            .first_valid_layer()
            .all_attributes()
            .from_file(reference_path.unwrap())
            .unwrap();

        let ref_size = ref_image.layer_data.size;
        println!("  Reference: {}x{} ({} pixels)",
                 ref_size.width(),
                 ref_size.height(),
                 ref_size.width() * ref_size.height());

        // The reference has a different data window (1,1) vs our union (0,0)
        // We need to extract the corresponding region from our composite
        let ref_width = ref_size.width();
        let ref_height = ref_size.height();
        let our_width = union_window.size.x();

        // Calculate offset: where the reference window starts in our coordinate system
        let offset_x = (ref_data_win.position.x() - union_window.position.x()) as usize;
        let offset_y = (ref_data_win.position.y() - union_window.position.y()) as usize;

        println!("  Comparing region: {}x{} pixels, offset ({}, {}) in our composite",
                 ref_width, ref_height, offset_x, offset_y);

        // Get reference pixels (R, G, B, A as f32 tuples)
        let ref_rgba_pixels = &ref_image.layer_data.channel_data.pixels.pixels;

        // Compare pixel-by-pixel - 100% exact match required
        let mut max_diff = 0.0f32;
        let mut mismatch_count = 0;

        for y in 0..ref_height {
            for x in 0..ref_width {
                // Reference pixel at (x, y)
                let ref_idx = y * ref_width + x;
                let (ref_r, ref_g, ref_b, ref_a) = ref_rgba_pixels[ref_idx];

                // Our pixel at (x + offset_x, y + offset_y) in our coordinate system
                let our_x = x + offset_x;
                let our_y = y + offset_y;
                let our_idx = our_y * our_width + our_x;
                let our_pixel = &our_pixels[our_idx];

                // Calculate differences
                let diff_r = (our_pixel.r - ref_r).abs();
                let diff_g = (our_pixel.g - ref_g).abs();
                let diff_b = (our_pixel.b - ref_b).abs();
                let diff_a = (our_pixel.a - ref_a).abs();

                max_diff = max_diff.max(diff_r).max(diff_g).max(diff_b).max(diff_a);

                // User requested: NO epsilon - 100% exact match!
                let epsilon = 0.0;
                if diff_r > epsilon || diff_g > epsilon || diff_b > epsilon || diff_a > epsilon {
                    if mismatch_count < 10 {
                        println!("  Pixel ({}, {}) [ref] / ({}, {}) [ours] mismatch:",
                                 x, y, our_x, our_y);
                        println!("    Ours: R={:.6} G={:.6} B={:.6} A={:.6}",
                                 our_pixel.r, our_pixel.g, our_pixel.b, our_pixel.a);
                        println!("    Ref:  R={:.6} G={:.6} B={:.6} A={:.6}",
                                 ref_r, ref_g, ref_b, ref_a);
                        println!("    Diff: R={:.6} G={:.6} B={:.6} A={:.6}",
                                 diff_r, diff_g, diff_b, diff_a);
                    }
                    mismatch_count += 1;
                }
            }
        }

        println!("  Max difference: {}", max_diff);
        println!("  Pixels compared: {}", ref_width * ref_height);
        println!("  Mismatches: {}", mismatch_count);

        assert_eq!(mismatch_count, 0,
            "Pixel mismatch: {} pixels differ from reference (max diff: {})",
            mismatch_count, max_diff);

        println!("\n✓ Deep-to-flat compositing: 100% pixel match with OpenEXR reference!");
    }
}
