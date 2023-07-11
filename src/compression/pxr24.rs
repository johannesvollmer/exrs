
//! Lossy compression for F32 data, but lossless compression for U32 and F16 data.
// see https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfPxr24Compressor.cpp

// This compressor is based on source code that was contributed to
// OpenEXR by Pixar Animation Studios. The compression method was
// developed by Loren Carpenter.


//  The compressor preprocesses the pixel data to reduce entropy, and then calls zlib.
//	Compression of HALF and UINT channels is lossless, but compressing
//	FLOAT channels is lossy: 32-bit floating-point numbers are converted
//	to 24 bits by rounding the significand to 15 bits.
//
//	When the compressor is invoked, the caller has already arranged
//	the pixel data so that the values for each channel appear in a
//	contiguous block of memory.  The compressor converts the pixel
//	values to unsigned integers: For UINT, this is a no-op.  HALF
//	values are simply re-interpreted as 16-bit integers.  FLOAT
//	values are converted to 24 bits, and the resulting bit patterns
//	are interpreted as integers.  The compressor then replaces each
//	value with the difference between the value and its left neighbor.
//	This turns flat fields in the image into zeroes, and ramps into
//	strings of similar values.  Next, each difference is split into
//	2, 3 or 4 bytes, and the bytes are transposed so that all the
//	most significant bytes end up in a contiguous block, followed
//	by the second most significant bytes, and so on.  The resulting
//	string of bytes is compressed with zlib.

use super::*;

use crate::error::Result;
use lebe::io::ReadPrimitive;
use crate::math::{subsampled_image_contains_line};


// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot


#[cfg_attr(target_endian = "big", allow(unused, unreachable_code))]
pub fn compress(channels: &ChannelList, remaining_bytes: Bytes<'_>, area: IntegerBounds) -> Result<ByteVec> {
    #[cfg(target_endian = "big")] {
        return Err(Error::unsupported(
            "PXR24 compression method not supported yet on big endian processor architecture"
        ))
    }

    if remaining_bytes.is_empty() { return Ok(Vec::new()); }

    // see https://github.com/AcademySoftwareFoundation/openexr/blob/3bd93f85bcb74c77255f28cdbb913fdbfbb39dfe/OpenEXR/IlmImf/ImfTiledOutputFile.cpp#L750-L842
    let remaining_bytes = super::convert_current_to_little_endian(remaining_bytes, channels, area);
    let mut remaining_bytes = remaining_bytes.as_slice(); // TODO less allocation

    let bytes_per_pixel: usize = channels.list.iter()
        .map(|channel| match channel.sample_type {
            SampleType::F16 => 2, SampleType::F32 => 3, SampleType::U32 => 4,
        })
        .sum();

    let mut raw = vec![0_u8; bytes_per_pixel * area.size.area()];

    {
        let mut write = raw.as_mut_slice();

        // TODO this loop should be an iterator in the `IntegerBounds` class, as it is used in all compressio methods
        for y in area.position.1..area.end().1 {
            for channel in &channels.list {
                if !subsampled_image_contains_line(usize_to_i32(channel.sampling.y()), y) {
                    continue;
                }

                // this apparently can't be a closure in Rust 1.43 due to borrowing ambiguity
                let sample_count_x = channel.subsampled_resolution(area.size).0;
                macro_rules! split_off_write_slice { () => {{
                    let (slice, rest) = write.split_at_mut(sample_count_x);
                    write = rest;
                    slice
                }}; }

                let mut previous_pixel: u32 = 0;

                match channel.sample_type {
                    SampleType::F16 => {
                        let out_byte_tuples = split_off_write_slice!().iter_mut()
                            .zip(split_off_write_slice!());

                        for (out_byte_0, out_byte_1) in out_byte_tuples {
                            let pixel = u16::read_from_native_endian(&mut remaining_bytes).unwrap() as u32;
                            let [byte_1, byte_0] = (pixel.wrapping_sub(previous_pixel) as u16).to_ne_bytes();

                            *out_byte_0 = byte_0;
                            *out_byte_1 = byte_1;
                            previous_pixel = pixel;
                        }
                    },

                    SampleType::U32 => {
                        let out_byte_quadruplets = split_off_write_slice!().iter_mut()
                            .zip(split_off_write_slice!())
                            .zip(split_off_write_slice!())
                            .zip(split_off_write_slice!());

                        for (((out_byte_0, out_byte_1), out_byte_2), out_byte_3) in out_byte_quadruplets {
                            let pixel = u32::read_from_native_endian(&mut remaining_bytes).unwrap();
                            let [byte_3, byte_2, byte_1, byte_0] = pixel.wrapping_sub(previous_pixel).to_ne_bytes();

                            *out_byte_0 = byte_0;
                            *out_byte_1 = byte_1;
                            *out_byte_2 = byte_2;
                            *out_byte_3 = byte_3;
                            previous_pixel = pixel;
                        }
                    },

                    SampleType::F32 => {
                        let out_byte_triplets = split_off_write_slice!().iter_mut()
                            .zip(split_off_write_slice!())
                            .zip(split_off_write_slice!());

                        for ((out_byte_0, out_byte_1), out_byte_2) in out_byte_triplets {
                            let pixel = f32_to_f24(f32::read_from_native_endian(&mut remaining_bytes).unwrap());
                            let [byte_2, byte_1, byte_0, _] = pixel.wrapping_sub(previous_pixel).to_ne_bytes();
                            previous_pixel = pixel;

                            *out_byte_0 = byte_0;
                            *out_byte_1 = byte_1;
                            *out_byte_2 = byte_2;
                        }
                    },
                }
            }
        }

        debug_assert_eq!(write.len(), 0, "bytes left after compression");
    }

    Ok(miniz_oxide::deflate::compress_to_vec_zlib(raw.as_slice(), 4))
}

#[cfg_attr(target_endian = "big", allow(unused, unreachable_code))]
pub fn decompress(channels: &ChannelList, bytes: ByteVec, area: IntegerBounds, expected_byte_size: usize, pedantic: bool) -> Result<ByteVec> {
    #[cfg(target_endian = "big")] {
        return Err(Error::unsupported(
            "PXR24 decompression method not supported yet on big endian processor architecture"
        ))
    }

    let options = zune_inflate::DeflateOptions::default().set_limit(expected_byte_size).set_size_hint(expected_byte_size);
    let mut decoder = zune_inflate::DeflateDecoder::new_with_options(&bytes, options);
    let raw = decoder.decode_zlib()
        .map_err(|_| Error::invalid("zlib-compressed data malformed"))?; // TODO share code with zip?

    let mut read = raw.as_slice();
    let mut out = Vec::with_capacity(expected_byte_size.min(2048*4));

    for y in area.position.1 .. area.end().1 {
        for channel in &channels.list {
            if !subsampled_image_contains_line(usize_to_i32(channel.sampling.y()), y) {
                continue;
            }

            let sample_count_x = channel.subsampled_resolution(area.size).0;
            let mut read_sample_line = ||{
                if sample_count_x > read.len() { return Err(Error::invalid("not enough data")) }
                let (samples, rest) = read.split_at(sample_count_x);
                read = rest;
                Ok(samples)
            };

            let mut pixel_accumulation: u32 = 0;

            match channel.sample_type {
                SampleType::F16 => {
                    let sample_byte_pairs = read_sample_line()?.iter()
                        .zip(read_sample_line()?);

                    for (&in_byte_0, &in_byte_1) in sample_byte_pairs {
                        let difference = u16::from_ne_bytes([in_byte_1, in_byte_0]) as u32;
                        pixel_accumulation = pixel_accumulation.overflowing_add(difference).0;
                        out.extend_from_slice(&(pixel_accumulation as u16).to_ne_bytes());
                    }
                },

                SampleType::U32 => {
                    let sample_byte_quads = read_sample_line()?.iter()
                        .zip(read_sample_line()?)
                        .zip(read_sample_line()?)
                        .zip(read_sample_line()?);

                    for (((&in_byte_0, &in_byte_1), &in_byte_2), &in_byte_3) in sample_byte_quads {
                        let difference = u32::from_ne_bytes([in_byte_3, in_byte_2, in_byte_1, in_byte_0]);
                        pixel_accumulation = pixel_accumulation.overflowing_add(difference).0;
                        out.extend_from_slice(&pixel_accumulation.to_ne_bytes());
                    }
                },

                SampleType::F32 => {
                    let sample_byte_triplets = read_sample_line()?.iter()
                        .zip(read_sample_line()?).zip(read_sample_line()?);

                    for ((&in_byte_0, &in_byte_1), &in_byte_2) in sample_byte_triplets {
                        let difference = u32::from_ne_bytes([0, in_byte_2, in_byte_1, in_byte_0]);
                        pixel_accumulation = pixel_accumulation.overflowing_add(difference).0;
                        out.extend_from_slice(&pixel_accumulation.to_ne_bytes());
                    }
                }
            }
        }
    }

    if pedantic && !read.is_empty() {
        return Err(Error::invalid("too much data"));
    }

    Ok(super::convert_little_endian_to_current(&out, channels, area))
}




/// Conversion from 32-bit to 24-bit floating-point numbers.
/// Reverse conversion is just a simple 8-bit left shift.
pub fn f32_to_f24(float: f32) -> u32 {
    let bits = float.to_bits();

    let sign = bits & 0x80000000;
    let exponent = bits & 0x7f800000;
    let mantissa = bits & 0x007fffff;

    let result = if exponent == 0x7f800000 {
        if mantissa != 0 {
            // F is a NAN; we preserve the sign bit and
            // the 15 leftmost bits of the significand,
            // with one exception: If the 15 leftmost
            // bits are all zero, the NAN would turn
            // into an infinity, so we have to set at
            // least one bit in the significand.

            let mantissa = mantissa >> 8;
            (exponent >> 8) | mantissa | if mantissa == 0 { 1 } else { 0 }
        }
        else { // F is an infinity.
            exponent >> 8
        }
    }
    else { // F is finite, round the significand to 15 bits.
        let result = ((exponent | mantissa) + (mantissa & 0x00000080)) >> 8;

        if result >= 0x7f8000 {
            // F was close to FLT_MAX, and the significand was
            // rounded up, resulting in an exponent overflow.
            // Avoid the overflow by truncating the significand
            // instead of rounding it.

            (exponent | mantissa) >> 8
        }
        else {
            result
        }
    };

    return (sign >> 8) | result;
}