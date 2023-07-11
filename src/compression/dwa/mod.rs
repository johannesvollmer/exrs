//! The DWA compression chooses one of multiple different compression techniques.
//! It automatically compresses RGB channels similar to JPEG (based on the names of the channels).
//!


mod linear_lookup_tables;
mod discrete_cosine_transform;

use crate::compression::{mod_p, ByteVec, Bytes};
use crate::error::usize_to_i32;
use crate::io::Data;
use crate::meta::attribute::ChannelList;
use crate::prelude::*;
use std::cmp::min;
use std::mem::size_of;
use lebe::io::{ReadPrimitive, WriteEndian};

/*
// TODO build this once and Arc-share across decompression threads?
#[derive(Debug, PartialEq)]
struct ChannelData {
    scheme: CompressorScheme,
    sampling: Vec2<usize>,
    sample_type: SampleType,
    quantize_linearly: bool,
    samples_per_pixel: usize,

    dimensions: Vec2<usize>,

    planar_uncompressed_buffer: Vec<u8>,
    planar_uncompressed_rle: Vec<[u8; 4]>,
    planer_uncompressed_sample_type: SampleType,
    planar_uncompressed_size: usize,
}

#[derive(Debug, PartialEq)]
enum CompressorScheme {
    Unknown, // TODO option<> instead?
    LossyDct,
    RLE,
}

const COMPRESSOR_SCHEME_NUM: usize = 3;

#[derive(Debug, PartialEq)]
enum AcCompression {
    StaticHuffman,
    Deflate,
}

#[derive(Debug, PartialEq)]
struct CscChannelSet {
    index: [usize; 3],
}

#[derive(Debug, PartialEq)]
struct Classifier {
    suffix: String,
    scheme: CompressorScheme,
    sample_type: SampleType,
    csc_index: usize,
    case_sensitive: bool,
}






pub fn decompress(
    channels: &ChannelList,
    compressed: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    debug_assert_eq!(
        expected_byte_size,
        rectangle.size.area() * channels.bytes_per_pixel,
        "expected byte size does not match header" // TODO compute instead of passing argument?
    );

    debug_assert!(!channels.list.is_empty(), "no channels found");

    if compressed.is_empty() {
        return Ok(Vec::new());
    }


    #[derive(Debug, PartialEq)]
    struct Compressor {
        ac_compression: AcCompression,
        max_scan_line_size: usize,
        num_scan_lines: usize,
        csc_sets: Vec<CscChannelSet>,
        channel_rules: Vec<Classifier>,

        packed_ac_buffer: Vec<u8>,
        packed_dc_buffer: Vec<u8>,
        rle_buffer: Vec<u8>,
        out_buffer: Vec<u8>,
        planar_unc_buffer: [u8; COMPRESSOR_SCHEME_NUM],
    }


    #[derive(Debug, PartialEq)]
    struct LossyDctDecoderBase<'r> {
        is_native_endian: bool,

        //packed_ac_count: usize,
        //packed_dc_count: usize,
        packed_ac: Vec<u8>,
        packed_dc: Vec<u8>,

        native_endian_to_linear_little_endian_lut: Option<Vec<u16>>,
        image_dimensions: Vec2<usize>,
        output_rows: Vec<Vec<&'r mut [u8]>>,
        sample_types: Vec<SampleType>, // one per output_row
        simd_aligned_data: Vec<Vec<f64>>,
    }

    /// Single Channel
    #[derive(Debug, PartialEq)]
    struct LossyDctDecoder<'r> {
        base: LossyDctDecoderBase<'r>,
    }

    /// RGB Channels
    #[derive(Debug, PartialEq)]
    struct LossyDctDecoderCsc<'r> {
        base: LossyDctDecoderBase<'r>,
    }

    unimplemented!();

    // debug_assert_eq!(out.len(), expected_byte_size);
    // Ok(super::convert_little_endian_to_current(&out, channels, rectangle))
}

pub fn compress(
    channels: &ChannelList,
    uncompressed: Bytes<'_>,
    min_max: IntegerBounds,
    ratio: Option<f32>,
    _v2: bool,
) -> Result<ByteVec> {
    if uncompressed.is_empty() {
        return Ok(Vec::new());
    }

    let _ratio = ratio.unwrap_or(45.0);

    // TODO do not convert endianness for f16-only images
    //      see https://github.com/AcademySoftwareFoundation/openexr/blob/3bd93f85bcb74c77255f28cdbb913fdbfbb39dfe/OpenEXR/IlmImf/ImfTiledOutputFile.cpp#L750-L842
    let uncompressed = super::convert_current_to_little_endian(uncompressed, channels, min_max);
    let _uncompressed = uncompressed.as_slice(); // TODO no alloc


    #[derive(Debug, PartialEq)]
    struct Compressor {
        ac_compression: AcCompression,
        max_scan_line_size: usize,
        num_scan_lines: usize,
        csc_sets: Vec<CscChannelSet>,
        channel_rules: Vec<Classifier>,

        packed_ac_buffer: Vec<u8>,
        packed_dc_buffer: Vec<u8>,
        rle_buffer: Vec<u8>,
        out_buffer: Vec<u8>,
        planar_unc_buffer: [u8; COMPRESSOR_SCHEME_NUM],
    }


    #[derive(Debug, PartialEq)]
    struct LossyDctEncoderBase<'r> {
        quantization_base_error: f32,
        dimensions: Vec2<usize>,
        little_endian_to_non_linear_native_endian_lut: Option<Vec<u16>>,

        packed_ac: Vec<u8>,
        packed_dc: Vec<u8>,

        output_rows: Vec<Vec<&'r [u8]>>,
        sample_types: Vec<SampleType>, // one per output_row
        simd_aligned_data: Vec<Vec<f64>>,

        quantization_table_y: Box<[f32; 64]>,
        quantization_table_cb_cr: Box<[f32; 64]>,
    }

    /// Single Channel
    #[derive(Debug, PartialEq)]
    struct LossyDctEncoder<'r> {
        base: LossyDctEncoderBase<'r>,
    }

    /// RGB Channels
    #[derive(Debug, PartialEq)]
    struct LossyDctEncoderCsc<'r> {
        base: LossyDctEncoderBase<'r>,
    }



    unimplemented!();
    // Ok(compressed)
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
    ) -> (ByteVec, ByteVec, ByteVec) {
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

        let compressed = super::compress(&channels, &pixel_bytes, rectangle, Some(0.5), true).unwrap();

        let decompressed = super::decompress(
            &channels, compressed.clone(), rectangle,
            pixel_bytes.len(), true
        ).unwrap();

        assert_eq!(decompressed.len(), pixel_bytes.len());
        (pixel_bytes, compressed, decompressed)
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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

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

        let (pixel_bytes, compressed, decompressed) =
            test_roundtrip_noise_with(channels, rectangle);

        assert_eq!(pixel_bytes.len(), 60);
        assert_eq!(compressed.len(), 62);
        assert_eq!(decompressed.len(), 60);
    }

}
*/