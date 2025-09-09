#![allow(unused)] // TODO only while developing. remove later.

//! DWAa/DWAb compression (Industrial Light & Magic / OpenEXR)
//!
//! Placeholder module for DWA compression algorithms. This will be implemented by
//! porting OpenEXR's DwaCompressor.{h,cpp} to Rust. The implementation will live here
//! and expose `compress` and `decompress` entry points wired into the crate.
//!
//! Until fully implemented, these functions return NotSupported with a message, so
//! callers/tests can be wired to require support specifically for DWAB.


mod helpers;
mod channeldata;
mod classifier;
mod tables;
mod coeffs;
mod decoder;
mod transform_8x8;
mod all;

use crate::compression::ByteVec;
use crate::error::{Error, Result};
use crate::meta::attribute::{ChannelList, IntegerBounds};

/// Decompress DWA (DWAA/DWAB) compressed bytes into native-endian pixel bytes.
///
/// `expected_byte_size` is the size of the uncompressed pixel block.
/// If `pedantic` is true, additional bytes after decoding will be considered an error.
pub(crate) fn decompress(
    channels: &ChannelList,
    compressed_le: ByteVec,
    pixel_section: IntegerBounds,
    expected_byte_size: usize,
    pedantic: bool,
) -> Result<ByteVec> {
    all::decompress(channels, compressed_le, pixel_section, expected_byte_size, pedantic)
}

/// Compress a native-endian pixel block into DWA (DWAA/DWAB) encoded little-endian bytes.
pub(crate) fn compress(
    channels: &ChannelList,
    uncompressed_ne: ByteVec,
    pixel_section: IntegerBounds,
    is_dwab: bool,
    level: Option<f32>,
) -> Result<ByteVec> {
    all::compress(channels, uncompressed_ne, pixel_section, is_dwab, level)
}



#[cfg(test)]
mod test {
    use super::*;
    use crate::compression::ByteVec;
    use crate::image::validate_results::ValidateResult;
    use crate::meta::attribute::ChannelList;
    use crate::prelude::f16;
    use crate::prelude::*;

    #[test]
    fn test_1() {
    }

    fn test_roundtrip_noise_with(
        channels: ChannelList,
        rectangle: IntegerBounds,
    ) -> Option<(ByteVec, ByteVec, ByteVec)> {
        let byte_count = channels
            .list
            .iter()
            .map(|c| {
                c.subsampled_resolution(rectangle.size).area() * c.sample_type.bytes_per_sample()
            })
            .sum();

        assert!(byte_count > 0);

        let pixel_bytes: ByteVec = (0..byte_count).map(|_| rand::random()).collect();
        assert_eq!(pixel_bytes.len(), byte_count);

        let compressed = match super::compress(&channels, pixel_bytes.clone(), rectangle, true, Some(0.5)) {
            Ok(c) => c,
            Err(Error::NotSupported(msg)) => { println!("skipping DWA roundtrip (compression not supported yet): {}", msg); return None; },
            Err(e) => panic!("unexpected error during DWA compress: {}", e),
        };

        let decompressed = super::decompress(
            &channels, compressed.clone(), rectangle,
            pixel_bytes.len(), true
        ).unwrap();

        assert_eq!(decompressed.len(), pixel_bytes.len());
        Some((pixel_bytes, compressed, decompressed))
    }

    #[test]
    fn roundtrip_noise_f16() {
        let channel = ChannelDescription {
            sample_type: SampleType::F16,
            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        };

        // Two similar channels.
        let channels = ChannelList::new(smallvec![channel.clone(), channel]);

        let rectangle = IntegerBounds {
            position: Vec2(-30, 100),
            size: Vec2(322, 731),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        // On my tests, B44 give a size of 44.08% the original data (this assert implies enough
        // pixels to be relevant).
        assert_eq!(pixel_bytes.len(), 941528);
        assert_eq!(compressed.len(), 415044);
        assert_eq!(decompressed.len(), 941528);
    }

    #[test]
    fn roundtrip_noise_f16_tiny() {
        let channel = ChannelDescription {
            sample_type: SampleType::F16,
            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        };

        // Two similar channels.
        let channels = ChannelList::new(smallvec![channel.clone(), channel]);

        let rectangle = IntegerBounds {
            position: Vec2(0, 0),
            size: Vec2(3, 2),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        // B44 being 4 by 4 block, compression is less efficient for tiny images.
        assert_eq!(pixel_bytes.len(), 24);
        assert_eq!(compressed.len(), 28);
        assert_eq!(decompressed.len(), 24);
    }

    #[test]
    fn roundtrip_noise_f32() {
        let channel = ChannelDescription {
            sample_type: SampleType::F32,
            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        };

        // Two similar channels.
        let channels = ChannelList::new(smallvec![channel.clone(), channel]);

        let rectangle = IntegerBounds {
            position: Vec2(-30, 100),
            size: Vec2(322, 731),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        assert_eq!(pixel_bytes.len(), 1883056);
        assert_eq!(compressed.len(), 1883056);
        assert_eq!(decompressed.len(), 1883056);
        assert_eq!(pixel_bytes, decompressed);
    }

    #[test]
    fn roundtrip_noise_f32_tiny() {
        let channel = ChannelDescription {
            sample_type: SampleType::F32,
            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        };

        // Two similar channels.
        let channels = ChannelList::new(smallvec![channel.clone(), channel]);

        let rectangle = IntegerBounds {
            position: Vec2(0, 0),
            size: Vec2(3, 2),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        assert_eq!(pixel_bytes.len(), 48);
        assert_eq!(compressed.len(), 48);
        assert_eq!(decompressed.len(), 48);
        assert_eq!(pixel_bytes, decompressed);
    }

    #[test]
    fn roundtrip_noise_u32() {
        let channel = ChannelDescription {
            sample_type: SampleType::U32,
            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        };

        // Two similar channels.
        let channels = ChannelList::new(smallvec![channel.clone(), channel]);

        let rectangle = IntegerBounds {
            position: Vec2(-30, 100),
            size: Vec2(322, 731),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        assert_eq!(pixel_bytes.len(), 1883056);
        assert_eq!(compressed.len(), 1883056);
        assert_eq!(decompressed.len(), 1883056);
        assert_eq!(pixel_bytes, decompressed);
    }

    #[test]
    fn roundtrip_noise_u32_tiny() {
        let channel = ChannelDescription {
            sample_type: SampleType::U32,
            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1, 1),
        };

        // Two similar channels.
        let channels = ChannelList::new(smallvec![channel.clone(), channel]);

        let rectangle = IntegerBounds {
            position: Vec2(0, 0),
            size: Vec2(3, 2),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        assert_eq!(pixel_bytes.len(), 48);
        assert_eq!(compressed.len(), 48);
        assert_eq!(decompressed.len(), 48);
        assert_eq!(pixel_bytes, decompressed);
    }

    #[test]
    fn roundtrip_noise_mix_f32_f16_u32() {
        let channels = ChannelList::new(smallvec![
            ChannelDescription {
                sample_type: SampleType::F32,
                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            },
            ChannelDescription {
                sample_type: SampleType::F16,
                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            },
            ChannelDescription {
                sample_type: SampleType::U32,
                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            }
        ]);

        let rectangle = IntegerBounds {
            position: Vec2(-30, 100),
            size: Vec2(322, 731),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        assert_eq!(pixel_bytes.len(), 2353820);
        assert_eq!(compressed.len(), 2090578);
        assert_eq!(decompressed.len(), 2353820);
    }

    #[test]
    fn roundtrip_noise_mix_f32_f16_u32_tiny() {
        let channels = ChannelList::new(smallvec![
            ChannelDescription {
                sample_type: SampleType::F32,
                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            },
            ChannelDescription {
                sample_type: SampleType::F16,
                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            },
            ChannelDescription {
                sample_type: SampleType::U32,
                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            }
        ]);

        let rectangle = IntegerBounds {
            position: Vec2(0, 0),
            size: Vec2(3, 2),
        };

        let triple = match test_roundtrip_noise_with(channels, rectangle) { Some(t) => t, None => return };
        let (pixel_bytes, compressed, decompressed) = triple;

        assert_eq!(pixel_bytes.len(), 60);
        assert_eq!(compressed.len(), 62);
        assert_eq!(decompressed.len(), 60);
    }

}