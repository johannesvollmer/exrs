//! Test for channel subsampling support

use exr::image::pixel_vec::PixelVec;
use exr::image::{Blocks, Encoding, Image, SpecificChannels};
use exr::math::Vec2;
use exr::meta::attribute::{ChannelDescription, ChannelList, IntegerBounds, SampleType, Text};
use exr::prelude::*;
use std::io::Cursor;

#[test]
fn channel_byte_calculations() {
    // Test that byte size calculations work correctly for subsampled channels
    let channel_full_res = ChannelDescription {
        name: Text::from("Y"),
        sample_type: SampleType::F16,
        quantize_linearly: false,
        sampling: Vec2(1, 1),
    };

    let channel_subsampled = ChannelDescription {
        name: Text::from("RY"),
        sample_type: SampleType::F16,
        quantize_linearly: true,
        sampling: Vec2(2, 2),
    };

    // Test for a 4x4 block
    let bounds = IntegerBounds {
        position: Vec2(0, 0),
        size: Vec2(4, 4),
    };

    // Full resolution channel: 4x4 = 16 pixels, 2 bytes each = 32 bytes
    let full_res_bytes = channel_full_res.byte_size_for_pixel_section(bounds);
    assert_eq!(full_res_bytes, 32);

    // Subsampled channel: 2x2 = 4 pixels, 2 bytes each = 8 bytes
    let subsampled_bytes = channel_subsampled.byte_size_for_pixel_section(bounds);
    assert_eq!(subsampled_bytes, 8);
}

#[test]
fn subsampling_422_validation() {
    // Test that 4:2:2 subsampling metadata is accepted
    // Channels must be sorted alphabetically: U, V, Y
    let channels = vec![
        ChannelDescription {
            name: Text::from("U"),
            sample_type: SampleType::F32,
            quantize_linearly: true,
            sampling: Vec2(2, 1), // 2x1 subsampling
        },
        ChannelDescription {
            name: Text::from("V"),
            sample_type: SampleType::F32,
            quantize_linearly: true,
            sampling: Vec2(2, 1), // 2x1 subsampling
        },
        ChannelDescription {
            name: Text::from("Y"),
            sample_type: SampleType::F32,
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        },
    ];

    let channel_list = ChannelList::new(smallvec::SmallVec::from_vec(channels));

    // Validate against a properly aligned data window
    let data_window = IntegerBounds {
        position: Vec2(0, 0),
        size: Vec2(8, 4), // 8x4 image, aligned for 2x1 subsampling
    };

    // Should not return an error (subsampling is now supported)
    let result = channel_list.validate(true, data_window, true);
    assert!(
        result.is_ok(),
        "4:2:2 subsampling should be valid: {:?}",
        result.err()
    );
}

#[test]
fn subsampling_requires_aligned_data_window() {
    // Test that data window must be properly aligned
    let channel = ChannelDescription {
        name: Text::from("C"),
        sample_type: SampleType::F16,
        quantize_linearly: false,
        sampling: Vec2(2, 2),
    };

    // Data window NOT aligned with sampling (size 3x3 is not divisible by 2)
    let bad_data_window = IntegerBounds {
        position: Vec2(0, 0),
        size: Vec2(3, 3),
    };

    let result = channel.validate(true, bad_data_window, true);
    assert!(
        result.is_err(),
        "Misaligned data window should fail validation"
    );

    // Data window properly aligned
    let good_data_window = IntegerBounds {
        position: Vec2(0, 0),
        size: Vec2(4, 4),
    };

    let result = channel.validate(true, good_data_window, true);
    assert!(result.is_ok(), "Aligned data window should pass validation");
}

#[test]
fn channel_list_bytes_per_pixel_section() {
    // Test ChannelList byte calculation with mixed subsampling
    let channels = vec![
        ChannelDescription {
            name: Text::from("Y"),
            sample_type: SampleType::F16, // 2 bytes
            quantize_linearly: false,
            sampling: Vec2(1, 1), // Full resolution
        },
        ChannelDescription {
            name: Text::from("RY"),
            sample_type: SampleType::F16, // 2 bytes
            quantize_linearly: true,
            sampling: Vec2(2, 2), // 2x2 subsampling
        },
        ChannelDescription {
            name: Text::from("BY"),
            sample_type: SampleType::F16, // 2 bytes
            quantize_linearly: true,
            sampling: Vec2(2, 2), // 2x2 subsampling
        },
    ];

    let channel_list = ChannelList::new(smallvec::SmallVec::from_vec(channels));

    // 4x4 block
    let bounds = IntegerBounds {
        position: Vec2(0, 0),
        size: Vec2(4, 4),
    };

    // Y: 4x4 = 16 pixels * 2 bytes = 32 bytes
    // RY: 2x2 = 4 pixels * 2 bytes = 8 bytes
    // BY: 2x2 = 4 pixels * 2 bytes = 8 bytes
    // Total: 48 bytes
    let total_bytes = channel_list.bytes_per_pixel_section(bounds);
    assert_eq!(total_bytes, 48);
}

#[test]
fn write_read_422_subsampled_image() {
    // Test writing and reading a 4:2:2 subsampled image (Y=full, Cb/Cr=half horizontal)
    // This matches typical video chroma subsampling

    let width = 64;
    let height = 48;

    // Create three channels: Y (full resolution), Cb and Cr (horizontally subsampled)
    let y_channel = AnyChannel {
        name: Text::from("Y"),
        sample_data: FlatSamples::F32(vec![0.5_f32; width * height]),
        quantize_linearly: false,
        sampling: Vec2(1, 1),
    };

    let cb_channel = AnyChannel {
        name: Text::from("Cb"),
        sample_data: FlatSamples::F32(vec![0.3_f32; (width / 2) * height]),
        quantize_linearly: false,
        sampling: Vec2(2, 1), // Horizontally subsampled
    };

    let cr_channel = AnyChannel {
        name: Text::from("Cr"),
        sample_data: FlatSamples::F32(vec![0.7_f32; (width / 2) * height]),
        quantize_linearly: false,
        sampling: Vec2(2, 1), // Horizontally subsampled
    };

    let channels = AnyChannels::sort(smallvec::smallvec![y_channel, cb_channel, cr_channel]);

    let layer = Layer::new(
        (width, height),
        LayerAttributes::named(""),
        Encoding {
            compression: Compression::Uncompressed,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    );

    let image = Image::from_layer(layer);

    // Write to memory
    let mut buffer = Vec::new();
    image
        .write()
        .to_buffered(Cursor::new(&mut buffer))
        .expect("Failed to write subsampled image");

    // Read it back
    let read_image = read()
        .no_deep_data()
        .all_resolution_levels()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_buffered(Cursor::new(&buffer))
        .expect("Failed to read subsampled image");

    // Verify the image was read correctly
    assert_eq!(read_image.layer_data.len(), 1);
    let layer = &read_image.layer_data[0];
    assert_eq!(layer.channel_data.list.len(), 3);

    // Check each channel
    for channel in &layer.channel_data.list {
        let channel_name = channel.name.to_string();
        match channel_name.as_str() {
            "Y" => {
                assert_eq!(channel.sampling, Vec2(1, 1));
                if let Levels::Singular(FlatSamples::F32(samples)) = &channel.sample_data {
                    assert_eq!(samples.len(), width * height);
                } else {
                    panic!("Y channel should be F32");
                }
            }
            "Cb" | "Cr" => {
                assert_eq!(channel.sampling, Vec2(2, 1));
                if let Levels::Singular(FlatSamples::F32(samples)) = &channel.sample_data {
                    assert_eq!(samples.len(), (width / 2) * height);
                } else {
                    panic!("Cb/Cr channels should be F32");
                }
            }
            _ => panic!("Unexpected channel: {}", channel.name),
        }
    }
}

#[test]
fn write_read_420_subsampled_image() {
    // Test writing and reading a 4:2:0 subsampled image (chroma half in both dimensions)

    let width = 64;
    let height = 48;

    // Create three channels with 4:2:0 subsampling
    let y_channel = AnyChannel {
        name: Text::from("Y"),
        sample_data: FlatSamples::F16(vec![f16::from_f32(0.5); width * height]),
        quantize_linearly: false,
        sampling: Vec2(1, 1),
    };

    let ry_channel = AnyChannel {
        name: Text::from("RY"),
        sample_data: FlatSamples::F16(vec![f16::from_f32(0.3); (width / 2) * (height / 2)]),
        quantize_linearly: true,
        sampling: Vec2(2, 2), // Both horizontally and vertically subsampled
    };

    let by_channel = AnyChannel {
        name: Text::from("BY"),
        sample_data: FlatSamples::F16(vec![f16::from_f32(0.7); (width / 2) * (height / 2)]),
        quantize_linearly: true,
        sampling: Vec2(2, 2), // Both horizontally and vertically subsampled
    };

    let channels = AnyChannels::sort(smallvec::smallvec![y_channel, ry_channel, by_channel]);

    let layer = Layer::new(
        (width, height),
        LayerAttributes::named(""),
        Encoding {
            compression: Compression::Uncompressed,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    );

    let image = Image::from_layer(layer);

    // Write to memory
    let mut buffer = Vec::new();
    image
        .write()
        .to_buffered(Cursor::new(&mut buffer))
        .expect("Failed to write 4:2:0 subsampled image");

    // Read it back
    let read_image = read()
        .no_deep_data()
        .all_resolution_levels()
        .all_channels()
        .all_layers()
        .all_attributes()
        .from_buffered(Cursor::new(&buffer))
        .expect("Failed to read 4:2:0 subsampled image");

    // Verify the image was read correctly
    assert_eq!(read_image.layer_data.len(), 1);
    let layer = &read_image.layer_data[0];
    assert_eq!(layer.channel_data.list.len(), 3);

    // Check each channel
    for channel in &layer.channel_data.list {
        let channel_name = channel.name.to_string();
        match channel_name.as_str() {
            "Y" => {
                assert_eq!(channel.sampling, Vec2(1, 1));
                if let Levels::Singular(FlatSamples::F16(samples)) = &channel.sample_data {
                    assert_eq!(samples.len(), width * height);
                } else {
                    panic!("Y channel should be F16");
                }
            }
            "BY" | "RY" => {
                assert_eq!(channel.sampling, Vec2(2, 2));
                if let Levels::Singular(FlatSamples::F16(samples)) = &channel.sample_data {
                    assert_eq!(samples.len(), (width / 2) * (height / 2));
                } else {
                    panic!("BY/RY channels should be F16");
                }
            }
            _ => panic!("Unexpected channel: {}", channel.name),
        }
    }
}

#[test]
fn specific_channels_handles_subsampling() {
    let width = 6;
    let height = 4;
    let size = Vec2(width, height);

    let y_channel = AnyChannel {
        name: Text::from("Y"),
        sample_data: FlatSamples::F32(
            (0..size.area())
                .map(|index| {
                    let x = index % width;
                    let y = index / width;
                    (x as f32) + (y as f32) * 10.0
                })
                .collect(),
        ),
        quantize_linearly: false,
        sampling: Vec2(1, 1),
    };

    let cb_channel = AnyChannel {
        name: Text::from("Cb"),
        sample_data: FlatSamples::F32(
            (0..(width / 2) * height)
                .map(|index| 100.0 + index as f32)
                .collect(),
        ),
        quantize_linearly: true,
        sampling: Vec2(2, 1),
    };

    let channels = AnyChannels::sort(smallvec::smallvec![y_channel, cb_channel]);

    let layer = Layer::new(
        size,
        LayerAttributes::named(""),
        Encoding {
            compression: Compression::Uncompressed,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    );

    let image = Image::from_layer(layer);

    let mut buffer = Vec::new();
    image
        .write()
        .to_buffered(Cursor::new(&mut buffer))
        .expect("failed to write subsampled image");

    let parsed = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .required("Y")
        .required("Cb")
        .collect_pixels(PixelVec::<(f32, f32)>::constructor, PixelVec::set_pixel)
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(&buffer))
        .expect("failed to read subsampled image");

    let read_pixels = &parsed.layer_data.channel_data.pixels;

    for y in 0..height {
        for x in 0..width {
            let pos = Vec2(x, y);
            let reconstructed = read_pixels.get_pixel(pos);

            if x % 2 == 0 {
                assert!(
                    reconstructed.1 >= 100.0,
                    "subsampled channel should have data at {:?}",
                    pos
                );
            } else {
                assert_eq!(
                    reconstructed.1, 0.0,
                    "even when no subsampled data, default should remain at {:?}",
                    pos
                );
            }
        }
    }
}

#[test]
fn specific_channels_roundtrip_422_subsampling() {
    let width = 8;
    let height = 4;
    let size = Vec2(width, height);

    let subsampled_pixels = PixelVec::new(
        size,
        (0..size.area())
            .map(|index| {
                let x = index % width;
                let y = index / width;
                let y_value = (x as f32) + (y as f32) * 10.0;
                let cb_value = if x % 2 == 0 { 100.0 + y_value } else { 0.0 };
                (y_value, cb_value)
            })
            .collect(),
    );

    fn channel(name: &str, sampling: Vec2<usize>, quantize_linearly: bool) -> ChannelDescription {
        ChannelDescription {
            name: Text::from(name),
            sample_type: SampleType::F32,
            quantize_linearly,
            sampling,
        }
    }

    let channels = SpecificChannels::build()
        .with_channel_details::<f32>(channel("Y", Vec2(1, 1), false))
        .with_channel_details::<f32>(channel("Cb", Vec2(2, 1), true))
        .with_pixels(subsampled_pixels.clone());

    let image = Image::from_encoded_channels(
        size,
        Encoding {
            compression: Compression::Uncompressed,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    );

    let mut buffer = Vec::new();
    image
        .write()
        .non_parallel()
        .to_buffered(Cursor::new(&mut buffer))
        .expect("failed to write specific channels with 4:2:2 subsampling");

    let parsed = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .required("Y")
        .required("Cb")
        .collect_pixels(PixelVec::<(f32, f32)>::constructor, PixelVec::set_pixel)
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(&buffer))
        .expect("failed to read 4:2:2 subsampled specific channels");

    let read_pixels = &parsed.layer_data.channel_data.pixels;

    for y in 0..height {
        for x in 0..width {
            let position = Vec2(x, y);
            let reference = subsampled_pixels.get_pixel(position);
            let result = read_pixels.get_pixel(position);

            assert_eq!(reference.0, result.0, "luma mismatch at {:?}", position);

            if x % 2 == 0 {
                assert_eq!(reference.1, result.1, "chroma mismatch at {:?}", position);
            } else {
                assert_eq!(
                    result.1, 0.0,
                    "non-sampled chroma pixel should keep default at {:?}",
                    position
                );
            }
        }
    }
}

#[test]
fn specific_channels_roundtrip_420_subsampling() {
    let width = 6;
    let height = 6;
    let size = Vec2(width, height);

    let subsampled_pixels = PixelVec::new(
        size,
        (0..size.area())
            .map(|index| {
                let x = index % width;
                let y = index / width;
                let y_value = (x as f32) + (y as f32) * 5.0;
                let chroma = if x % 2 == 0 && y % 2 == 0 {
                    200.0 + y_value
                } else {
                    0.0
                };
                (y_value, chroma)
            })
            .collect(),
    );

    fn channel(name: &str, sampling: Vec2<usize>) -> ChannelDescription {
        ChannelDescription {
            name: Text::from(name),
            sample_type: SampleType::F32,
            quantize_linearly: false,
            sampling,
        }
    }

    let channels = SpecificChannels::build()
        .with_channel_details::<f32>(channel("Y", Vec2(1, 1)))
        .with_channel_details::<f32>(channel("Chroma", Vec2(2, 2)))
        .with_pixels(subsampled_pixels.clone());

    let image = Image::from_encoded_channels(
        size,
        Encoding {
            compression: Compression::Uncompressed,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    );

    let mut buffer = Vec::new();
    image
        .write()
        .non_parallel()
        .to_buffered(Cursor::new(&mut buffer))
        .expect("failed to write 4:2:0 specific channels");

    let parsed = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .required("Y")
        .required("Chroma")
        .collect_pixels(PixelVec::<(f32, f32)>::constructor, PixelVec::set_pixel)
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(&buffer))
        .expect("failed to read 4:2:0 specific channels");

    let read_pixels = &parsed.layer_data.channel_data.pixels;

    for y in 0..height {
        for x in 0..width {
            let position = Vec2(x, y);
            let reference = subsampled_pixels.get_pixel(position);
            let result = read_pixels.get_pixel(position);

            assert_eq!(reference.0, result.0, "luma mismatch at {:?}", position);

            if x % 2 == 0 && y % 2 == 0 {
                assert_eq!(reference.1, result.1, "chroma mismatch at {:?}", position);
            } else {
                assert_eq!(result.1, 0.0, "default chroma expected at {:?}", position);
            }
        }
    }
}

#[test]
fn specific_channels_read_modify_rewrite_roundtrip() {
    let width = 10;
    let height = 6;
    let size = Vec2(width, height);

    fn channel(name: &str, sampling: Vec2<usize>) -> ChannelDescription {
        ChannelDescription {
            name: Text::from(name),
            sample_type: SampleType::F32,
            quantize_linearly: false,
            sampling,
        }
    }

    let original_pixels = PixelVec::new(
        size,
        (0..size.area())
            .map(|index| {
                let x = index % width;
                let y = index / width;
                let y_value = (x as f32) + (y as f32) * 3.0;
                let u = if x % 2 == 0 { 50.0 + y_value } else { 0.0 };
                let v = if y % 2 == 0 { 75.0 + y_value } else { 0.0 };
                (y_value, u, v)
            })
            .collect(),
    );

    let channels = SpecificChannels::build()
        .with_channel_details::<f32>(channel("Y", Vec2(1, 1)))
        .with_channel_details::<f32>(channel("U", Vec2(2, 1)))
        .with_channel_details::<f32>(channel("V", Vec2(1, 2)))
        .with_pixels(original_pixels.clone());

    let encoding = Encoding {
        compression: Compression::Uncompressed,
        blocks: Blocks::ScanLines,
        line_order: LineOrder::Increasing,
    };

    let image = Image::from_encoded_channels(size, encoding, channels);

    let mut buffer = Vec::new();
    image
        .write()
        .to_buffered(Cursor::new(&mut buffer))
        .expect("failed to write original subsampled image");

    let first_roundtrip = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .required("Y")
        .required("U")
        .required("V")
        .collect_pixels(
            PixelVec::<(f32, f32, f32)>::constructor,
            PixelVec::set_pixel,
        )
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(&buffer))
        .expect("failed to read original subsampled image");

    let mut second_buffer = Vec::new();
    let rewrite_specific = SpecificChannels::new(
        first_roundtrip.layer_data.channel_data.channels.clone(),
        first_roundtrip.layer_data.channel_data.pixels.clone(),
    );
    Image::from_encoded_channels(size, encoding, rewrite_specific)
        .write()
        .to_buffered(Cursor::new(&mut second_buffer))
        .expect("failed to rewrite subsampled image");

    let second_roundtrip = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .required("Y")
        .required("U")
        .required("V")
        .collect_pixels(
            PixelVec::<(f32, f32, f32)>::constructor,
            PixelVec::set_pixel,
        )
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .from_buffered(Cursor::new(&second_buffer))
        .expect("failed to read rewritten subsampled image");

    let rewritten_pixels = &second_roundtrip.layer_data.channel_data.pixels;

    for y in 0..height {
        for x in 0..width {
            let position = Vec2(x, y);
            let reference = original_pixels.get_pixel(position);
            let result = rewritten_pixels.get_pixel(position);

            assert_eq!(
                reference.0, result.0,
                "luma mismatch after rewrite at {:?}",
                position
            );

            if x % 2 == 0 {
                assert_eq!(
                    reference.1, result.1,
                    "U mismatch after rewrite at {:?}",
                    position
                );
            } else {
                assert_eq!(
                    result.1, 0.0,
                    "non-sampled U should remain default at {:?}",
                    position
                );
            }

            if y % 2 == 0 {
                assert_eq!(
                    reference.2, result.2,
                    "V mismatch after rewrite at {:?}",
                    position
                );
            } else {
                assert_eq!(
                    result.2, 0.0,
                    "non-sampled V should remain default at {:?}",
                    position
                );
            }
        }
    }
}
