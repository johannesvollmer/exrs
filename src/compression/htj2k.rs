//! Lossless High-Throughput JPEG 2000 (HTJ2K, ITU-T T.814 | ISO/IEC
//! 15444-15), as OpenEXR writes it for `HTJ2K32` and `HTJ2K256`.
//!
//! Each chunk holds one HTJ2K codestream, which the `htj2k` crate decodes,
//! behind a short header that maps the codestream's components to the
//! channels of the chunk. This follows `ht_undo_impl` in OpenEXR's
//! `internal_ht.cpp` and `read_header` in `internal_ht_common.cpp`.
//!
//! The chunk header, all fields big-endian:
//!
//! ```text
//! u16  magic                      "HT"
//! u32  payload_size               bytes after this field, up to the codestream
//! u16  channel_count
//! u16  file_channel[channel_count]  channel index of each component
//! ..   (rest of the payload, ignored)
//! ..   codestream, from 6 + payload_size to the end of the chunk
//! ```
//!
//! OpenEXR codes HALF channels as signed 16-bit components and FLOAT and
//! UINT channels as 32-bit components, with NLT type 3 on the float ones so
//! that their bit patterns code as integers. The decoder undoes all of that,
//! so each sample comes out as the bits of the channel's sample type.
//!
//! Only decoding is implemented.

use super::*;
use crate::math::Vec2;

/// "HT", the magic number of `HTJ2K32` and `HTJ2K256` chunks. OpenEXR also
/// accepts "HL" for its lossy `LJ2K` compression, which exrs does not read.
const MAGIC: u16 = u16::from_be_bytes(*b"HT");

/// Bytes of the chunk header before its payload: the magic number and the
/// payload size.
const PREFIX_SIZE: usize = 6;

/// Bounds what the codestream decoder may allocate for one channel, besides
/// [`MEMORY_PER_CHANNEL_COLUMN`] for each pixel of the chunk's width.
///
/// For each component the decoder holds one row of code blocks per
/// subband, each sized to the tallest row of blocks in its subband, plus a
/// few lines per resolution. So it grows with the chunk width and with the
/// smaller of the chunk height and the nominal block height. With 64-bit
/// samples (FLOAT and UINT channels) and 5 decomposition levels, a 32-line
/// chunk needs about 350 bytes per pixel of width whatever the block size,
/// and a 256-line chunk about 0.8 KiB with OpenEXR's 128x32 blocks and up to
/// 2.2 KiB with blocks 256 high. 32-bit samples (HALF) need about half.
///
/// The limit covers every block size for the chunk heights exrs and OpenEXR
/// use, while stopping a damaged codestream from claiming far more memory
/// than the chunk could need. Only blocks 256 or more high in chunks of 512
/// lines or more, which no OpenEXR compression method produces, may be
/// refused in wide chunks.
const MEMORY_PER_CHANNEL: u64 = 4 << 20;

/// See [`MEMORY_PER_CHANNEL`].
const MEMORY_PER_CHANNEL_COLUMN: u64 = 4 << 10;

/// Decompresses one chunk into a new buffer of native-endian samples, in the
/// usual layout: line by line, and within each line the channels in order.
pub fn decompress(
    channels: &ChannelList,
    compressed: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    let mut decompressed = vec![0_u8; expected_byte_size];
    decompress_into(channels, &compressed, rectangle, &mut decompressed)?;
    Ok(decompressed)
}

/// Decompresses one chunk into `decompressed`, which must be exactly the
/// size of the chunk's samples. See [`decompress`].
fn decompress_into(
    channels: &ChannelList,
    compressed: &[u8],
    rectangle: IntegerBounds,
    decompressed: &mut [u8],
) -> UnitResult {
    let channel_count = channels.list.len();
    let width = rectangle.size.width();
    let height = rectangle.size.height();

    let (file_channels, codestream) = read_chunk_header(compressed, channel_count)?;

    let (layout, byte_size) = chunk_layout(channels, rectangle)?;
    if byte_size != decompressed.len() {
        return Err(Error::invalid("decompressed chunk size"));
    }
    if decompressed.is_empty() {
        return Ok(());
    }

    let mut decoder = ::htj2k::codestream::Codestream::new();
    let memory_limit = (width as u64)
        .saturating_mul(MEMORY_PER_CHANNEL_COLUMN)
        .saturating_add(MEMORY_PER_CHANNEL)
        .saturating_mul(channel_count as u64);
    decoder.set_memory_limit(memory_limit);
    decoder.read_headers(codestream).map_err(codestream_error)?;

    // The codestream must be exactly the chunk: OpenEXR pads only lossy
    // codestreams, which these compression methods never are.
    let siz = decoder.access_siz();
    let extent = siz.get_image_extent();
    let offset = siz.get_image_offset();
    let image_width = extent.x.checked_sub(offset.x);
    let image_height = extent.y.checked_sub(offset.y);
    if image_width.map(|w| w as usize) != Some(width)
        || image_height.map(|h| h as usize) != Some(height)
    {
        return Err(Error::invalid("HTJ2K image size does not match the chunk"));
    }
    if siz.get_num_components() as usize != channel_count {
        return Err(Error::invalid("HTJ2K component count does not match the channels"));
    }
    // Each component must have the sampling and the size of its channel.
    // OpenEXR codes a chunk at image offset 0, so a component has
    // ceil(size / sampling) samples each way, which is the size of the
    // channel when the chunk starts on a multiple of its sampling.
    for (component, &file_channel) in (0_u32..).zip(&file_channels) {
        let sampling = siz.get_downsampling(component);
        let channel_layout = &layout[file_channel];
        if Vec2(sampling.x as usize, sampling.y as usize) != channels.list[file_channel].sampling
            || siz.get_recon_width(component) as usize != channel_layout.width
            || siz.get_recon_height(component) as usize != channel_layout.lines.len()
        {
            return Err(Error::invalid("HTJ2K component size does not match the chunk"));
        }
    }

    // OpenEXR codes subsampled channels one after the other, and otherwise
    // one line of each component in turn, which is one line of the chunk.
    let planar = is_planar(channels);
    decoder.set_planar(planar);
    decoder.create().map_err(codestream_error)?;

    let mut pull_line = |component: usize, line: usize| -> UnitResult {
        let (pulled, samples) = decoder
            .pull()
            .map_err(codestream_error)?
            .ok_or_else(|| Error::invalid("HTJ2K codestream ended early"))?;
        if pulled as usize != component || !samples.is_i32() {
            return Err(Error::invalid("HTJ2K line order"));
        }

        let file_channel = file_channels[component];
        let channel_layout = &layout[file_channel];
        let samples = samples
            .i32()
            .get(..channel_layout.width)
            .ok_or_else(|| Error::invalid("HTJ2K line size"))?;

        let sample_type = channels.list[file_channel].sample_type;
        let start = channel_layout.lines[line];
        let size = sample_type.bytes_per_sample() * channel_layout.width;
        write_samples(&mut decompressed[start..start + size], samples, sample_type);
        Ok(())
    };

    if planar {
        for (component, &file_channel) in file_channels.iter().enumerate() {
            for line in 0..layout[file_channel].lines.len() {
                pull_line(component, line)?;
            }
        }
    } else {
        for line in 0..height {
            for component in 0..channel_count {
                pull_line(component, line)?;
            }
        }
    }

    Ok(())
}

/// Where the samples of one channel are in an uncompressed chunk.
struct ChannelLayout {
    /// Samples per line.
    width: usize,

    /// The byte offset of each line of the channel, top to bottom.
    lines: Vec<usize>,
}

/// Where the samples of each channel are in an uncompressed chunk, and the
/// size of the chunk in bytes. The chunk holds its lines top to bottom, and
/// each line the channels in order; a channel subsampled vertically has a
/// line only where y is a multiple of its sampling.
fn chunk_layout(
    channels: &ChannelList,
    rectangle: IntegerBounds,
) -> Result<(Vec<ChannelLayout>, usize)> {
    let mut layout = Vec::with_capacity(channels.list.len());
    for channel in &channels.list {
        let sampling = channel.sampling;
        if sampling.x() == 0
            || sampling.y() == 0
            || rectangle.size.width() % sampling.x() != 0
            || mod_p(rectangle.position.x(), usize_to_i32(sampling.x(), "sampling")?) != 0
        {
            return Err(Error::invalid("channel sampling does not divide the chunk"));
        }
        layout.push(ChannelLayout {
            width: rectangle.size.width() / sampling.x(),
            lines: Vec::new(),
        });
    }

    let mut byte_size = 0_usize;
    for y in rectangle.position.y()..rectangle.end().y() {
        for (channel, channel_layout) in channels.list.iter().zip(&mut layout) {
            if mod_p(y, usize_to_i32(channel.sampling.y(), "sampling")?) != 0 {
                continue;
            }
            channel_layout.lines.push(byte_size);
            byte_size = channel
                .sample_type
                .bytes_per_sample()
                .checked_mul(channel_layout.width)
                .and_then(|size| byte_size.checked_add(size))
                .ok_or_else(|| Error::invalid("chunk size"))?;
        }
    }

    Ok((layout, byte_size))
}

/// Whether `OpenEXR` codes the components one after the other, which it does
/// when any channel is subsampled.
fn is_planar(channels: &ChannelList) -> bool {
    channels.list.iter().any(|channel| channel.sampling != Vec2(1, 1))
}

/// Reads the chunk header. Returns the file channel of each codestream
/// component and the codestream.
fn read_chunk_header(compressed: &[u8], channel_count: usize) -> Result<(Vec<usize>, &[u8])> {
    let mut remaining = compressed;
    let mut take = |count: usize| {
        if remaining.len() < count {
            return Err(Error::invalid("HTJ2K chunk header"));
        }
        let (taken, rest) = remaining.split_at(count);
        remaining = rest;
        Ok(taken)
    };
    let read_u16 = |bytes: &[u8]| u16::from_be_bytes([bytes[0], bytes[1]]);

    if read_u16(take(2)?) != MAGIC {
        return Err(Error::invalid("HTJ2K chunk header magic number"));
    }
    let payload = take(4)?;
    let payload_size = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let codestream_start = (payload_size as usize)
        .checked_add(PREFIX_SIZE)
        .filter(|&start| start <= compressed.len())
        .ok_or_else(|| Error::invalid("HTJ2K chunk header size"))?;

    let count = usize::from(read_u16(take(2)?));
    if count != channel_count {
        return Err(Error::invalid("HTJ2K channel count does not match the header"));
    }

    // a permutation of the channels
    let mut file_channels = Vec::with_capacity(count);
    let mut seen = vec![false; count];
    for _ in 0..count {
        let index = usize::from(read_u16(take(2)?));
        if index >= count || seen[index] {
            return Err(Error::invalid("HTJ2K channel map"));
        }
        seen[index] = true;
        file_channels.push(index);
    }

    let codestream = &compressed[codestream_start..];
    if codestream.is_empty() {
        return Err(Error::invalid("HTJ2K codestream missing"));
    }
    Ok((file_channels, codestream))
}

/// Stores decoded samples as native-endian samples of `sample_type`. The
/// decoder returns the bits of each sample in an `i32`; a HALF sample is in
/// its low 16 bits.
fn write_samples(target: &mut [u8], samples: &[i32], sample_type: SampleType) {
    match sample_type {
        SampleType::F16 => {
            for (bytes, &sample) in target.chunks_exact_mut(2).zip(samples) {
                bytes.copy_from_slice(&(sample as u16).to_ne_bytes());
            }
        }
        SampleType::F32 | SampleType::U32 => {
            for (bytes, &sample) in target.chunks_exact_mut(4).zip(samples) {
                bytes.copy_from_slice(&(sample as u32).to_ne_bytes());
            }
        }
    }
}

fn codestream_error(error: ::htj2k::Error) -> Error {
    Error::invalid(format!("HTJ2K codestream: {}", error.message()))
}
