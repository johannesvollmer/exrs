mod table;

use crate::compression::{mod_p, ByteVec, Bytes};
use crate::error::usize_to_i32;
use crate::io::Data;
use crate::meta::attribute::ChannelList;
use crate::prelude::*;
use std::any::Any;
use std::cmp::min;
use std::mem::size_of;
use table::{EXP_TABLE, LOG_TABLE};

fn convert_from_linear(s: &mut [u16; 16]) {
    for i in 0..16 {
        s[i] = EXP_TABLE[s[i] as usize];
    }
}

fn convert_to_linear(s: &mut [u16; 16]) {
    for i in 0..16 {
        s[i] = LOG_TABLE[s[i] as usize];
    }
}

fn shift_and_round(x: i32, shift: i32) -> i32 {
    let x = x << 1;
    let a = (1 << shift) - 1;
    let shift = shift + 1;
    let b = (x >> shift) & 1;
    (x + a + b) >> shift
}

/// Pack a block of 4 by 4 16-bit pixels (32 bytes) into either 14 or 3 bytes.
fn pack(s: [u16; 16], b: &mut [u8], opt_flat_fields: bool, exact_max: bool) -> usize {
    // TODO: b slice should be &mut [u8; 14], but rust doesn't support sized slice.
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

    let mut t_max = 0u16; // Use *t.iter().max().unwrap()?

    for i in 0..16 {
        if t_max < t[i] {
            t_max = t[i];
        }
    }

    //
    // Compute a set of running differences, r[0] ... r[14]:
    // Find a shift value such that after rounding off the
    // rightmost bits and shifting all differenes are between
    // -32 and +31.  Then bias the differences so that they
    // end up between 0 and 63.
    //

    let mut shift = -1;
    let mut d = [0i32; 16];
    let mut r = [0i32; 15];
    let mut r_min: i32;
    let mut r_max: i32;

    const BIAS: i32 = 0x20;

    loop {
        shift += 1;

        //
        // Compute absolute differences, d[0] ... d[15],
        // between t_max and t[0] ... t[15].
        //
        // Shift and round the absolute differences.
        //

        for i in 0..16 {
            d[i] = shift_and_round((t_max - t[i]).into(), shift);
        }

        //
        // Convert d[0] .. d[15] into running differences
        //

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

        for i in 0..15 {
            if r_min > r[i] {
                r_min = r[i];
            }

            if r_max < r[i] {
                r_max = r[i];
            }
        }

        if !(r_min < 0 || r_max > 0x3f) {
            break;
        }
    }

    if r_min == BIAS && r_max == BIAS && opt_flat_fields {
        //
        // Special case - all pixels have the same value.
        // We encode this in 3 instead of 14 bytes by
        // storing the value 0xfc in the third output byte,
        // which cannot occur in the 14-byte encoding.
        //

        b[0] = (t[0] >> 8) as u8;
        b[1] = t[0] as u8;
        b[2] = 0xfc;

        return 3;
    }

    if exact_max {
        //
        // Adjust t[0] so that the pixel whose value is equal
        // to t_max gets represented as accurately as possible.
        //

        t[0] = t_max - (d[0] << shift) as u16;
    }

    //
    // Pack t[0], shift and r[0] ... r[14] into 14 bytes:
    //

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

fn b_u32(b: &[u8], i: usize) -> u32 {
    b[i] as u32
}

// 0011 1111
const SIX_BITS: u32 = 0x3f; // 0x3fu8

// Unpack a 14-byte block into 4 by 4 16-bit pixels.
fn unpack14(b: &[u8], s: &mut [u16; 16]) {
    // TODO: b slice should be &mut [u8; 14], but rust doesn't support sized slice.
    assert_eq!(b.len(), 14);
    assert_ne!(b[2], 0xfc);

    s[0] = ((b_u32(&b, 0) << 8) | b_u32(&b, 1)) as u16;

    let shift = (b_u32(&b, 2) >> 2);
    let bias = 0x20 << shift;

    s[4] = (s[0] as u32 + ((((b_u32(&b, 2) << 4) | (b_u32(&b, 3) >> 4)) & SIX_BITS) << shift)
        - bias) as u16;
    s[8] = (s[4] as u32 + ((((b_u32(&b, 3) << 2) | (b_u32(&b, 4) >> 6)) & SIX_BITS) << shift)
        - bias) as u16;
    s[12] = (s[8] as u32 + ((b_u32(&b, 4) & SIX_BITS) << shift) - bias) as u16;

    s[1] = (s[0] as u32 + ((b_u32(&b, 5) >> 2) << shift) - bias) as u16;
    s[5] = (s[4] as u32 + ((((b_u32(&b, 5) << 4) | (b_u32(&b, 6) >> 4)) & SIX_BITS) << shift)
        - bias) as u16;
    s[9] = (s[8] as u32 + ((((b_u32(&b, 6) << 2) | (b_u32(&b, 7) >> 6)) & SIX_BITS) << shift)
        - bias) as u16;
    s[13] = (s[12] as u32 + ((b_u32(&b, 7) & SIX_BITS) << shift) - bias) as u16;

    s[2] = (s[1] as u32 + ((b_u32(&b, 8) >> 2) << shift) - bias) as u16;
    s[6] = (s[5] as u32 + ((((b_u32(&b, 8) << 4) | (b_u32(&b, 9) >> 4)) & SIX_BITS) << shift)
        - bias) as u16;
    s[10] = (s[9] as u32 + ((((b_u32(&b, 9) << 2) | (b_u32(&b, 10) >> 6)) & SIX_BITS) << shift)
        - bias) as u16;
    s[14] = (s[13] as u32 + ((b_u32(&b, 10) & SIX_BITS) << shift) - bias) as u16;

    s[3] = (s[2] as u32 + ((b_u32(&b, 11) >> 2) << shift) - bias) as u16;
    s[7] = (s[6] as u32 + ((((b_u32(&b, 11) << 4) | (b_u32(&b, 12) >> 4)) & SIX_BITS) << shift)
        - bias) as u16;
    s[11] = (s[10] as u32 + ((((b_u32(&b, 12) << 2) | (b_u32(&b, 13) >> 6)) & SIX_BITS) << shift)
        - bias) as u16;
    s[15] = (s[14] as u32 + ((b_u32(&b, 13) & SIX_BITS) << shift) - bias) as u16;

    for i in 0..16 {
        if (s[i] & 0x8000) != 0 {
            s[i] &= 0x7fff;
        } else {
            s[i] = !s[i];
        }
    }
}

// Unpack a 3-byte block into 4 by 4 identical 16-bit pixels.
fn unpack3(b: &[u8], s: &mut [u16; 16]) {
    // TODO: b slice should be &mut [u8; 3], but rust doesn't support sized slice.
    assert_eq!(b[2], 0xfc);

    s[0] = (((b[0] as u32) << 8) | (b[1] as u32)) as u16;

    if (s[0] & 0x8000) != 0 {
        s[0] &= 0x7fff;
    } else {
        s[0] = !s[0];
    }

    for i in 0..16 {
        s[i] = s[0];
    }
}

#[derive(Debug)]
struct ChannelData {
    tmp_start_index: usize,
    tmp_end_index: usize,

    resolution: Vec2<usize>,
    y_sampling: usize,
    type_: SampleType,
    quantize_linearly: bool,
    samples_per_pixel: usize,
}

fn cpy_u16(src: &[u16], src_i: usize, dst: &mut [u16], dst_i: usize, n: usize) {
    // assert_eq!(src.len(), dst.len());

    // for i in 0..src.len() {
    for i in 0..n {
        dst[dst_i + i] = src[src_i + i];
    }
}

pub fn decompress(
    channels: &ChannelList,
    compressed: &ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    let expected_value_count = expected_byte_size / 2;
    debug_assert_eq!(
        expected_byte_size,
        rectangle.size.area() * channels.bytes_per_pixel
    );
    debug_assert_ne!(expected_value_count, 0);
    debug_assert!(!channels.list.is_empty());

    if compressed.is_empty() {
        return Ok(Vec::new());
    }

    let mut channel_data: Vec<ChannelData> = Vec::with_capacity(channels.list.len());
    let mut tmp_read_index = 0;

    for channel in channels.list.iter() {
        let channel = ChannelData {
            tmp_start_index: tmp_read_index,
            tmp_end_index: tmp_read_index,
            resolution: channel.subsampled_resolution(rectangle.size),
            y_sampling: channel.sampling.y(),
            type_: channel.sample_type,
            quantize_linearly: channel.quantize_linearly,
            samples_per_pixel: channel.sample_type.bytes_per_sample()
                / SampleType::F16.bytes_per_sample(),
        };

        tmp_read_index += channel.resolution.area() * channel.samples_per_pixel;
        channel_data.push(channel);
    }

    let mut in_i = 0usize;
    let mut remaining = compressed.len();

    let mut out = vec![0u8; expected_byte_size];
    let mut tmp_u16_buffer = Vec::<u16>::with_capacity(expected_byte_size / 2);
    let mut out_i = 0;

    debug_assert_eq!(tmp_read_index, expected_value_count);

    for channel in &mut channel_data {
        // Sample types that does not support B44 compression (u32 and f32) are raw copied.
        if channel.type_ != SampleType::F16 {
            // Compute byte count for current channel.
            let byte_count = channel.resolution.area()
                * channel.samples_per_pixel
                * channel.type_.bytes_per_sample();

            if remaining < byte_count {
                // notEnoughData();
                return Err(Error::invalid("not enough data"));
            }

            //memcpy (cd.start, inPtr, n);
            for i in channel.tmp_start_index..(channel.tmp_start_index + byte_count) {
                // TOTALLY WRONG
                out[out_i + i] = compressed[in_i + i];
            }

            // Raw copy bytes in u16 temp buffer.
            for i in (0..byte_count).step_by(2) {
                let v0 = compressed[in_i + i];
                let v1 = compressed[in_i + i + 1];
                tmp_u16_buffer.push(u16::from_be_bytes([v0, v1]))
            }

            out_i += byte_count;
            in_i += byte_count;
            remaining -= byte_count;

            assert_eq!(out.len(), out_i);

            continue;
        }

        assert_eq!(channel.type_, SampleType::F16);

        // Prepare buffer to get uncompressed datas.
        let sample_count = channel.resolution.area() * channel.samples_per_pixel;
        tmp_u16_buffer.resize(tmp_u16_buffer.len() + sample_count, 0);

        let _cd_nx = channel.resolution.x() * channel.samples_per_pixel;
        let cd_ny = channel.resolution.y() * channel.samples_per_pixel;
        let cd_start = channel.tmp_start_index;

        // HALF channel
        for y in (0..cd_ny).step_by(4) {
            // Compute index in out buffer.
            let mut row0 = cd_start + y * _cd_nx;
            let mut row1 = row0 + _cd_nx;
            let mut row2 = row1 + _cd_nx;
            let mut row3 = row2 + _cd_nx;

            for x in (0.._cd_nx).step_by(4) {
                let mut s = [0u16; 16];

                if remaining < 3 {
                    // notEnoughData();
                    return Err(Error::invalid("not enough data"));
                }

                // If shift exponent is 63, call unpack14 (ignoring unused bits)
                if compressed[in_i + 2] >= (13 << 2) {
                    unpack3(&compressed[in_i..(in_i + 3)], &mut s);
                    out_i += 3;
                    in_i += 3;
                    remaining -= 3;
                } else {
                    if remaining < 14 {
                        // notEnoughData();
                        return Err(Error::invalid("not enough data"));
                    }

                    unpack14(&compressed[in_i..(in_i + 14)], &mut s);
                    out_i += 14;
                    in_i += 14;
                    remaining -= 14;
                }

                if channel.quantize_linearly {
                    convert_to_linear(&mut s);
                }

                // Avoid to go outside the block (I guess?).
                let byte_count = match x + 3 < _cd_nx {
                    true => 4 * size_of::<u16>(),
                    false => (_cd_nx - x) * size_of::<u16>(),
                };

                // for _ in 0..byte_count {
                //     out.push(0);
                // }

                assert!(byte_count > 0);
                assert!(byte_count <= 8);
                assert_eq!(byte_count % 2, 0);
                // println!("byte_count {}", byte_count);
                // println!("row0 {}", row0);
                // println!("out {}", out.len());
                // println!("? {}", (row0 + byte_count) > out.len());
                let sample_count = byte_count / 2;

                if y + 3 < cd_ny {
                    // cpy_u16_to_u8(&s[0..4], &mut out[row0..(row0 + byte_count)], byte_count);
                    // cpy_u16_to_u8(&s[4..8], &mut out[row1..(row1 + byte_count)], byte_count);
                    // cpy_u16_to_u8(&s[8..12], &mut out[row2..(row2 + byte_count)], byte_count);
                    // cpy_u16_to_u8(&s[12..16], &mut out[row3..(row3 + byte_count)], byte_count);

                    // println!("byte_count {}", byte_count);
                    // println!("sample_count {}", sample_count);
                    // println!("row0 {}", row0);
                    cpy_u16(&s, 0, &mut tmp_u16_buffer, row0, sample_count);
                    cpy_u16(&s, 4, &mut tmp_u16_buffer, row1, sample_count);
                    cpy_u16(&s, 8, &mut tmp_u16_buffer, row2, sample_count);
                    cpy_u16(&s, 12, &mut tmp_u16_buffer, row3, sample_count);

                    // memcpy (row0, &s[ 0], n);
                    // memcpy (row1, &s[ 4], n);
                    // memcpy (row2, &s[ 8], n);
                    // memcpy (row3, &s[12], n);
                    // for i in 0..4 {
                    //     u16::write(s[i + 0], &mut out[(row0 + i)..(row0 + i + 1)])
                    //         .expect("write to in-memory failed");
                    //     u16::write(s[i + 4], &mut out[(row1 + i)..(row1 + i + 1)])
                    //         .expect("write to in-memory failed");
                    //     u16::write(s[i + 8], &mut out[(row2 + i)..(row2 + i + 1)])
                    //         .expect("write to in-memory failed");
                    //     u16::write(s[i + 12], &mut out[(row3 + i)..(row3 + i + 1)])
                    //         .expect("write to in-memory failed");
                    //     // out[(row0 + i)..(row0 + i + 1)] = s[i + 0].to_be_bytes();
                    //     // out[(row1 + i)..(row1 + i + 1)] = s[i + 4].to_be_bytes();
                    //     // out[(row2 + i)..(row2 + i + 1)] = s[i + 8].to_be_bytes();
                    //     // out[(row3 + i)..(row3 + i + 1)] = s[i + 12].to_be_bytes();
                    //     // out[row0 + i] = s[i + 0];
                    //     // out[row1 + i] = s[i + 4];
                    //     // out[row2 + i] = s[i + 8];
                    //     // out[row3 + i] = s[i + 12];
                    // }
                } else {
                    // memcpy (row0, &s[ 0], n);
                    // cpy_u16_to_u8(&s[0..4], &mut out[row0..(row0 + byte_count)], byte_count);
                    cpy_u16(&s, 0, &mut tmp_u16_buffer, row0, sample_count);

                    if y + 1 < cd_ny {
                        // memcpy (row1, &s[ 4], n);
                        // cpy_u16_to_u8(&s[4..8], &mut out[row1..(row1 + byte_count)], byte_count);
                        cpy_u16(&s, 4, &mut tmp_u16_buffer, row1, sample_count);
                    }

                    if y + 2 < cd_ny {
                        // memcpy (row2, &s[ 8], n);
                        // cpy_u16_to_u8(&s[8..12], &mut out[row2..(row2 + byte_count)], byte_count);
                        cpy_u16(&s, 8, &mut tmp_u16_buffer, row2, sample_count);
                    }
                }

                row0 += 4;
                row1 += 4;
                row2 += 4;
                row3 += 4;
            }
        }

        /*char *outEnd = _outBuffer;

        if (_format == XDR)
        {
        for (int y = minY; y <= maxY; ++y)
        {
            for (int i = 0; i < _numChans; ++i)
            {
            ChannelData &cd = _channelData[i];

            if (modp (y, cd.ys) != 0)
                continue;

            if (cd.type == HALF)
            {
                for (int x = cd.nx; x > 0; --x)
                {
                Xdr::write <CharPtrIO> (outEnd, *cd.end);
                ++cd.end;
                }
            }
            else
            {
                int n = cd.nx * cd.size;
                memcpy (outEnd, cd.end, n * sizeof (unsigned short));
                outEnd += n * sizeof (unsigned short);
                cd.end += n;
            }
            }
        }
        }
        else
        {
        for (int y = minY; y <= maxY; ++y)
        {
            for (int i = 0; i < _numChans; ++i)
            {
            ChannelData &cd = _channelData[i];

            #if defined (DEBUG)
                assert (cd.type == HALF);
            #endif

            if (modp (y, cd.ys) != 0)
                continue;

            int n = cd.nx * cd.size;
            memcpy (outEnd, cd.end, n * sizeof (unsigned short));
            outEnd += n * sizeof (unsigned short);
            cd.end += n;
            }
        }
        }

        #if defined (DEBUG)

        for (int i = 1; i < _numChans; ++i)
            assert (_channelData[i-1].end == _channelData[i].start);

        assert (_channelData[_numChans-1].end == tmpBufferEnd);

        #endif

        if (inSize > 0)
        tooMuchData();

        outPtr = _outBuffer;
        return static_cast<int>(outEnd - _outBuffer);*/
    }

    let mut out_reel = Vec::with_capacity(expected_byte_size);

    for y in rectangle.position.y()..rectangle.end().y() {
        for channel in &mut channel_data {
            if mod_p(y, usize_to_i32(channel.y_sampling)) != 0 {
                continue;
            }

            let u16s_per_line = channel.resolution.x() * channel.samples_per_pixel;
            let next_tmp_end_index = channel.tmp_end_index + u16s_per_line;
            let values = &tmp_u16_buffer[channel.tmp_end_index..next_tmp_end_index];
            channel.tmp_end_index = next_tmp_end_index;

            // We can support uncompressed data in the machine's native format
            // if all image channels are of type HALF, and if the Xdr and the
            // native representations of a half have the same size.
            if channels.uniform_sample_type == Some(SampleType::F16) {
                // machine-dependent data format is a simple memcpy
                use lebe::io::WriteEndian;
                out_reel
                    .write_as_native_endian(values)
                    .expect("write to in-memory failed");
            } else {
                u16::write_slice(&mut out_reel, values).expect("write to in-memory failed");
            }
        }
    }

    for index in 1..channel_data.len() {
        debug_assert_eq!(
            channel_data[index - 1].tmp_end_index,
            channel_data[index].tmp_start_index
        );
    }

    debug_assert_eq!(channel_data.last().unwrap().tmp_end_index * 2, out.len());
    debug_assert_eq!(out_reel.len(), expected_byte_size);

    // Ok(out)
    Ok(out_reel)
}

pub fn compress(
    channels: &ChannelList,
    uncompressed: Bytes<'_>,
    rectangle: IntegerBounds,
    opt_flat_fields: bool,
) -> Result<ByteVec> {
    if uncompressed.is_empty() {
        return Ok(Vec::new());
    }

    let mut tmp = vec![0_u16; uncompressed.len() / 2];

    let mut channel_data = Vec::new();

    let mut tmp_end_index = 0;
    for channel in &channels.list {
        let number_samples = channel.subsampled_resolution(rectangle.size);
        let byte_size = channel.sample_type.bytes_per_sample() / SampleType::F16.bytes_per_sample();
        let byte_count = byte_size * number_samples.area();

        let channel = ChannelData {
            tmp_end_index,
            tmp_start_index: tmp_end_index,
            y_sampling: channel.sampling.y(),
            resolution: number_samples,
            type_: channel.sample_type,
            quantize_linearly: channel.quantize_linearly,
            samples_per_pixel: byte_size,
        };

        tmp_end_index += byte_count;
        channel_data.push(channel);
    }

    debug_assert_eq!(tmp_end_index, tmp.len());

    // min_x = rectangle.position.x;
    // max_x = rectangle.position.x + rectangle.size.x;
    // min_y = rectangle.position.y;
    // max_y = rectangle.position.y + rectangle.size.y;

    let mut remaining_uncompressed_bytes = uncompressed;
    for y in rectangle.position.y()..rectangle.end().y() {
        for channel in &mut channel_data {
            if mod_p(y, usize_to_i32(channel.y_sampling)) != 0 {
                continue;
            }
            let u16s_per_line = channel.resolution.x() * channel.samples_per_pixel;
            let next_tmp_end_index = channel.tmp_end_index + u16s_per_line;
            let target = &mut tmp[channel.tmp_end_index..next_tmp_end_index];
            channel.tmp_end_index = next_tmp_end_index;

            // We can support uncompressed data in the machine's native format
            // if all image channels are of type HALF, and if the Xdr and the
            // native representations of a half have the same size.
            if channels.uniform_sample_type == Some(SampleType::F16) {
                use lebe::io::ReadEndian;
                remaining_uncompressed_bytes
                    .read_from_native_endian_into(target)
                    .expect("in-memory read failed");
            } else {
                u16::read_slice(&mut remaining_uncompressed_bytes, target)
                    .expect("in-memory read failed");
            }
        }
    }

    let mut b44_compressed = Vec::with_capacity(uncompressed.len());
    b44_compressed.resize(uncompressed.len(), 0);
    let mut b44_end = 0; // Buffer byte index for storing next compressed values.

    // println!("b44_compressed {}", b44_compressed.len());

    for channel in channel_data {
        // UINT or FLOAT channel.
        if channel.type_ != SampleType::F16 {
            // TODO: Raw copy.
            continue;
        }

        let _cd_nx = channel.resolution.x() * channel.samples_per_pixel;
        let cd_ny = channel.resolution.y() * channel.samples_per_pixel;
        let cd_start = channel.tmp_start_index;

        // HALF channel
        for y in (0..cd_ny).step_by(4) {
            //
            // Copy the next 4x4 pixel block into array s.
            // If the width, cd.nx, or the height, cd.ny, of
            // the pixel data in _tmpBuffer is not divisible
            // by 4, then pad the data by repeating the
            // rightmost column and the bottom row.
            //
            let cd_nx = channel.resolution.x() * channel.samples_per_pixel;

            // Compute index in temp buffer.
            let mut row0 = cd_start + y * cd_nx;
            let mut row1 = row0 + cd_nx;
            let mut row2 = row1 + cd_nx;
            let mut row3 = row2 + cd_nx;

            if y + 3 >= cd_ny {
                if y + 1 >= cd_ny {
                    row1 = row0;
                }

                if y + 2 >= cd_ny {
                    row2 = row1;
                }

                row3 = row2;
            }

            for x in (0..cd_nx).step_by(4) {
                let mut s = [0u16; 16];

                if x + 3 >= cd_nx {
                    let n = cd_nx - x;

                    for i in 0..4 {
                        let j = min(i, n - 1);

                        s[i + 0] = tmp[row0 + j];
                        s[i + 4] = tmp[row1 + j];
                        s[i + 8] = tmp[row2 + j];
                        s[i + 12] = tmp[row3 + j];
                    }
                } else {
                    // memcpy (&s[ 0], row0, 4 * sizeof (unsigned short));
                    // memcpy (&s[ 4], row1, 4 * sizeof (unsigned short));
                    // memcpy (&s[ 8], row2, 4 * sizeof (unsigned short));
                    // memcpy (&s[12], row3, 4 * sizeof (unsigned short));
                    s[0] = tmp[row0];
                    s[1] = tmp[row0 + 1];
                    s[2] = tmp[row0 + 2];
                    s[3] = tmp[row0 + 3];
                    s[4] = tmp[row1];
                    s[5] = tmp[row1 + 1];
                    s[6] = tmp[row1 + 2];
                    s[7] = tmp[row1 + 3];
                    s[8] = tmp[row2];
                    s[9] = tmp[row2 + 1];
                    s[10] = tmp[row2 + 2];
                    s[11] = tmp[row2 + 3];
                    s[12] = tmp[row3];
                    s[13] = tmp[row3 + 1];
                    s[14] = tmp[row3 + 2];
                    s[15] = tmp[row3 + 3];
                }

                // Move to next block.
                row0 += 4;
                row1 += 4;
                row2 += 4;
                row3 += 4;

                //
                // Compress the contents of array s and append the
                // results to the output buffer.
                //

                if channel.quantize_linearly {
                    convert_from_linear(&mut s);
                }

                // println!("{}..{}", b44_end, (b44_end + 14));

                b44_end += pack(
                    s,
                    &mut b44_compressed[b44_end..(b44_end + 14)],
                    opt_flat_fields,
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
            assert!((f1 - f2).abs() < 0.01);
        }
    }

    fn test_roundtrip_noise_with(channels: ChannelList, rectangle: IntegerBounds) {
        let pixel_bytes: ByteVec = (0..channels.bytes_per_pixel * rectangle.size.area())
            .map(|_| rand::random())
            .collect();

        assert!(pixel_bytes.len() > 0);

        let compressed = b44::compress(&channels, &pixel_bytes, rectangle, true).unwrap();

        assert!(compressed.len() <= pixel_bytes.len());

        // On my tests, B44 give a size of 44.08% the original data (this assert implies enough
        // pixels to be relevant).
        assert!(compressed.len() as f64 <= pixel_bytes.len() as f64 * 0.445);

        let decompressed =
            b44::decompress(&channels, &compressed, rectangle, pixel_bytes.len(), true).unwrap();

        assert_eq!(decompressed.len(), pixel_bytes.len());

        // for i in 0..pixel_bytes.len() {
        //     let f1 = f16::from_be_bytes([pixel_bytes[i], pixel_bytes[i + 1]]).to_f64();
        //     let f2 = f16::from_be_bytes([decompressed[i], decompressed[i + 1]]).to_f64();
        //     assert!((f1 - f2).abs() < 0.01);
        // }
    }

    #[test]
    fn roundtrip_any_sample_type() {
        // for &sample_type in &[SampleType::F16, SampleType::F32, SampleType::U32] {
        for &sample_type in &[SampleType::F16] {
            let channel = ChannelDescription {
                sample_type,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1, 1),
            };

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
    }

    #[test]
    fn roundtrip_any_sample_type_toto() {
        {
            let width = 2048;
            let height = 2048;

            let channels = SpecificChannels::rgba(|Vec2(x, y)| {
                (
                    // generate (or lookup in your own image) an f32 rgb color for each of the 2048x2048 pixels
                    x as f32 / 2048.0,         // red
                    y as f32 / 2048.0,         // green
                    1.0 - (y as f32 / 2048.0), // blue
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
            let width = 2048;
            let height = 2048;

            let channels = SpecificChannels::rgba(|Vec2(x, y)| {
                (
                    // generate (or lookup in your own image) an f32 rgb color for each of the 2048x2048 pixels
                    f16::from_f32(x as f32 / 2048.0),         // red
                    f16::from_f32(y as f32 / 2048.0),         // green
                    f16::from_f32(1.0 - (y as f32 / 2048.0)), // blue
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
