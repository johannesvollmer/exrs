//! Test for channel subsampling support

use exr::math::Vec2;
use exr::meta::attribute::{ChannelDescription, ChannelList, IntegerBounds, SampleType, Text};

#[test]
fn test_channel_byte_calculations() {
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
fn test_422_subsampling_validation() {
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
fn test_subsampling_requires_aligned_data_window() {
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
fn test_channel_list_bytes_per_pixel_section() {
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
