// Moving channel samples between the layouts DWA needs: the interleaved
// scanline buffer the rest of the crate uses, the per-channel byte runs, the
// planar byte planes of the UNKNOWN/RLE sections, and back again.

use half::f16;

use crate::{
    compression::ByteVec,
    error::{Error, Result},
    meta::attribute::{ChannelList, IntegerBounds, SampleType},
};

use super::{ChannelInfo, CompressorScheme};

pub(super) fn split_scanline_channels(
    data: &[u8],
    channels: &ChannelList,
    infos: &[ChannelInfo],
    rectangle: IntegerBounds,
) -> Result<Vec<Vec<u8>>> {
    // The scanline buffer is already in channel order, but the encoder needs
    // each channel sliced back out so the per-scheme packing matches the C
    // reference's running cursors.
    let mut per_channel: Vec<Vec<u8>> = infos
        .iter()
        .map(|info| Vec::with_capacity(info.width * info.height * info.bytes_per_sample))
        .collect();
    let mut input = data;

    for y in rectangle.position.y()..rectangle.end().y() {
        for (index, channel) in channels.list.iter().enumerate() {
            let sampling_y = channel.sampling.y().max(1) as i32;
            if y % sampling_y != 0 {
                continue;
            }

            let row_length = infos[index].width * infos[index].bytes_per_sample;
            if row_length > input.len() {
                return Err(Error::invalid("DWA input data truncated"));
            }
            let (row, rest) = input.split_at(row_length);
            per_channel[index].extend_from_slice(row);
            input = rest;
        }
    }

    if !input.is_empty() {
        return Err(Error::invalid("DWA input data size mismatch"));
    }

    Ok(per_channel)
}

pub(super) fn pack_unknown_channels(
    infos: &[ChannelInfo],
    channel_bytes: &[Vec<u8>],
    scheme: CompressorScheme,
) -> Vec<u8> {
    // UNKNOWN channels stay planar and are concatenated in channel order
    // before the zlib step.
    let total_len = infos
        .iter()
        .zip(channel_bytes)
        .filter(|(info, _)| info.scheme == scheme)
        .map(|(_, bytes)| bytes.len())
        .sum();
    let mut out = Vec::with_capacity(total_len);

    for (info, bytes) in infos.iter().zip(channel_bytes) {
        if info.scheme == scheme {
            out.extend_from_slice(bytes);
        }
    }
    out
}

pub(super) fn pack_rle_channels(infos: &[ChannelInfo], channel_bytes: &[Vec<u8>]) -> Vec<u8> {
    // RLE channels are repacked into byte planes first, then byte-RLE'd and
    // zlib-compressed by the caller.
    let total_len = infos
        .iter()
        .zip(channel_bytes)
        .filter(|(info, _)| info.scheme == CompressorScheme::Rle)
        .map(|(_, bytes)| bytes.len())
        .sum();
    let mut out = Vec::with_capacity(total_len);

    for (info, bytes) in infos.iter().zip(channel_bytes) {
        if info.scheme == CompressorScheme::Rle {
            out.extend_from_slice(&separate_byte_planes(bytes, info.bytes_per_sample));
        }
    }
    out
}

fn separate_byte_planes(interleaved: &[u8], bytes_per_sample: usize) -> Vec<u8> {
    let sample_count = interleaved.len() / bytes_per_sample;
    let mut planar = vec![0u8; interleaved.len()];

    for byte in 0..bytes_per_sample {
        for sample in 0..sample_count {
            planar[byte * sample_count + sample] = interleaved[sample * bytes_per_sample + byte];
        }
    }

    planar
}

pub(super) fn u16s_to_le_bytes(values: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 2);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// Split a planar buffer into one byte run per channel of the given scheme,
/// in channel order (mirrors "DwaCompressor_setupChannelData"s running
/// per-scheme cursor). Other schemes get an empty vec.
pub(super) fn split_planar_channels(
    infos: &[ChannelInfo],
    scheme: CompressorScheme,
    planar: &[u8],
) -> Result<Vec<Vec<u8>>> {
    let mut per_channel = vec![vec![]; infos.len()];
    let mut cursor = 0;

    for (channel_bytes, info) in per_channel.iter_mut().zip(infos) {
        if info.scheme != scheme {
            continue;
        }
        let length = info.width * info.height * info.bytes_per_sample;
        *channel_bytes = planar
            .get(cursor..cursor + length)
            .ok_or_else(|| Error::invalid("truncated DWA channel data"))?
            .to_vec();
        cursor += length;
    }
    Ok(per_channel)
}

/// Restore per-sample byte order from byte planes
pub(super) fn interleave_byte_planes(planar: &[u8], bytes_per_sample: usize) -> Vec<u8> {
    let sample_count = planar.len() / bytes_per_sample;
    let mut interleaved = vec![0u8; planar.len()];
    for sample in 0..sample_count {
        for byte in 0..bytes_per_sample {
            interleaved[sample * bytes_per_sample + byte] = planar[byte * sample_count + sample];
        }
    }
    interleaved
}

/// Interleave the per-channel decoded data into the scanline layout the
/// rest of exrs expects: rows of "y" ascending, channels in list order
/// within each row, samples little-endian.
pub(super) fn write_scanlines(
    channels: &ChannelList,
    infos: &[ChannelInfo],
    rectangle: IntegerBounds,
    lossy_samples: &[Vec<f16>],
    unknown_bytes: &[Vec<u8>],
    rle_bytes: &[Vec<u8>],
    expected_byte_size: usize,
) -> Result<ByteVec> {
    // Reassemble the per-channel decoded data into the scanline layout the
    // rest of the crate expects: rows in ascending y, channels in list order.
    let mut out = Vec::with_capacity(expected_byte_size);

    for y in rectangle.position.y()..rectangle.end().y() {
        for (index, channel) in channels.list.iter().enumerate() {
            let sampling_y = channel.sampling.y().max(1) as i32;
            if y % sampling_y != 0 {
                continue;
            }

            let info = &infos[index];
            let row = ((y - rectangle.position.y()) / sampling_y) as usize;

            match info.scheme {
                CompressorScheme::LossyDct => {
                    let row_samples = &lossy_samples[index][row * info.width..][..info.width];
                    match info.sample_type {
                        SampleType::F16 => {
                            for sample in row_samples {
                                out.extend_from_slice(&sample.to_bits().to_le_bytes());
                            }
                        }
                        SampleType::F32 => {
                            for sample in row_samples {
                                out.extend_from_slice(&sample.to_f32().to_le_bytes());
                            }
                        }
                        // rejected before decoding
                        SampleType::U32 => {
                            return Err(Error::unsupported(
                                "DWA lossy DCT compression of u32 channels",
                            ));
                        }
                    }
                }

                CompressorScheme::Unknown | CompressorScheme::Rle => {
                    let bytes = if info.scheme == CompressorScheme::Unknown {
                        &unknown_bytes[index]
                    } else {
                        &rle_bytes[index]
                    };
                    let row_length = info.width * info.bytes_per_sample;
                    out.extend_from_slice(&bytes[row * row_length..][..row_length]);
                }
            }
        }
    }

    if out.len() != expected_byte_size {
        return Err(Error::invalid("DWA decoded size mismatch"));
    }
    Ok(out)
}
