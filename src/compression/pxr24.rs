
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
use inflate::inflate_bytes_zlib;
use crate::prelude::attributes::ChannelList;
use crate::prelude::SampleType;
use crate::prelude::meta::attributes::Channel;
use lebe::io::ReadPrimitive;
use deflate::write::ZlibEncoder;
use std::ops::Index;


// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot


pub fn compress(channels: &ChannelList, mut remaining_bytes: Bytes<'_>, area: IntRect) -> Result<ByteVec> {
    if remaining_bytes.is_empty() { return Ok(Vec::new()); }

    let bytes_per_pixel: usize = channels.list.iter()
        .map(|channel| match channel.sample_type {
            SampleType::F16 => 2, SampleType::F32 => 3, SampleType::U32 => 4,
        })
        .sum();

    let mut raw = vec![0_u8; bytes_per_pixel * area.size.area()];
    let mut write_index = 0;

    for y in area.position.1 .. area.end().1 {
        for channel in &channels.list {
            if mod_p(y, channel.sampling.1 as i32) != 0 { continue; }
            let sample_count_x = channel.subsampled_resolution(area.size).0;

            let mut write_indices = [0_usize; 4];
            let mut previous_pixel: u32 = 0;

            match channel.sample_type {
                SampleType::F16 => {
                    write_indices[0] = write_index;
                    write_indices[1] = write_indices[0] + sample_count_x;
                    write_index = write_indices[1] + sample_count_x;

                    for _ in 0..sample_count_x {
                        let pixel = u16::read_from_native_endian(&mut remaining_bytes).unwrap() as u32;
                        let difference = pixel.wrapping_sub(previous_pixel);
                        previous_pixel = pixel;

                        raw[write_indices[0]] = (difference >> 8) as u8;
                        raw[write_indices[1]] = difference as u8;

                        write_indices[0] += 1;
                        write_indices[1] += 1;
                    }
                },

                SampleType::U32 => {
                    write_indices[0] = write_index;
                    write_indices[1] = write_indices[0] + sample_count_x;
                    write_indices[2] = write_indices[1] + sample_count_x;
                    write_indices[3] = write_indices[2] + sample_count_x;
                    write_index = write_indices[3] + sample_count_x;

                    for _ in 0..sample_count_x {
                        let pixel = u32::read_from_native_endian(&mut remaining_bytes).unwrap();
                        let difference = pixel.wrapping_sub(previous_pixel);
                        previous_pixel = pixel;

                        raw[write_indices[0]] = (difference >> 24) as u8;
                        raw[write_indices[1]] = (difference >> 16) as u8;
                        raw[write_indices[2]] = (difference >> 8) as u8;
                        raw[write_indices[3]] = difference as u8;

                        write_indices[0] += 1;
                        write_indices[1] += 1;
                        write_indices[2] += 1;
                        write_indices[3] += 1;
                    }
                },

                SampleType::F32 => {
                    write_indices[0] = write_index;
                    write_indices[1] = write_indices[0] + sample_count_x;
                    write_indices[2] = write_indices[1] + sample_count_x;
                    write_index = write_indices[2] + sample_count_x;

                    for _ in 0..sample_count_x {
                        let pixel = f32_to_f24(f32::read_from_native_endian(&mut remaining_bytes).unwrap());
                        let difference = pixel.wrapping_sub(previous_pixel);
                        previous_pixel = pixel;

                        raw[write_indices[0]] = (difference >> 16) as u8;
                        raw[write_indices[1]] = (difference >> 8) as u8;
                        raw[write_indices[2]] = difference as u8;

                        write_indices[0] += 1;
                        write_indices[1] += 1;
                        write_indices[2] += 1;
                    }
                },
            }
        }
    }

    // TODO fine-tune compression options
    let mut compressor = ZlibEncoder::new(
        Vec::with_capacity(raw.len()),
        deflate::Compression::Fast
    );

    debug_assert_eq!(raw.len(), write_index);
    std::io::copy(&mut raw.as_slice(), &mut compressor)?;
    Ok(compressor.finish()?)
}



pub fn decompress(channels: &ChannelList, bytes: Bytes<'_>, area: IntRect, expected_byte_size: usize) -> Result<ByteVec> {
    if bytes.is_empty() { return Ok(Vec::new()) }

    let raw = inflate_bytes_zlib(bytes)
        .map_err(|msg| Error::invalid(msg))?; // TODO share code with zip?

    let mut read_index = 0;
    let mut write = Vec::with_capacity(expected_byte_size.min(2048*4));

    for y in area.position.1 .. area.end().1 {
        for channel in &channels.list {
            if mod_p(y, channel.sampling.1 as i32) != 0 { continue; }

            let sample_count_x = channel.subsampled_resolution(area.size).0;

            let mut read_indices = [0_usize; 4];
            let mut pixel_accumulation: u32 = 0;

            match channel.sample_type {
                SampleType::F16 => {
                    read_indices[0] = read_index;
                    read_indices[1] = read_indices[0] + sample_count_x;
                    read_index = read_indices[1] + sample_count_x;

                    if read_index > raw.len() {
                        return Err(Error::invalid("not enough data"));
                    }

                    for _ in 0..sample_count_x {
                        let difference = u16::from_ne_bytes([raw[read_indices[1]], raw[read_indices[0]]]) as u32;
                        read_indices[0] += 1;
                        read_indices[1] += 1;

                        pixel_accumulation = pixel_accumulation.overflowing_add(difference).0;
                        write.extend_from_slice(&(pixel_accumulation as u16).to_ne_bytes());
                    }
                },

                SampleType::U32 => {
                    read_indices[0] = read_index;
                    read_indices[1] = read_indices[0] + sample_count_x;
                    read_indices[2] = read_indices[1] + sample_count_x;
                    read_indices[3] = read_indices[2] + sample_count_x;
                    read_index = read_indices[3] + sample_count_x;

                    if read_index > raw.len() {
                        return Err(Error::invalid("not enough data"));
                    }

                    for _ in 0..sample_count_x {
                        let difference = u32::from_ne_bytes([
                            raw[read_indices[3]], raw[read_indices[2]],
                            raw[read_indices[1]], raw[read_indices[0]],
                        ]);

                        read_indices[0] += 1;
                        read_indices[1] += 1;
                        read_indices[2] += 1;
                        read_indices[3] += 1;

                        pixel_accumulation = pixel_accumulation.overflowing_add(difference).0;
                        write.extend_from_slice(&pixel_accumulation.to_ne_bytes());
                    }
                },

                SampleType::F32 => {
                    read_indices[0] = read_index;
                    read_indices[1] = read_indices[0] + sample_count_x;
                    read_indices[2] = read_indices[1] + sample_count_x;
                    read_index = read_indices[2] + sample_count_x;

                    if read_index > raw.len() {
                        return Err(Error::invalid("not enough data"));
                    }

                    for _ in 0..sample_count_x {
                        let difference = u32::from_ne_bytes([
                            0, raw[read_indices[2]], raw[read_indices[1]], raw[read_indices[0]],
                        ]);

                        read_indices[0] += 1;
                        read_indices[1] += 1;
                        read_indices[2] += 1;

                        pixel_accumulation = pixel_accumulation.overflowing_add(difference).0;
                        write.extend_from_slice(&pixel_accumulation.to_ne_bytes());
                    }
                }
            }
        }
    }

    if read_index != raw.len() {
        return Err(Error::invalid("too much data"));
    }

    Ok(write)
}




// TODO share code with piz?
fn mod_p(x: i32, y: i32) -> i32 {
    x - y * div_p(x, y)
}

// TODO share code with piz?
fn div_p (x: i32, y: i32) -> i32 {
    if x >= 0 {
        if y >= 0 { x  / y }
        else { -(x  / -y) }
    }
    else {
        if y >= 0 { -((y-1-x) / y) }
        else { (-y-1-x) / -y }
    }
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