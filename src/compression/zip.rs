// see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfCompressor.cpp

use super::optimize_bytes::{
    differences_to_samples, interleave_byte_blocks, samples_to_differences,
    separate_bytes_fragments,
};
use super::{convert_current_to_little_endian, ByteVec, ChannelList, Error, IntegerBounds};
use crate::error::Result;

// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot

pub fn decompress_bytes(
    channels: &ChannelList,
    data_le: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    let options = zune_inflate::DeflateOptions::default()
        .set_limit(expected_byte_size)
        .set_size_hint(expected_byte_size);
    let mut decoder = zune_inflate::DeflateDecoder::new_with_options(&data_le, options);
    let mut decompressed_le = decoder
        .decode_zlib()
        .map_err(|_| Error::invalid("zlib-compressed data malformed"))?;

    differences_to_samples(&mut decompressed_le);
    interleave_byte_blocks(&mut decompressed_le);

    super::convert_little_endian_to_current(decompressed_le, channels, rectangle)
    // TODO no alloc
}

pub fn compress_bytes(
    channels: &ChannelList,
    uncompressed_ne: ByteVec,
    rectangle: IntegerBounds,
) -> Result<ByteVec> {
    // see https://github.com/AcademySoftwareFoundation/openexr/blob/3bd93f85bcb74c77255f28cdbb913fdbfbb39dfe/OpenEXR/IlmImf/ImfTiledOutputFile.cpp#L750-L842
    let mut packed_le = convert_current_to_little_endian(uncompressed_ne, channels, rectangle)?;

    separate_bytes_fragments(&mut packed_le);
    samples_to_differences(&mut packed_le);

    Ok(miniz_oxide::deflate::compress_to_vec_zlib(
        packed_le.as_slice(),
        4,
    ))
}

/// Decompress raw byte data with full ZIP preprocessing pipeline.
/// Used for deep data offset tables and sample data.
/// ZIP compression includes byte interleaving AND delta encoding for deep data.
#[cfg(feature = "deep")]
pub fn decompress_raw(
    compressed_le: ByteVec,
    expected_byte_size: usize,
) -> Result<ByteVec> {
    // If compressed size equals expected size, data is stored uncompressed
    // (compression didn't help, so it was left as-is)
    if compressed_le.len() == expected_byte_size {
        return Ok(compressed_le);
    }

    let options = zune_inflate::DeflateOptions::default()
        .set_limit(expected_byte_size)
        .set_size_hint(expected_byte_size);
    let mut decoder = zune_inflate::DeflateDecoder::new_with_options(&compressed_le, options);
    let mut decompressed = decoder
        .decode_zlib()
        .map_err(|_| Error::invalid("zlib-compressed data malformed"))?;

    // DEBUG: Print first and last 40 bytes at each stage
    if super::deep_debug_enabled() && decompressed.len() >= 100 {
        eprintln!("DEBUG zip decompress: After zlib (first 40 bytes):");
        eprint!("  ");
        for (i, b) in decompressed[..40].iter().enumerate() {
            if i > 0 && i % 20 == 0 { eprint!("\n  "); }
            eprint!("{:02x} ", b);
        }
        eprintln!();
        eprintln!("DEBUG zip decompress: After zlib (last 40 bytes):");
        eprint!("  ");
        let start = decompressed.len().saturating_sub(40);
        for (i, b) in decompressed[start..].iter().enumerate() {
            if i > 0 && i % 20 == 0 { eprint!("\n  "); }
            eprint!("{:02x} ", b);
        }
        eprintln!();
    }

    // Full ZIP reconstruction pipeline (same as used for regular image data):
    // 1. Delta reconstruction
    differences_to_samples(&mut decompressed);

    if super::deep_debug_enabled() && decompressed.len() >= 100 {
        eprintln!("DEBUG zip decompress: After delta reconstruction (first 40 bytes):");
        eprint!("  ");
        for (i, b) in decompressed[..40].iter().enumerate() {
            if i > 0 && i % 20 == 0 { eprint!("\n  "); }
            eprint!("{:02x} ", b);
        }
        eprintln!();
        eprintln!("DEBUG zip decompress: After delta reconstruction (last 40 bytes):");
        eprint!("  ");
        let start = decompressed.len().saturating_sub(40);
        for (i, b) in decompressed[start..].iter().enumerate() {
            if i > 0 && i % 20 == 0 { eprint!("\n  "); }
            eprint!("{:02x} ", b);
        }
        eprintln!();
    }

    // 2. Byte interleaving (recombine even/odd bytes)
    interleave_byte_blocks(&mut decompressed);

    if super::deep_debug_enabled() && decompressed.len() >= 100 {
        eprintln!("DEBUG zip decompress: After byte interleaving (first 40 bytes):");
        eprint!("  ");
        for (i, b) in decompressed[..40].iter().enumerate() {
            if i > 0 && i % 20 == 0 { eprint!("\n  "); }
            eprint!("{:02x} ", b);
        }
        eprintln!();
        eprintln!("DEBUG zip decompress: After byte interleaving (last 40 bytes):");
        eprint!("  ");
        let start = decompressed.len().saturating_sub(40);
        for (i, b) in decompressed[start..].iter().enumerate() {
            if i > 0 && i % 20 == 0 { eprint!("\n  "); }
            eprint!("{:02x} ", b);
        }
        eprintln!();
    }

    Ok(decompressed)
}

/// Compress raw byte data with full ZIP preprocessing pipeline.
/// Used for deep data offset tables and sample data.
/// ZIP compression includes byte separation AND delta encoding for deep data.
#[cfg(feature = "deep")]
pub fn compress_raw(mut uncompressed_le: ByteVec) -> Result<ByteVec> {
    // Full ZIP compression pipeline (same as used for regular image data):
    // 1. Byte separation (split into even/odd bytes for better compression)
    separate_bytes_fragments(&mut uncompressed_le);

    // 2. Delta encoding
    samples_to_differences(&mut uncompressed_le);

    Ok(miniz_oxide::deflate::compress_to_vec_zlib(
        uncompressed_le.as_slice(),
        4,
    ))
}
