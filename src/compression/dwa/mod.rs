// DWA / DWAB (lossy DCT) compression and decompression, ported from
// OpenEXRCores internal_dwa_compressor.h and alike.
//
// A DWA chunk is: 11 u64 counters, the channel rules (version >= 2),
// then four sections (UNKNOWN, AC, DC, RLE). Channels are classified by
// name suffix + sample type into a compression scheme; R/G/B triplets
// sharing a prefix are additionally CSC'd (Y'CbCr). All LOSSY_DCT
// channel groups of a chunk consume the same planar AC/DC streams.
//
// This module holds the two public entry points (`compress`/`decompress`) and
// the cross-cutting channel classification both directions share; each stage of
// the pipeline lives in its own submodule.

use crate::{
    compression::ByteVec,
    error::{Error, Result},
    meta::attribute::{ChannelList, IntegerBounds, SampleType},
};

mod channel_layout;
mod channel_rules;
mod chunk_header;
mod lossy_dct;
mod section_stream;

// Public for benches.
#[doc(hidden)]
pub mod color_space_conversion;

// Public for benches.
#[doc(hidden)]
pub mod discrete_cosine_transform;

#[cfg(test)]
mod tests;

// Public for the `dwa_bench` example.
#[doc(hidden)]
#[cfg(feature = "dwa-profile")]
pub mod profile;

use channel_layout::{
    compute_row_offsets, pack_rle_channels, pack_unknown_channels, rle_planar_size,
    split_scanline_channels, u16s_to_le_bytes, write_scanlines_fused,
};
use channel_rules::{
    Rule, default_channel_rules, legacy_channel_rules, parse_channel_rules,
    write_relevant_channel_rules,
};
use chunk_header::{AcCompression, DwaHeader};
use lossy_dct::{decode_lossy_channels, encode_lossy_channels};
use section_stream::{
    decode_ac_section_into, decode_dc_section, decode_rle_section_into,
    decode_unknown_section_into, split_sections, zip_deconstruct_bytes,
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum CompressorScheme {
    Unknown,
    LossyDct,
    Rle,
}

/// The part of a channel name after the last '.'
fn channel_suffix(name: &str) -> &str {
    match name.rfind('.') {
        Some(dot) => &name[dot + 1..],
        None => name,
    }
}

#[derive(Debug, Clone)]
struct ChannelInfo {
    scheme: CompressorScheme,
    width: usize,
    height: usize,
    bytes_per_sample: usize,
    sample_type: SampleType,
    quantize_linearly: bool,
}

/// Classify every channel and find the CSC'd R/G/B triplets, mirroring
/// "DwaCompressor_classifyChannels": a prefix map (in order of first
/// appearance, which determines group decode order) collects which channel
/// holds each triplet slot, and a group forms when all three slots are
/// filled with LOSSY_DCT channels of identical sampling.
fn classify_channels(
    channels: &ChannelList,
    rectangle: IntegerBounds,
    rules: &[Rule],
) -> (Vec<ChannelInfo>, Vec<[usize; 3]>) {
    // Match channel names against the rule table, then collect R/G/B triplets
    // in the order the prefixes first appear. That order controls how the
    // shared lossy streams are consumed later.
    let mut infos = Vec::with_capacity(channels.list.len());
    let mut prefix_map: Vec<(String, [Option<usize>; 3])> = Vec::new();

    for (index, channel) in channels.list.iter().enumerate() {
        let name = channel.name.to_string();
        let suffix = channel_suffix(&name);
        let prefix = &name[..name.len() - suffix.len()];

        if !prefix_map.iter().any(|(known, _)| known == prefix) {
            prefix_map.push((prefix.to_string(), [None; 3]));
        }

        let mut scheme = CompressorScheme::Unknown;
        for rule in rules {
            if rule.matches(suffix, channel.sample_type) {
                scheme = rule.scheme;
                if let Some(csc_index) = rule.csc_index {
                    for (known, slots) in &mut prefix_map {
                        if known == prefix {
                            slots[csc_index] = Some(index);
                        }
                    }
                }
            }
        }

        let sampling_x = channel.sampling.x().max(1);
        let sampling_y = channel.sampling.y().max(1);
        infos.push(ChannelInfo {
            scheme,
            width: (rectangle.size.width() + sampling_x - 1) / sampling_x,
            height: (rectangle.size.height() + sampling_y - 1) / sampling_y,
            bytes_per_sample: channel.sample_type.bytes_per_sample(),
            sample_type: channel.sample_type,
            quantize_linearly: channel.quantize_linearly,
        });
    }

    let csc_groups = prefix_map
        .into_iter()
        .filter_map(|(_, slots)| {
            let [Some(r), Some(g), Some(b)] = slots else {
                return None;
            };
            let all_lossy =
                [r, g, b].iter().all(|&index| infos[index].scheme == CompressorScheme::LossyDct);
            let same_sampling = channels.list[r].sampling == channels.list[g].sampling
                && channels.list[r].sampling == channels.list[b].sampling;
            (all_lossy && same_sampling).then_some([r, g, b])
        })
        .collect();

    (infos, csc_groups)
}

pub fn compress(
    channels: &ChannelList,
    uncompressed_ne: ByteVec,
    rectangle: IntegerBounds,
    compression_level: Option<f32>,
) -> Result<ByteVec> {
    if uncompressed_ne.is_empty() {
        return Ok(vec![]);
    }

    let uncompressed_le =
        crate::compression::convert_current_to_little_endian(uncompressed_ne, channels, rectangle)?;

    // The chunk is written in the same section order the decoder expects:
    // header + rules, then UNKNOWN, AC, DC and RLE payloads.
    let rules = default_channel_rules();
    let rule_bytes = write_relevant_channel_rules(&rules, channels)?;
    let (channel_infos, csc_groups) = classify_channels(channels, rectangle, &rules);
    let channel_bytes =
        split_scanline_channels(&uncompressed_le, channels, &channel_infos, rectangle)?;

    let unknown_planar =
        pack_unknown_channels(&channel_infos, &channel_bytes, CompressorScheme::Unknown);
    let rle_raw = pack_rle_channels(&channel_infos, &channel_bytes);

    let quant_base_error = compression_level.unwrap_or(45.0) / 100000.0;
    let (ac_values, dc_values) =
        encode_lossy_channels(&channel_infos, &csc_groups, &channel_bytes, quant_base_error)?;

    let unknown_compressed = if unknown_planar.is_empty() {
        Vec::new()
    } else {
        super::compress_zlib(&unknown_planar, 9)
    };

    let ac_compression = AcCompression::StaticHuffman;
    let ac_compressed = if ac_values.is_empty() {
        Vec::new()
    } else {
        crate::compression::huffman::compress(&ac_values)?
    };

    let dc_compressed = if dc_values.is_empty() {
        Vec::new()
    } else {
        let mut dc_bytes = u16s_to_le_bytes(&dc_values);
        zip_deconstruct_bytes(&mut dc_bytes);
        super::compress_zlib(&dc_bytes, 9)
    };

    let (rle_uncompressed_size, rle_compressed) = if rle_raw.is_empty() {
        (0, Vec::new())
    } else {
        let rle_tokens = super::rle::pack_rle_tokens(&rle_raw);
        let compressed = super::compress_zlib(&rle_tokens, 9);
        let rle_tokens_len = rle_tokens.len();
        crate::block::pool::recycle(rle_tokens);
        (rle_tokens_len, compressed)
    };

    let header = DwaHeader {
        version: 2,
        unknown_uncompressed_size: unknown_planar.len(),
        unknown_compressed_size: unknown_compressed.len(),
        ac_compressed_size: ac_compressed.len(),
        dc_compressed_size: dc_compressed.len(),
        rle_compressed_size: rle_compressed.len(),
        rle_uncompressed_size,
        rle_raw_size: rle_raw.len(),
        ac_count: ac_values.len(),
        dc_count: dc_values.len(),
        ac_compression,
    };

    let mut out = crate::block::pool::take_with_capacity(
        11 * 8
            + rule_bytes.len()
            + unknown_compressed.len()
            + ac_compressed.len()
            + dc_compressed.len()
            + rle_compressed.len(),
    );
    header.write(&mut out);
    out.extend_from_slice(&rule_bytes);
    out.extend_from_slice(&unknown_compressed);
    out.extend_from_slice(&ac_compressed);
    out.extend_from_slice(&dc_compressed);
    out.extend_from_slice(&rle_compressed);

    // Already copied into `out`; recycle so the next chunk can reuse the pages.
    crate::block::pool::recycle(unknown_compressed);
    crate::block::pool::recycle(ac_compressed);
    crate::block::pool::recycle(dc_compressed);
    crate::block::pool::recycle(rle_compressed);

    Ok(out)
}

pub fn decompress(
    channels: &ChannelList,
    compressed_le: &[u8],
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    if compressed_le.is_empty() {
        return Ok(vec![0u8; expected_byte_size]);
    }

    // the writer stores chunks raw when compression would not have helped
    if compressed_le.len() == expected_byte_size {
        return crate::compression::convert_little_endian_to_current(
            compressed_le.to_vec(),
            channels,
            rectangle,
        );
    }

    #[cfg(feature = "dwa-profile")]
    let total = profile::start();

    let mut input = compressed_le;
    let header = DwaHeader::parse(&mut input)?;

    let rules = if header.version < 2 {
        legacy_channel_rules()
    } else {
        parse_channel_rules(&mut input)?
    };

    let (channel_infos, csc_groups) = classify_channels(channels, rectangle, &rules);

    if channel_infos.iter().any(|info| {
        info.scheme == CompressorScheme::LossyDct && info.sample_type == SampleType::U32
    }) {
        return Err(Error::unsupported("DWA lossy DCT compression of u32 channels"));
    }

    let [unknown_section, ac_section, dc_section, rle_section] = split_sections(input, &header)?;

    // Reused across chunks on this thread (zlib output, no further expansion).
    thread_local! {
        static UNKNOWN_BUFFER: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    #[cfg(feature = "dwa-profile")]
    let t = profile::start();
    let mut unknown_buffer =
        UNKNOWN_BUFFER.with(|buffer| std::mem::take(&mut *buffer.borrow_mut()));
    let unknown_len = decode_unknown_section_into(unknown_section, &header, &mut unknown_buffer)?;
    let unknown_planar = &unknown_buffer[..unknown_len];
    #[cfg(feature = "dwa-profile")]
    t.stop(&profile::UNKNOWN_NS);

    // Reused across chunks: AC coefficients plus Huffman scratch (`words`).
    thread_local! {
        static AC_OUT_BUFFER: std::cell::RefCell<Vec<u16>> = const { std::cell::RefCell::new(Vec::new()) };
        static AC_WORDS_BUFFER: std::cell::RefCell<Vec<u64>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    #[cfg(feature = "dwa-profile")]
    let t = profile::start();
    let mut ac_buffer = AC_OUT_BUFFER.with(|buffer| std::mem::take(&mut *buffer.borrow_mut()));
    let mut ac_words_buffer =
        AC_WORDS_BUFFER.with(|buffer| std::mem::take(&mut *buffer.borrow_mut()));
    let ac_len = decode_ac_section_into(ac_section, &header, &mut ac_buffer, &mut ac_words_buffer)?;
    AC_WORDS_BUFFER.with(|buffer| *buffer.borrow_mut() = ac_words_buffer);
    #[cfg(feature = "dwa-profile")]
    t.stop(&profile::AC_NS);

    #[cfg(feature = "dwa-profile")]
    let t = profile::start();
    let dc_packed = decode_dc_section(dc_section, &header)?;
    #[cfg(feature = "dwa-profile")]
    t.stop(&profile::DC_NS);

    // Reused across chunks; on error this thread simply drops it.
    thread_local! {
        static RLE_BUFFER: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    #[cfg(feature = "dwa-profile")]
    let t = profile::start();
    let mut rle_buffer = RLE_BUFFER.with(|buffer| std::mem::take(&mut *buffer.borrow_mut()));
    let rle_length = decode_rle_section_into(
        rle_section,
        &header,
        rle_planar_size(&channel_infos),
        &mut rle_buffer,
    )?;
    let rle_planar = &rle_buffer[..rle_length];
    #[cfg(feature = "dwa-profile")]
    t.stop(&profile::RLE_NS);

    let row_offsets = compute_row_offsets(channels, &channel_infos, rectangle);

    // Caller-owned; comes from the block pool (readers recycle after copy-out).
    #[cfg(feature = "dwa-profile")]
    let t = profile::start();
    let mut out = crate::block::pool::take_zeroed(expected_byte_size);
    #[cfg(feature = "dwa-profile")]
    {
        t.stop(&profile::OUT_ALLOC_NS);
        profile::OUT_BYTES
            .fetch_add(expected_byte_size as u64, std::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(feature = "dwa-profile")]
    let t = profile::start();
    decode_lossy_channels(
        &channel_infos,
        &csc_groups,
        &ac_buffer[..ac_len],
        &dc_packed,
        &row_offsets,
        &mut out,
    )?;
    AC_OUT_BUFFER.with(|buffer| *buffer.borrow_mut() = ac_buffer);
    #[cfg(feature = "dwa-profile")]
    t.stop(&profile::DCT_NS);

    #[cfg(feature = "dwa-profile")]
    let t = profile::start();
    write_scanlines_fused(
        channels,
        &channel_infos,
        rectangle,
        &row_offsets,
        unknown_planar,
        rle_planar,
        &mut out,
    )?;
    #[cfg(feature = "dwa-profile")]
    t.stop(&profile::ASSEMBLE_NS);

    RLE_BUFFER.with(|buffer| *buffer.borrow_mut() = rle_buffer);
    UNKNOWN_BUFFER.with(|buffer| *buffer.borrow_mut() = unknown_buffer);

    #[cfg(feature = "dwa-profile")]
    total.stop(&profile::TOTAL_NS);

    crate::compression::convert_little_endian_to_current(out, channels, rectangle)
}
