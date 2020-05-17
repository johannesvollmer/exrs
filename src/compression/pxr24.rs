
// see https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfPxr24Compressor.cpp


//! Lossy compression for F32 data, but lossless compression for U32 and F16 data.

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

// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot


pub fn compress(channels: &ChannelList, mut remaining_bytes: Bytes<'_>, area: IntRect) -> Result<ByteVec> {
    if remaining_bytes.is_empty() { return Ok(Vec::new()); }

    let mut raw = vec![0_u8; channels.bytes_per_pixel * area.size.area()];
    let mut write_index = 0;

    for y in area.position.1 .. area.end().1 {
        for channel in &channels.list {
            if mod_p(y, channel.sampling.1 as i32) != 0 { continue; }
            let sample_count_x = channel.subsampled_resolution(area.size).0; // numSamples(channel.sampling.0, area.size.0);

            let mut indices = [0_usize; 4];
            let mut previous_pixel: u32 = 0;

            match channel.sample_type {
                SampleType::F16 => {
                    indices[0] = write_index;
                    indices[1] = indices[0] + sample_count_x;
                    write_index = indices[1] + sample_count_x;

                    for _ in 0..sample_count_x {
                        let pixel = u16::read_from_native_endian(&mut remaining_bytes).unwrap() as u32;
                        let difference = pixel.wrapping_sub(previous_pixel);
                        previous_pixel = pixel;

                        raw[indices[0]] = (difference >> 8) as u8;
                        raw[indices[1]] = difference as u8;

                        indices[0] += 1;
                        indices[1] += 1;
                    }
                    // for (int j = 0; j < n; ++j)
                    // {
                    //     half pixel;
                    //
                    //     pixel = *(const half *) inPtr;
                    //     inPtr += sizeof (half);
                    //
                    //     unsigned int diff = pixel.bits() - previousPixel;
                    //     previousPixel = pixel.bits();
                    //
                    //     *(ptr[0]++) = diff >> 8;
                    //     *(ptr[1]++) = diff;
                    // }
                },

                SampleType::U32 => {
                    indices[0] = write_index;
                    indices[1] = indices[0] + sample_count_x;
                    indices[2] = indices[1] + sample_count_x;
                    indices[3] = indices[2] + sample_count_x;
                    write_index = indices[3] + sample_count_x;

                    for _ in 0..sample_count_x {
                        let pixel = u32::read_from_native_endian(&mut remaining_bytes).unwrap();
                        let difference = pixel.wrapping_sub(previous_pixel);
                        previous_pixel = pixel;

                        raw[indices[0]] = (difference >> 24) as u8;
                        raw[indices[1]] = (difference >> 16) as u8;
                        raw[indices[2]] = (difference >> 8) as u8;
                        raw[indices[3]] = difference as u8;

                        indices[0] += 1;
                        indices[1] += 1;
                        indices[2] += 1;
                        indices[3] += 1;
                    }
                },

                SampleType::F32 => {
                    indices[0] = write_index;
                    indices[1] = indices[0] + sample_count_x;
                    indices[2] = indices[1] + sample_count_x;
                    write_index = indices[2] + sample_count_x;

                    for _ in 0..sample_count_x {
                        let pixel = f32::read_from_native_endian(&mut remaining_bytes).unwrap();
                        let pixel = f32_to_f24(pixel);

                        let difference = pixel.wrapping_sub(previous_pixel);
                        previous_pixel = pixel;

                        raw[indices[0]] = (difference >> 16) as u8;
                        raw[indices[1]] = (difference >> 8) as u8;
                        raw[indices[2]] = difference as u8;

                        indices[0] += 1;
                        indices[1] += 1;
                        indices[2] += 1;
                    }
                },
            }
        }
    }

    let mut compressor = ZlibEncoder::new(
        Vec::with_capacity(raw.len()),
        deflate::Compression::Default
    );

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

            let sample_count_x = channel.subsampled_resolution(area.size).0; // numSamples(channel.sampling.0, area.size.0);

            let mut indices = [0_usize; 4];
            let mut pixel_accumulation: u32 = 0;

            match channel.sample_type {
                SampleType::F16 => {
                    indices[0] = read_index;
                    indices[1] = indices[0] + sample_count_x;
                    read_index = indices[1] + sample_count_x;
                    // ptr[0] = tmpBufferEnd;
                    // ptr[1] = ptr[0] + n;
                    // tmpBufferEnd = ptr[1] + n;

                    if read_index > raw.len() {
                        panic!("not enough data");
                        // return Err();

                        // if ( (uLongf)(tmpBufferEnd - _tmpBuffer) > tmpSize)
                        // notEnoughData();
                    }

                    for _ in 0..sample_count_x {

                        let difference: u32 = ((raw[indices[0]] as u32) << 8) | (raw[indices[1]] as u32);
                        indices[0] += 1;
                        indices[1] += 1;

                        pixel_accumulation += difference;

                        let value = pixel_accumulation as u16; // TODO like that??
                        write.extend_from_slice(&value.to_ne_bytes());
                    }
                    // for (int j = 0; j < n; ++j)
                    // {
                    //     unsigned int diff = (*(ptr[0]++) << 8) |
                    //     *(ptr[1]++);
                    //
                    //     pixel += diff;
                    //
                    //     half * hPtr = (half *) writePtr;
                    //     hPtr->setBits ((unsigned short) pixel);
                    //     writePtr += sizeof (half);
                    // }
                },

                SampleType::U32 => {
                    indices[0] = read_index;
                    indices[1] = indices[0] + sample_count_x;
                    indices[2] = indices[1] + sample_count_x;
                    indices[3] = indices[2] + sample_count_x;
                    read_index = indices[3] + sample_count_x;
                    // ptr[0] = tmpBufferEnd;
                    // ptr[1] = ptr[0] + n;
                    // ptr[2] = ptr[1] + n;
                    // ptr[3] = ptr[2] + n;
                    // tmpBufferEnd = ptr[3] + n;

                    if read_index > raw.len() {
                        panic!("not enough data");
                        // return Err();

                        // if ( (uLongf)(tmpBufferEnd - _tmpBuffer) > tmpSize)
                        // notEnoughData();
                    }

                    for _ in 0..sample_count_x {
                        let diff: u32 = ((raw[indices[0]] as u32) << 24)
                            | ((raw[indices[1]] as u32) << 16)
                            | ((raw[indices[2]] as u32) << 8)
                            | (raw[indices[3]] as u32); // TODO use from_le_bytes instead?

                        indices[0] += 1;
                        indices[1] += 1;
                        indices[2] += 1;
                        indices[3] += 1;

                        pixel_accumulation += diff;

                        write.extend_from_slice(&pixel_accumulation.to_ne_bytes());
                    }
                    // for (int j = 0; j < n; ++j) {
                    //     unsigned int diff = (*(ptr[0]++) << 24) |
                    //     (*(ptr[1]++) << 16) |
                    //     (*(ptr[2]++) <<  8) |
                    //     *(ptr[3]++);
                    //
                    //     pixel += diff;
                    //
                    //     char *pPtr = (char *) &pixel;
                    //
                    //     for (size_t k = 0; k < sizeof (pixel); ++k)
                    //     *writePtr++ = *pPtr++;
                    // }
                },

                SampleType::F32 => {
                    indices[0] = read_index;
                    indices[1] = indices[0] + sample_count_x;
                    indices[2] = indices[1] + sample_count_x;
                    read_index = indices[2] + sample_count_x;
                    // ptr[0] = tmpBufferEnd;
                    // ptr[1] = ptr[0] + n;
                    // ptr[2] = ptr[1] + n;
                    // tmpBufferEnd = ptr[2] + n;

                    if read_index > raw.len() {
                        panic!("not enough data");
                        // return Err();

                        // if ( (uLongf)(tmpBufferEnd - _tmpBuffer) > tmpSize)
                        // notEnoughData();
                    }

                    for _ in 0..sample_count_x {
                        let diff: u32 = ((raw[indices[0]] as u32) << 24)
                            | ((raw[indices[1]] as u32) << 16)
                            | ((raw[indices[2]] as u32) << 8); // TODO use from_le_bytes instead?

                        indices[0] += 1;
                        indices[1] += 1;
                        indices[2] += 1;

                        pixel_accumulation += diff;

                        write.extend_from_slice(&pixel_accumulation.to_ne_bytes());
                    }

                    // for (int j = 0; j < n; ++j){
                    //     unsigned int diff = (*(ptr[0]++) << 24) |
                    //     (*(ptr[1]++) << 16) |
                    //     (*(ptr[2]++) <<  8);
                    //     pixel += diff;
                    //
                    //     char *pPtr = (char *) &pixel;
                    //
                    //     for (size_t k = 0; k < sizeof (pixel); ++k)
                    //     *writePtr++ = *pPtr++;
                    // }
                }
            }
        }
    }

    if read_index != raw.len() {
        panic!("too much data");
        // return Err()
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