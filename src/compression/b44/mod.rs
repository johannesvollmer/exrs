mod table;

use crate::compression::{mod_p, ByteVec, Bytes};
use crate::error::usize_to_i32;
use crate::io::Data;
use crate::meta::attribute::ChannelList;
use crate::prelude::*;
use std::cmp::min;
use std::mem::size_of;
use table::{EXP_TABLE, LOG_TABLE};

const BLOCK_SAMPLE_COUNT: usize = 4;

// As B44 compression is only use on f16 channels, we can have a conste for this value.
const BLOCK_X_BYTE_COUNT: usize = BLOCK_SAMPLE_COUNT * size_of::<u16>();

#[inline]
fn convert_from_linear(s: &mut [u16; 16]) {
    for v in s {
        *v = EXP_TABLE[*v as usize];
    }
}

#[inline]
fn convert_to_linear(s: &mut [u16; 16]) {
    for v in s {
        *v = LOG_TABLE[*v as usize];
    }
}

#[inline]
fn shift_and_round(x: i32, shift: i32) -> i32 {
    let x = x << 1;
    let a = (1 << shift) - 1;
    let shift = shift + 1;
    let b = (x >> shift) & 1;
    (x + a + b) >> shift
}

/// Pack a block of 4 by 4 16-bit pixels (32 bytes) into either 14 or 3 bytes.
fn pack(s: [u16; 16], b: &mut [u8], optimize_flat_fields: bool, exact_max: bool) -> usize {

    let mut t = [0u16; 16];

    for i in 0..16 {
        if (s[i] & 0x7c00) == 0x7c00 {
            t[i] = 0x8000;
        } else if (s[i] & 0x8000) != 0 {
            t[i] = !s[i];
        } else {
            t[i] = s[i] | 0x8000;
        }
    }

    let t_max = t.iter().max().unwrap();

    // Compute a set of running differences, r[0] ... r[14]:
    // Find a shift value such that after rounding off the
    // rightmost bits and shifting all differences are between
    // -32 and +31.  Then bias the differences so that they
    // end up between 0 and 63.
    let mut shift = -1;
    let mut d = [0i32; 16];
    let mut r = [0i32; 15];
    let mut r_min: i32;
    let mut r_max: i32;

    const BIAS: i32 = 0x20;

    loop {
        shift += 1;

        // Compute absolute differences, d[0] ... d[15],
        // between t_max and t[0] ... t[15].
        //
        // Shift and round the absolute differences.
        d.iter_mut().zip(&t)
            .for_each(|(d_v, t_v)|
                *d_v = shift_and_round((t_max - t_v).into(), shift));

        // Convert d[0] .. d[15] into running differences
        r[0] = d[0] - d[4] + BIAS;
        r[1] = d[4] - d[8] + BIAS;
        r[2] = d[8] - d[12] + BIAS;

        r[3] = d[0] - d[1] + BIAS;
        r[4] = d[4] - d[5] + BIAS;
        r[5] = d[8] - d[9] + BIAS;
        r[6] = d[12] - d[13] + BIAS;

        r[7] = d[1] - d[2] + BIAS;
        r[8] = d[5] - d[6] + BIAS;
        r[9] = d[9] - d[10] + BIAS;
        r[10] = d[13] - d[14] + BIAS;

        r[11] = d[2] - d[3] + BIAS;
        r[12] = d[6] - d[7] + BIAS;
        r[13] = d[10] - d[11] + BIAS;
        r[14] = d[14] - d[15] + BIAS;

        r_min = r[0];
        r_max = r[0];

        r.iter().copied().for_each(|v| {
            if r_min > v {
                r_min = v;
            }

            if r_max < v {
                r_max = v;
            }
        });

        if !(r_min < 0 || r_max > 0x3f) {
            break;
        }
    }

    if r_min == BIAS && r_max == BIAS && optimize_flat_fields {
        // Special case - all pixels have the same value.
        // We encode this in 3 instead of 14 bytes by
        // storing the value 0xfc in the third output byte,
        // which cannot occur in the 14-byte encoding.
        b[0] = (t[0] >> 8) as u8;
        b[1] = t[0] as u8;
        b[2] = 0xfc;

        return 3;
    }

    if exact_max {
        // Adjust t[0] so that the pixel whose value is equal
        // to t_max gets represented as accurately as possible.
        t[0] = t_max - (d[0] << shift) as u16;
    }

    // Pack t[0], shift and r[0] ... r[14] into 14 bytes:
    b[0] = (t[0] >> 8) as u8;
    b[1] = t[0] as u8;

    b[2] = ((shift << 2) | (r[0] >> 4)) as u8;
    b[3] = ((r[0] << 4) | (r[1] >> 2)) as u8;
    b[4] = ((r[1] << 6) | r[2]) as u8;

    b[5] = ((r[3] << 2) | (r[4] >> 4)) as u8;
    b[6] = ((r[4] << 4) | (r[5] >> 2)) as u8;
    b[7] = ((r[5] << 6) | r[6]) as u8;

    b[8] = ((r[7] << 2) | (r[8] >> 4)) as u8;
    b[9] = ((r[8] << 4) | (r[9] >> 2)) as u8;
    b[10] = ((r[9] << 6) | r[10]) as u8;

    b[11] = ((r[11] << 2) | (r[12] >> 4)) as u8;
    b[12] = ((r[12] << 4) | (r[13] >> 2)) as u8;
    b[13] = ((r[13] << 6) | r[14]) as u8;

    return 14;
}

// Tiny macro to simply get block array value as a u32.
macro_rules! b32 {
    ($b:expr, $i:expr) => {
        $b[$i] as u32
    };
}

// 0011 1111
const SIX_BITS: u32 = 0x3f;

// Unpack a 14-byte block into 4 by 4 16-bit pixels.
fn unpack14(b: &[u8], s: &mut [u16; 16]) {
    debug_assert_eq!(b.len(), 14);
    debug_assert_ne!(b[2], 0xfc);

    s[0] = ((b32!(b, 0) << 8) | b32!(b, 1)) as u16;

    let shift = b32!(b, 2) >> 2;
    let bias = 0x20 << shift;

    s[4] = (s[0] as u32 + ((((b32!(b, 2) << 4) | (b32!(b, 3) >> 4)) & SIX_BITS) << shift) - bias) as u16;
    s[8] = (s[4] as u32 + ((((b32!(b, 3) << 2) | (b32!(b, 4) >> 6)) & SIX_BITS) << shift) - bias) as u16;
    s[12] = (s[8] as u32 + ((b32!(b, 4) & SIX_BITS) << shift) - bias) as u16;

    s[1] = (s[0] as u32 + ((b32!(b, 5) >> 2) << shift) - bias) as u16;
    s[5] = (s[4] as u32 + ((((b32!(b, 5) << 4) | (b32!(b, 6) >> 4)) & SIX_BITS) << shift) - bias) as u16;
    s[9] = (s[8] as u32 + ((((b32!(b, 6) << 2) | (b32!(b, 7) >> 6)) & SIX_BITS) << shift) - bias) as u16;
    s[13] = (s[12] as u32 + ((b32!(b, 7) & SIX_BITS) << shift) - bias) as u16;

    s[2] = (s[1] as u32 + ((b32!(b, 8) >> 2) << shift) - bias) as u16;
    s[6] = (s[5] as u32 + ((((b32!(b, 8) << 4) | (b32!(b, 9) >> 4)) & SIX_BITS) << shift)  - bias) as u16;
    s[10] = (s[9] as u32 + ((((b32!(b, 9) << 2) | (b32!(b, 10) >> 6)) & SIX_BITS) << shift) - bias) as u16;
    s[14] = (s[13] as u32 + ((b32!(b, 10) & SIX_BITS) << shift) - bias) as u16;

    s[3] = (s[2] as u32 + ((b32!(b, 11) >> 2) << shift) - bias) as u16;
    s[7] = (s[6] as u32 + ((((b32!(b, 11) << 4) | (b32!(b, 12) >> 4)) & SIX_BITS) << shift) - bias) as u16;
    s[11] = (s[10] as u32 + ((((b32!(b, 12) << 2) | (b32!(b, 13) >> 6)) & SIX_BITS) << shift) - bias) as u16;
    s[15] = (s[14] as u32 + ((b32!(b, 13) & SIX_BITS) << shift) - bias) as u16;

    for i in 0..16 {
        if (s[i] & 0x8000) != 0 {
            s[i] &= 0x7fff;
        } else {
            s[i] = !s[i];
        }
    }
}

// Unpack a 3-byte block `b` into 4 by 4 identical 16-bit pixels in `s` array.
fn unpack3(b: &[u8], s: &mut [u16; 16]) {
    debug_assert_eq!(b[2], 0xfc);

    // Get the 16-bit value from the block.
    let mut value = ((b32!(b, 0) << 8) | b32!(b, 1)) as u16;

    if (value & 0x8000) != 0 {
        value &= 0x7fff;
    } else {
        value = !value;
    }

    s.fill(value);  // All pixels have save value.
}

#[derive(Debug)]
struct ChannelData {
    tmp8_start_index: usize,
    tmp8_end_index: usize,

    resolution: Vec2<usize>,
    y_sampling: usize,
    type_: SampleType,
    quantize_linearly: bool,
    samples_per_pixel: usize,
}

#[inline]
fn memcpy_u16_to_u8(src: &[u16], dst: &mut [u8]) {
    src.iter()
        .zip(dst.chunks_exact_mut(2))
        .for_each(|(a, b)| b.copy_from_slice(&a.to_ne_bytes()));
}

#[inline]
fn memcpy_u8_to_u16(src: &[u8], dst: &mut [u16]) {
    dst.iter_mut()
        .zip(src.chunks_exact(2))
        .for_each(|(a, b)| *a = u16::from_ne_bytes([b[0], b[1]]));
}

#[inline]
fn cpy_u8(src: &[u16], src_i: usize, dst: &mut [u8], dst_i: usize, n: usize) {

    memcpy_u16_to_u8( &src[src_i..src_i+n], &mut dst[dst_i..dst_i+2*n]);
}

pub fn decompress(
    channels: &ChannelList,
    compressed: &ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    debug_assert_eq!(
        expected_byte_size,
        rectangle.size.area() * channels.bytes_per_pixel
    );
    debug_assert!(!channels.list.is_empty());

    if compressed.is_empty() {
        return Ok(Vec::new());
    }

    // Extract channel information needed for decompression.
    let mut channel_data: Vec<ChannelData> = Vec::with_capacity(channels.list.len());
    let mut tmp8_read_index = 0;

    for channel in channels.list.iter() {
        let channel = ChannelData {
            tmp8_start_index: tmp8_read_index,
            tmp8_end_index: tmp8_read_index,
            resolution: channel.subsampled_resolution(rectangle.size),
            y_sampling: channel.sampling.y(),
            type_: channel.sample_type,
            quantize_linearly: channel.quantize_linearly,
            samples_per_pixel: channel.sampling.area(),
        };

        tmp8_read_index += channel.resolution.area() * channel.samples_per_pixel * channel.type_.bytes_per_sample();
        channel_data.push(channel);
    }

    let mut in_i = 0usize;
    let mut remaining = compressed.len();

    let mut tmp_u8_buffer = Vec::with_capacity(expected_byte_size);

    println!("channel count {}", channel_data.len());

    for channel in &channel_data {

         println!("channel iter");

        debug_assert_eq!(remaining, compressed.len()-in_i);

        // Compute information for current channel.
        let sample_count = channel.resolution.area() * channel.samples_per_pixel;
        let byte_count = sample_count * channel.type_.bytes_per_sample();

        // Sample types that does not support B44 compression (u32 and f32) are raw copied.
        // In this branch, "compressed" array is actually raw, uncompressed data.
        if channel.type_ != SampleType::F16 {

            debug_assert_eq!(channel.type_.bytes_per_sample(), 4);

            println!("remaining {}", remaining);
            println!("byte_count {}", byte_count);

            if remaining < byte_count {
                return Err(Error::invalid("not enough data"));
            }

            tmp_u8_buffer.extend_from_slice(&compressed[in_i..(in_i+byte_count)]);

            in_i += byte_count;
            remaining -= byte_count;

            continue;
        }

        // HALF channel
        // The rest of the code assume we are manipulating u16 (2 bytes) values.
        debug_assert_eq!(channel.type_, SampleType::F16);
        debug_assert_eq!(channel.type_.bytes_per_sample(), size_of::<u16>());

        // Increase buffer to get new uncompressed datas.
        tmp_u8_buffer.resize( tmp_u8_buffer.len() + byte_count, 0);

        let x_sample_count = channel.resolution.x() * channel.samples_per_pixel;
        let y_sample_count = channel.resolution.y() * channel.samples_per_pixel;

        let bytes_per_sample = size_of::<u16>();

        let x_byte_count = x_sample_count * bytes_per_sample;
        let cd8_start = channel.tmp8_start_index;

        for y in (0..y_sample_count).step_by(BLOCK_SAMPLE_COUNT) {
            // Compute index in output (decompressed) buffer.
            let mut row8_0 = cd8_start + y * x_byte_count;
            let mut row8_1 = row8_0 + x_byte_count;
            let mut row8_2 = row8_1 + x_byte_count;
            let mut row8_3 = row8_2 + x_byte_count;

            // Move in pixel x line, 4 by 4.
            for x in (0..x_sample_count).step_by(BLOCK_SAMPLE_COUNT) {

                // Extract the 4 by 4 block of 16-bit floats from the compressed buffer.
                let mut s = [0u16; 16];

                if remaining < 3 {
                    return Err(Error::invalid("not enough data"));
                }

                // If shift exponent is 63, call unpack14 (ignoring unused bits)
                if compressed[in_i + 2] >= (13 << 2) {
                    if remaining < 3 {
                        return Err(Error::invalid("not enough data"));
                    }

                    unpack3(&compressed[in_i..(in_i + 3)], &mut s);

                    in_i += 3;
                    remaining -= 3;
                } else {
                    if remaining < 14 {
                        return Err(Error::invalid("not enough data"));
                    }

                    unpack14(&compressed[in_i..(in_i + 14)], &mut s);

                    in_i += 14;
                    remaining -= 14;
                }

                if channel.quantize_linearly {
                    convert_to_linear(&mut s);
                }

                // Get resting samples from the line to copy in temp buffer (without going outside channel).
                let x_resting_sample_count = match x + 3 < x_sample_count {
                    true => BLOCK_SAMPLE_COUNT,
                    false => (x_sample_count - x),
                };

                debug_assert!(x_resting_sample_count > 0);
                debug_assert!(x_resting_sample_count <= BLOCK_SAMPLE_COUNT);

                // Copy rows (without going outside channel).
                if y + 3 < y_sample_count {
                    cpy_u8(&s, 0, &mut tmp_u8_buffer, row8_0, x_resting_sample_count);
                    cpy_u8(&s, 4, &mut tmp_u8_buffer, row8_1, x_resting_sample_count);
                    cpy_u8(&s, 8, &mut tmp_u8_buffer, row8_2, x_resting_sample_count);
                    cpy_u8(&s, 12, &mut tmp_u8_buffer, row8_3, x_resting_sample_count);

                } else {
                    debug_assert!(y < y_sample_count);

                    cpy_u8(&s, 0, &mut tmp_u8_buffer, row8_0, x_resting_sample_count);

                    if y + 1 < y_sample_count {
                        cpy_u8(&s, 4, &mut tmp_u8_buffer, row8_1, x_resting_sample_count);
                    }

                    if y + 2 < y_sample_count {
                        cpy_u8(&s, 8, &mut tmp_u8_buffer, row8_2, x_resting_sample_count);
                    }
                }

                // Update row's array index to 4 next pixels.
                row8_0 += BLOCK_X_BYTE_COUNT;
                row8_1 += BLOCK_X_BYTE_COUNT;
                row8_2 += BLOCK_X_BYTE_COUNT;
                row8_3 += BLOCK_X_BYTE_COUNT;
            }
        }
    }

    debug_assert_eq!(tmp_u8_buffer.len(), expected_byte_size);

    println!("tmp_u8_buffer {}", tmp_u8_buffer.len());

    let mut out_reel = Vec::with_capacity(expected_byte_size);

    println!("rectangle {}", rectangle.end().y() - rectangle.position.y());

    let mut byte_count = 0;

    println!("y size {}", (rectangle.position.y()..rectangle.end().y()).len());  // 731

    println!("channels.uniform_sample_type == Some(SampleType::F16) {}",
             channels.uniform_sample_type == Some(SampleType::F16));

    for y in rectangle.position.y()..rectangle.end().y() {
        for channel in &mut channel_data {
            if mod_p(y, usize_to_i32(channel.y_sampling)) != 0 {
                continue;
            }

            let x_sample_count = channel.resolution.x() * channel.samples_per_pixel;
            let bytes_per_line = x_sample_count * channel.type_.bytes_per_sample();
            let next_tmp8_end_index = channel.tmp8_end_index + bytes_per_line;
            let range8 = channel.tmp8_end_index..next_tmp8_end_index;
            let values8 = &tmp_u8_buffer[range8];
            channel.tmp8_end_index = next_tmp8_end_index;

            // We can support uncompressed data in the machine's native format
            // if all image channels are of type HALF, and if the Xdr and the
            // native representations of a half have the same size.
            if channels.uniform_sample_type == Some(SampleType::F16) {
                // machine-dependent data format is a simple memcpy
                use lebe::io::WriteEndian;
                out_reel
                    .write_as_native_endian(values8)
                    .expect("write to in-memory failed");
            } else {
                u8::write_slice(&mut out_reel, values8).expect("write to in-memory failed");
            }

            byte_count += values8.len();
        }
    }

    println!("byte count outed {}", byte_count);

    for index in 1..channel_data.len() {
        debug_assert_eq!(
            channel_data[index - 1].tmp8_end_index,
            channel_data[index].tmp8_start_index
        );
    }

    debug_assert_eq!(out_reel.len(), expected_byte_size);

    Ok(out_reel)
}

pub fn compress(
    channels: &ChannelList,
    uncompressed: Bytes<'_>,
    rectangle: IntegerBounds,
    optimize_flat_fields: bool,
) -> Result<ByteVec> {
    if uncompressed.is_empty() {
        return Ok(Vec::new());
    }

    println!("compress()");
    println!("uncompressed.len() {}", uncompressed.len());

    let mut tmp8 = vec![0_u8; uncompressed.len()];

    let mut channel_data = Vec::new();

    let mut tmp8_end_index = 0;
    for channel in &channels.list {
        let number_samples = channel.subsampled_resolution(rectangle.size);

        let sample_count = channel.subsampled_resolution(rectangle.size).area();
        let byte_count8 = sample_count * channel.sample_type.bytes_per_sample();

        let channel = ChannelData {
            tmp8_start_index: tmp8_end_index,
            tmp8_end_index,
            y_sampling: channel.sampling.y(),
            resolution: number_samples,
            type_: channel.sample_type,
            quantize_linearly: channel.quantize_linearly,
            samples_per_pixel: channel.sampling.area(),
        };

        tmp8_end_index += byte_count8;
        channel_data.push(channel);
    }

    // debug_assert_eq!(tmp_end_index, tmp.len());
    debug_assert_eq!(tmp8_end_index, tmp8.len());

    let mut remaining_uncompressed_bytes = uncompressed;
    for y in rectangle.position.y()..rectangle.end().y() {
        for channel in &mut channel_data {
            if mod_p(y, usize_to_i32(channel.y_sampling)) != 0 {
                continue;
            }

            let x_sample_count = channel.resolution.x() * channel.samples_per_pixel;
            let bytes_per_line = x_sample_count * channel.type_.bytes_per_sample();
            let next_tmp8_end_index = channel.tmp8_end_index + bytes_per_line;
            let range8 = channel.tmp8_end_index..next_tmp8_end_index;
            let target8 = &mut tmp8[range8];
            channel.tmp8_end_index = next_tmp8_end_index;

            // We can support uncompressed data in the machine's native format
            // if all image channels are of type HALF, and if the Xdr and the
            // native representations of a half have the same size.
            if channels.uniform_sample_type == Some(SampleType::F16) {
                use lebe::io::ReadEndian;
                remaining_uncompressed_bytes
                    .read_from_native_endian_into(target8)
                    .expect("in-memory read failed");
            } else {
                u8::read_slice(&mut remaining_uncompressed_bytes, target8)
                    .expect("in-memory read failed");
            }
        }
    }

    // Generate a whole buffer that we will crop to proper size once compression is done.
    let mut b44_compressed = vec![0; uncompressed.len()];
    let mut b44_end = 0; // Buffer byte index for storing next compressed values.

    for channel in &channel_data {
        // U32 and F32 channels are raw copied.
        if channel.type_ != SampleType::F16 {

            debug_assert_eq!(channel.type_.bytes_per_sample(), 4);

            // Raw byte copy.
            let slice = &tmp8[channel.tmp8_start_index..channel.tmp8_end_index];
            slice.iter().copied().for_each(|b|{
                b44_compressed[b44_end] = b;
                b44_end += 1;
            });

            continue;
        }

        // HALF channel
        debug_assert_eq!(channel.type_, SampleType::F16);
        debug_assert_eq!(channel.type_.bytes_per_sample(), size_of::<u16>());

        let x_sample_count = channel.resolution.x() * channel.samples_per_pixel;
        let y_sample_count = channel.resolution.y() * channel.samples_per_pixel;

        let x_byte_count = x_sample_count * size_of::<u16>();
        let cd8_start = channel.tmp8_start_index;

        for y in (0..y_sample_count).step_by(BLOCK_SAMPLE_COUNT) {
            //
            // Copy the next 4x4 pixel block into array s.
            // If the width, cd.nx, or the height, cd.ny, of
            // the pixel data in _tmpBuffer is not divisible
            // by 4, then pad the data by repeating the
            // rightmost column and the bottom row.
            //

            // Compute row index in temp buffer.
            let mut row8_0 = cd8_start + y * x_byte_count;
            let mut row8_1 = row8_0 + x_byte_count;
            let mut row8_2 = row8_1 + x_byte_count;
            let mut row8_3 = row8_2 + x_byte_count;

            if y + 3 >= y_sample_count {
                if y + 1 >= y_sample_count {
                    row8_1 = row8_0;
                }

                if y + 2 >= y_sample_count {
                    row8_2 = row8_1;
                }

                row8_3 = row8_2;
            }

            for x in (0..x_sample_count).step_by(BLOCK_SAMPLE_COUNT) {
                let mut s = [0u16; 16];

                if x + 3 >= x_sample_count {
                    let n = x_sample_count - x;

                    for i in 0..BLOCK_SAMPLE_COUNT {
                        let j = min(i, n - 1);

                        s[i + 0] = u16::from_ne_bytes([tmp8[(row8_0 + j)], tmp8[(row8_0 + j + 1)]]);
                        s[i + 4] = u16::from_ne_bytes([tmp8[(row8_1 + j)], tmp8[(row8_1 + j + 1)]]);
                        s[i + 8] = u16::from_ne_bytes([tmp8[(row8_2 + j)], tmp8[(row8_2 + j + 1)]]);
                        s[i + 12] = u16::from_ne_bytes([tmp8[(row8_3 + j)], tmp8[(row8_3 + j + 1)]]);
                    }
                } else {
                    memcpy_u8_to_u16(&tmp8[row8_0..(row8_0+BLOCK_X_BYTE_COUNT)], &mut s[0..4]);
                    memcpy_u8_to_u16( &tmp8[row8_1..(row8_1+BLOCK_X_BYTE_COUNT)], &mut s[4..8]);
                    memcpy_u8_to_u16( &tmp8[row8_2..(row8_2+BLOCK_X_BYTE_COUNT)], &mut s[8..12]);
                    memcpy_u8_to_u16( &tmp8[row8_3..(row8_3+BLOCK_X_BYTE_COUNT)], &mut s[12..16]);
                }

                // Move to next block.
                row8_0 += BLOCK_X_BYTE_COUNT;
                row8_1 += BLOCK_X_BYTE_COUNT;
                row8_2 += BLOCK_X_BYTE_COUNT;
                row8_3 += BLOCK_X_BYTE_COUNT;

                // Compress the contents of array `s` and append the results to the output buffer.
                if channel.quantize_linearly {
                    convert_from_linear(&mut s);
                }

                b44_end += pack(
                    s,
                    &mut b44_compressed[b44_end..(b44_end + 14)],
                    optimize_flat_fields,
                    !channel.quantize_linearly,
                );
            }
        }
    }

    b44_compressed.resize(b44_end, 0);

    Ok(b44_compressed)
}

#[cfg(test)]
mod test {
    use crate::compression::b44;
    use crate::compression::b44::{convert_from_linear, convert_to_linear};
    use crate::compression::ByteVec;
    use crate::meta::attribute::*;
    use crate::prelude::f16;
    use crate::prelude::*;

    #[test]
    fn test_convert_from_to_linear() {
        // Create two identical arrays with random floats.
        let mut s1 = [0u16; 16];

        for i in 0..16 {
            s1[i] = f16::from_f32(rand::random::<f32>()).to_bits();
        }

        let s2 = s1.clone();

        // Apply two reversible conversion.
        convert_from_linear(&mut s1);
        convert_to_linear(&mut s1);

        // And check.
        for (u1, u2) in s1.iter().zip(&s2) {
            let f1 = f16::from_bits(*u1).to_f64();
            let f2 = f16::from_bits(*u2).to_f64();
            debug_assert!((f1 - f2).abs() < 0.01);
        }
    }

    fn test_roundtrip_noise_with(channels: ChannelList, rectangle: IntegerBounds) {

        let mut i = 0;
        for channel in &channels.list {

            let number_samples = channel.subsampled_resolution(rectangle.size);
            let byte_count = number_samples.area() * channel.sample_type.bytes_per_sample();

            i += byte_count;
        }
        println!("byte_count i {}", i);

        let pixel_bytes: ByteVec = (0..i)
            .map(|_| rand::random())
            .collect();

        debug_assert!(pixel_bytes.len() > 0);

        let compressed = b44::compress(&channels, &pixel_bytes, rectangle, true).unwrap();

        // Detect if all channels have same sample types.
        let same_sample_type = channels.list.iter()
            .all(|c| c.sample_type == channels.list[0].sample_type);

        // On my tests, B44 give a size of 44.08% the original data (this assert implies enough
        // pixels to be relevant).
        if same_sample_type {
            match channels.list[0].sample_type {
                SampleType::F16 => debug_assert!(compressed.len() as f64 <= pixel_bytes.len() as f64 * 0.445),
                SampleType::F32 | SampleType::U32 => debug_assert_eq!(compressed.len(), pixel_bytes.len()),
            }
        };

        println!("{:?}", channels.list[0].sample_type);
        println!("decompress");

        let decompressed =
            b44::decompress(&channels, &compressed, rectangle, pixel_bytes.len(), true).unwrap();

        debug_assert_eq!(decompressed.len(), pixel_bytes.len());
    }

    #[test]
    fn roundtrip_any_sample_type_f16() {
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
            // let rectangle = IntegerBounds {
            //     position: Vec2(-1, 3),
            //     size: Vec2(10, 10),
            // };

            test_roundtrip_noise_with(channels, rectangle);
    }

    #[test]
    fn roundtrip_any_sample_type_f32() {
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

            test_roundtrip_noise_with(channels, rectangle);
    }

    #[test]
    fn roundtrip_any_sample_type_u32() {
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

            test_roundtrip_noise_with(channels, rectangle);
    }

    #[test]
    fn roundtrip_any_sample_type_mix_f32_f16_u32() {

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

        test_roundtrip_noise_with(channels, rectangle);
    }

    #[test]
    fn roundtrip_any_sample_type_toto() {
        {
            let width = 512;
            let height = 512;

            let channels = SpecificChannels::rgba(|Vec2(x, y)| {
                (
                    // generate (or lookup in your own image) an f32 rgb color for each of the 2048x2048 pixels
                    x as f32 / 512.0,         // red
                    y as f32 / 512.0,         // green
                    1.0 - (y as f32 / 512.0), // blue
                    f16::from_f32(0.8),        // 16-bit alpha
                )
            });
            let mut image = Image::from_channels((width, height), channels);
            image.layer_data.encoding.compression = crate::compression::Compression::Uncompressed;
            image
                .write()
                .to_file("/home/narann/Desktop/minimal_rgb.exr")
                .unwrap();
        }
        {
            let width = 512;
            let height = 512;

            let channels = SpecificChannels::rgba(|Vec2(x, y)| {
                (
                    // generate (or lookup in your own image) an f32 rgb color for each of the 2048x2048 pixels
                    f16::from_f32(x as f32 / 512.0),         // red
                    f16::from_f32(y as f32 / 512.0),         // green
                    f16::from_f32(1.0 - (y as f32 / 512.0)), // blue
                    f16::from_f32(0.8),                       // 16-bit alpha
                )
            });
            let mut image = Image::from_channels((width, height), channels);
            image.layer_data.encoding.compression = crate::compression::Compression::B44;
            image
                .write()
                .to_file("/home/narann/Desktop/minimal_rgb_b44.exr")
                .unwrap();
        }
        {
            let mut image = crate::prelude::read()
                .no_deep_data()
                .largest_resolution_level()
                .all_channels()
                .all_layers()
                .all_attributes()
                .from_file("/home/narann/Desktop/minimal_rgb_b44.exr")
                .unwrap();
            for layer in &mut image.layer_data {
                layer.encoding.compression = crate::compression::Compression::Uncompressed;
                // let image = Image::from_layer(layer);
            }
            // image.layer_data.encoding.compression = crate::compression::Compression::Uncompressed;
            // let image = Image::from_layer(image.layer_data);
            // image.layer_data.encoding.compression = crate::compression::Compression::Uncompressed;
            image
                .write()
                .to_file("/home/narann/Desktop/minimal_rgb_b44_uncomp.exr")
                .unwrap();
        }
    }
}
