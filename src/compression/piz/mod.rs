

//! The PIZ compression method is a wavelet compression,
//! based on the PIZ image format, customized for OpenEXR.
// inspired by  https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfPizCompressor.cpp

mod huffman;
mod wavelet;

use crate::prelude::common::*;
use crate::io::Data;
use crate::meta::attribute::*;
use crate::compression::{ByteVec, Bytes, mod_p};


const U16_RANGE: usize = (1_i32 << 16_i32) as usize;
const BITMAP_SIZE: usize  = (U16_RANGE as i32 >> 3_i32) as usize;

#[derive(Debug)]
struct ChannelData {
    tmp_start_index: usize,
    tmp_end_index: usize,

    resolution: Vec2<usize>,
    y_sampling: usize,
    samples_per_pixel: usize,
}


pub fn decompress(
    channels: &ChannelList,
    compressed: ByteVec,
    rectangle: IntRect,
    expected_byte_size: usize,
    pedantic: bool
) -> Result<ByteVec>
{
    let expected_value_count = expected_byte_size / 2;
    debug_assert_eq!(expected_byte_size, rectangle.size.area() * channels.bytes_per_pixel);
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
            y_sampling: channel.sampling.y(),
            resolution: channel.subsampled_resolution(rectangle.size),
            samples_per_pixel: channel.sample_type.bytes_per_sample() / SampleType::F16.bytes_per_sample()
        };

        tmp_read_index += channel.resolution.area() * channel.samples_per_pixel;
        channel_data.push(channel);
    }

    debug_assert_eq!(tmp_read_index, expected_value_count);

    let mut bitmap = vec![0_u8; BITMAP_SIZE]; // FIXME use bit_vec!

    let mut remaining_input = compressed.as_slice();
    let min_non_zero = u16::read(&mut remaining_input)? as usize;
    let max_non_zero = u16::read(&mut remaining_input)? as usize;

    if max_non_zero >= BITMAP_SIZE || min_non_zero >= BITMAP_SIZE {
        return Err(Error::invalid("compression data"));
    }

    if min_non_zero <= max_non_zero {
        u8::read_slice(&mut remaining_input, &mut bitmap[min_non_zero ..= max_non_zero])?;
    }

    let (lookup_table, max_value) = reverse_lookup_table_from_bitmap(&bitmap);

    {
        let length = i32::read(&mut remaining_input)?;
        if pedantic && length as usize != remaining_input.len() {
            // TODO length might be smaller than remaining??
            return Err(Error::invalid("compression data"));
        }
    }

    let mut tmp_u16_buffer = huffman::decompress(remaining_input, expected_value_count)?;

    for channel in &channel_data {
        let u16_count = channel.resolution.area() * channel.samples_per_pixel;
        let u16s = &mut tmp_u16_buffer[channel.tmp_start_index .. channel.tmp_start_index + u16_count];

        for offset in 0..channel.samples_per_pixel { // if channel is 32 bit, compress interleaved as two 16 bit values
            wavelet::decode(
                &mut u16s[offset..],
                channel.resolution,
                Vec2(channel.samples_per_pixel, channel.resolution.x() * channel.samples_per_pixel),
                max_value
            )?;
        }
    }

    // Expand the pixel data to their original range
    apply_lookup_table(&mut tmp_u16_buffer, &lookup_table);

    // let out_buffer_size = (max_scan_line_size * scan_line_count) + 65536 + 8192; // TODO not use expected byte size?
    let mut out = Vec::with_capacity(expected_byte_size);

    for y in rectangle.position.y() .. rectangle.end().y() {
        for channel in &mut channel_data {
            if mod_p(y, channel.y_sampling as i32) != 0 {
                continue;
            }

            let u16s_per_line = channel.resolution.x() * channel.samples_per_pixel;
            let next_tmp_end_index = channel.tmp_end_index + u16s_per_line;
            let values = &tmp_u16_buffer[channel.tmp_end_index .. next_tmp_end_index];
            channel.tmp_end_index = next_tmp_end_index;

            // We can support uncompressed data in the machine's native format
            // if all image channels are of type HALF, and if the Xdr and the
            // native representations of a half have the same size.
            if channels.uniform_sample_type == Some(SampleType::F16) { // machine-dependent data format is a simple memcpy
                use lebe::io::WriteEndian;
                out.write_as_native_endian(values).expect("write to in-memory failed");
            }
            else {
                u16::write_slice(&mut out, values).expect("write to in-memory failed");
            }
        }
    }

    for index in 1..channel_data.len() {
        debug_assert_eq!(channel_data[index - 1].tmp_end_index, channel_data[index].tmp_start_index);
    }

    debug_assert_eq!(channel_data.last().unwrap().tmp_end_index, tmp_u16_buffer.len());
    debug_assert_eq!(out.len(), expected_byte_size);

    Ok(out)
}



pub fn compress(
    channels: &ChannelList,
    uncompressed: Bytes<'_>,
    rectangle: IntRect
) -> Result<ByteVec>
{
    if uncompressed.is_empty() {
        return Ok(Vec::new());
    }

    let mut tmp = vec![0_u16; uncompressed.len() / 2 ];
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
            samples_per_pixel: byte_size,
        };

        tmp_end_index += byte_count;
        channel_data.push(channel);
    }

    debug_assert_eq!(tmp_end_index, tmp.len());

    let mut remaining_uncompressed_bytes = uncompressed;
    for y in rectangle.position.y() .. rectangle.end().y() {
        for channel in &mut channel_data {
            if mod_p(y, channel.y_sampling as i32) != 0 { continue; }
            let u16s_per_line = channel.resolution.x() * channel.samples_per_pixel;
            let next_tmp_end_index = channel.tmp_end_index + u16s_per_line;
            let target = &mut tmp[channel.tmp_end_index .. next_tmp_end_index];
            channel.tmp_end_index = next_tmp_end_index;

            // We can support uncompressed data in the machine's native format
            // if all image channels are of type HALF, and if the Xdr and the
            // native representations of a half have the same size.
            if channels.uniform_sample_type == Some(SampleType::F16) {
                use lebe::io::ReadEndian;
                remaining_uncompressed_bytes.read_from_native_endian_into(target).expect("in-memory read failed");
            }
            else {
                u16::read_slice(&mut remaining_uncompressed_bytes, target).expect("in-memory read failed");
            }
        }
    }


    let (min_non_zero, max_non_zero, bitmap) = bitmap_from_data(&tmp);
    let (max_value, table) = forward_lookup_table_from_bitmap(&bitmap);
    apply_lookup_table(&mut tmp, &table);

    let mut piz_compressed = Vec::with_capacity(uncompressed.len() / 3);
    (min_non_zero as u16).write(&mut piz_compressed)?;
    (max_non_zero as u16).write(&mut piz_compressed)?;

    if min_non_zero <= max_non_zero {
        piz_compressed.extend_from_slice(&bitmap[min_non_zero ..= max_non_zero]);
    }

    for channel in channel_data {
        for offset in 0 .. channel.samples_per_pixel {
            wavelet::encode(
                &mut tmp[channel.tmp_start_index + offset .. channel.tmp_end_index],
                channel.resolution,
                Vec2(channel.samples_per_pixel, channel.resolution.x() * channel.samples_per_pixel),
                max_value
            )?;
        }
    }

    let huffman_compressed: Vec<u8> = huffman::compress(&tmp)?;

    (huffman_compressed.len() as i32).write(&mut piz_compressed).expect("in-memory write failed");
    piz_compressed.extend_from_slice(&huffman_compressed);

    Ok(piz_compressed)
}


pub fn bitmap_from_data(data: &[u16]) -> (usize, usize, Vec<u8>) {
    let mut bitmap = vec![0_u8; BITMAP_SIZE];

    for value in data {
        bitmap[*value as usize >> 3] |= 1 << (*value as u8 & 7);
    }

    bitmap[0] = bitmap[0] & !1; // zero is not explicitly stored in the bitmap; we assume that the data always contain zeroes

    let min_index = bitmap.iter().position(|&value| value != 0);
    let max_index = min_index.map(|min|  // only if min was found
        min + bitmap[min..].iter().rposition(|&value| value != 0).expect("[min] not found")
    );

    (min_index.unwrap_or(0), max_index.unwrap_or(0), bitmap)
}

pub fn forward_lookup_table_from_bitmap(bitmap: &[u8]) -> (u16, Vec<u16>) {
    debug_assert_eq!(bitmap.len(), BITMAP_SIZE);

    let mut table = vec![0_u16; U16_RANGE];
    let mut count = 0_usize;

    for (index, entry) in table.iter_mut().enumerate() {
        if index == 0 || bitmap[index >> 3] as usize & (1 << (index & 7)) != 0 {
            *entry = count as u16;
            count += 1;
        }
    }

    ((count - 1) as u16, table)
}

fn reverse_lookup_table_from_bitmap(bitmap: Bytes<'_>) -> (Vec<u16>, u16) {
    let mut table = Vec::with_capacity(U16_RANGE);

    for index in 0 .. U16_RANGE {
        if index == 0 || ((bitmap[index >> 3] as usize & (1 << (index & 7))) != 0) {
            table.push(index as u16);
        }
    }

    debug_assert!(!table.is_empty());
    let max_value = (table.len() - 1) as u16;

    // fill remaining up to u16 range
    assert!(table.len() <= U16_RANGE);
    table.resize(U16_RANGE, 0);

    (table, max_value)
}

fn apply_lookup_table(data: &mut [u16], table: &[u16]) {
    for data in data {
        *data = table[*data as usize];
    }
}

#[cfg(test)]
mod test {
    use crate::prelude::common::*;
    use crate::compression::ByteVec;
    use crate::compression::piz;
    use crate::meta::attribute::*;

    fn test_roundtrip_noise_with(channels: ChannelList, rectangle: IntRect){
        let pixel_bytes: ByteVec = (0 .. channels.bytes_per_pixel * rectangle.size.area())
            .map(|_| rand::random()).collect();

        let compressed = piz::compress(&channels, &pixel_bytes, rectangle).unwrap();
        let decompressed = piz::decompress(&channels, compressed, rectangle, pixel_bytes.len(), true).unwrap();

        assert_eq!(pixel_bytes, decompressed);
    }


    #[test]
    fn roundtrip_any_sample_type(){
        for &sample_type in &[SampleType::F16, SampleType::F32, SampleType::U32] {
            let channel = ChannelInfo {
                sample_type,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            };

            let channels = ChannelList::new(smallvec![ channel.clone(), channel ]);

            let rectangle = IntRect {
                position: Vec2(-30, 100),
                size: Vec2(322, 731),
            };

            test_roundtrip_noise_with(channels, rectangle);
        }
    }

    #[test]
    fn roundtrip_two_channels(){
        let channel = ChannelInfo {
            sample_type: SampleType::F16,

            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1,1)
        };

        let channel2 = ChannelInfo {
            sample_type: SampleType::F32,

            name: Default::default(),
            quantize_linearly: false,
            sampling: Vec2(1,1)
        };

        let channels = ChannelList::new(smallvec![ channel, channel2 ]);

        let rectangle = IntRect {
            position: Vec2(-3, 1),
            size: Vec2(2323, 3132),
        };

        test_roundtrip_noise_with(channels, rectangle);
    }



    #[test]
    fn roundtrip_seven_channels(){
        let channels = ChannelList::new(smallvec![
            ChannelInfo {
                sample_type: SampleType::F32,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            },

            ChannelInfo {
                sample_type: SampleType::F32,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            },

            ChannelInfo {
                sample_type: SampleType::F32,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            },

            ChannelInfo {
                sample_type: SampleType::F16,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            },

            ChannelInfo {
                sample_type: SampleType::F32,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            },

            ChannelInfo {
                sample_type: SampleType::F32,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            },

            ChannelInfo {
                sample_type: SampleType::U32,

                name: Default::default(),
                quantize_linearly: false,
                sampling: Vec2(1,1)
            },
        ]);

        let rectangle = IntRect {
            position: Vec2(-3, 1),
            size: Vec2(2323, 3132),
        };

        test_roundtrip_noise_with(channels, rectangle);
    }

}