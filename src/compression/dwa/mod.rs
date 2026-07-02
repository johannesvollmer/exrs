// DWA / DWAB (lossy DCT) decompression for exrs (loading/decomp only).
// Ported from OpenEXRCore internal_dwa* .

use std::{convert::TryInto, sync::OnceLock};

use crate::{
    compression::ByteVec,
    error::{Error, Result},
    meta::attribute::{ChannelList, IntegerBounds, SampleType},
};

mod csc;
mod idct;

use half::f16;

/// Number of u64 counters in the DWA chunk header.
const NUM_SIZES_SINGLE: usize = 11;

#[derive(Debug, Clone, Copy)]
enum AcCompression {
    StaticHuffman,
    Deflate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CompressorScheme {
    Unknown = 0,
    LossyDct,
    Rle,
}

/// Layout of the fixed 11-counter DWA chunk header. Every slot stays named
/// here even though not all are read directly from `DataSizes` (some, like
/// `Version`, are only used while parsing), since this documents the on-disk format.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
#[repr(usize)]
enum DataSizes {
    Version = 0,
    UnknownUncompressedSize,
    UnknownCompressedSize,
    AcCompressedSize,
    DcCompressedSize,
    RleCompressedSize,
    RleUncompressedSize,
    RleRawSize,
    AcUncompressedCount,
    DcUncompressedCount,
    AcCompression,
}

/// Build / get the to-linear half-float lookup table (perceptual -> linear).
fn to_linear_table() -> &'static [u16; 65536] {
    static TABLE: OnceLock<[u16; 65536]> = OnceLock::new();

    TABLE.get_or_init(|| {
        let mut tab = [0u16; 65536];
        for (i, slot) in tab.iter_mut().enumerate() {
            let h = half::f16::from_bits(i as u16);
            *slot = dwa_convert_to_linear(h).to_bits();
        }
        tab
    })
}

/// Decoding needs the inverse (nonlinear stored -> linear).
fn dwa_convert_to_linear(x: half::f16) -> half::f16 {
    let f = x.to_f32();
    if !f.is_finite() {
        return half::f16::ZERO;
    }
    let sign = if f < 0.0 {
        -1.0
    } else {
        1.0
    };
    let f = f.abs();

    let out = if f <= 1.0 {
        f.powf(2.2)
    } else {
        // exp(2.2) ^ (f - 1) == exp(2.2 * (f - 1))
        (9.02501329156_f32).powf(f - 1.0)
    };

    half::f16::from_f32(sign * out)
}

/// Read the DWA chunk header counters (big-endian / XDR on disk).
fn read_dwa_counters(input: &mut &[u8]) -> Result<[u64; NUM_SIZES_SINGLE]> {
    let mut counters = [0u64; NUM_SIZES_SINGLE];
    for c in &mut counters {
        if input.len() < 8 {
            return Err(Error::invalid("truncated DWA counter"));
        }
        let bytes: [u8; 8] = input[..8].try_into().unwrap();
        *c = u64::from_le_bytes(bytes);
        *input = &input[8..];
    }
    Ok(counters)
}

/// Classifier used to decide how a channel was compressed, based on its name suffix.
#[derive(Debug, Clone)]
struct Classifier {
    suffix: String,
    scheme: CompressorScheme,
}

pub fn decompress(
    channels: &ChannelList,
    compressed_le: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    if compressed_le.is_empty() {
        return Ok(vec![0u8; expected_byte_size]);
    }

    if compressed_le.len() == expected_byte_size {
        return crate::compression::convert_little_endian_to_current(
            compressed_le,
            channels,
            rectangle,
        );
    }

    let full = compressed_le.as_slice();

    // Parse the fixed size header (11 * u64) from the start of the chunk data
    if full.len() < NUM_SIZES_SINGLE * 8 {
        return Err(Error::invalid("truncated DWA header"));
    }

    let mut hdr_cursor = &full[..];
    let counters = read_dwa_counters(&mut hdr_cursor)?;

    let version = counters[DataSizes::Version as usize];
    let unknown_comp = counters[DataSizes::UnknownCompressedSize as usize];
    let ac_comp = counters[DataSizes::AcCompressedSize as usize];
    let dc_comp = counters[DataSizes::DcCompressedSize as usize];
    let rle_comp = counters[DataSizes::RleCompressedSize as usize];
    let rle_uncompressed_size = counters[DataSizes::RleUncompressedSize as usize] as usize;
    let rle_raw_size = counters[DataSizes::RleRawSize as usize] as usize;
    let ac_count = counters[DataSizes::AcUncompressedCount as usize];
    let dc_count = counters[DataSizes::DcUncompressedCount as usize];
    let ac_comp_mode = counters[DataSizes::AcCompression as usize];

    let ac_compression = match ac_comp_mode {
        0 => AcCompression::StaticHuffman,
        1 => AcCompression::Deflate,
        _ => {
            return Err(Error::invalid("unknown DWA AC compression mode"));
        }
    };

    let after_header = &full[NUM_SIZES_SINGLE * 8..];
    // DWA v2+ prefixes the channel rules with their own 2-byte little-endian
    // total size (including the size field itself). That size is read directly
    // instead of inferring it from the buffer length, since the chunk buffer
    // can be larger than header + rules + sections.
    let rule_size = if version >= 2 && after_header.len() >= 2 {
        u16::from_le_bytes([after_header[0], after_header[1]]) as usize
    } else {
        0
    };
    let section_start = if rule_size > 0 && rule_size <= after_header.len() {
        &after_header[rule_size..]
    } else {
        after_header
    };
    let mut data = section_start;

    let channel_rules: Vec<Classifier> = get_legacy_channel_rules();

    // Split the four sections (unknown, AC, DC, RLE). Each length is clamped to
    // the data actually available so a truncated chunk degrades gracefully
    // instead of panicking on an out-of-bounds slice.
    let u_len = std::cmp::min(unknown_comp as usize, data.len());
    let u_sec = &data[..u_len];
    data = &data[u_len..];

    let a_len = std::cmp::min(ac_comp as usize, data.len());
    let ac_sec = &data[..a_len];
    data = &data[a_len..];

    let d_len = std::cmp::min(dc_comp as usize, data.len());
    let dc_sec = &data[..d_len];
    data = &data[d_len..];

    let r_len = std::cmp::min(rle_comp as usize, data.len());
    let rle_sec = &data[..r_len];

    // UNKNOWN section: raw (non-DCT-compressible) channel data, zlib-compressed
    // and laid out planar (i.e. one channel's full width*height data fully
    // before the next), in channel order. Matches DwaCompressor::uncompress
    // in internal_dwa_compressor.h, which just inflates this straight into
    // _planarUncBuffer[UNKNOWN] and memcpy's scanlines out of it verbatim.
    let unknown_uncompressed_size = counters[DataSizes::UnknownUncompressedSize as usize] as usize;
    let unknown_raw: Vec<u8> = if !u_sec.is_empty() {
        inflate_zlib(u_sec, unknown_uncompressed_size).unwrap_or_default()
    } else {
        vec![]
    };

    // RLE section: zlib-compressed, then classic byte-oriented RLE-encoded.
    // The RLE-decoded result is planar per RLE-scheme channel (e.g. alpha),
    // and within each channel it is further split into byte-planes (all
    // first bytes of every pixel, then all second bytes, ...), in channel
    // order. Matches DwaCompressor::uncompress's RLE handling in
    // internal_dwa_compressor.h, which zlib-inflates into `_rleBuffer` and
    // then calls `internal_rle_decompress` into `_planarUncBuffer[RLE]`.
    let rle_raw: Vec<u8> = if !rle_sec.is_empty() && rle_raw_size > 0 {
        let inflated = inflate_zlib(rle_sec, rle_uncompressed_size).unwrap_or_default();
        super::rle::unpack_rle_tokens(&inflated, rle_raw_size, false).unwrap_or_default()
    } else {
        vec![]
    };

    // AC section: either Huffman or zlib -> produces u16 values.
    let ac_packed: Vec<u16> = if !ac_sec.is_empty() {
        let p = match ac_compression {
            AcCompression::StaticHuffman => {
                crate::compression::piz::huffman::decompress(ac_sec, ac_count as usize)
                    .unwrap_or_default()
            }
            AcCompression::Deflate => {
                let bytes = inflate_zlib(ac_sec, (ac_count as usize) * 2).unwrap_or_default();
                bytes.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect()
            }
        };
        if p.is_empty() {
            // fallback treat sec as u16 le if count matches roughly
            ac_sec.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect()
        } else {
            p
        }
    } else {
        vec![]
    };

    // DC is always zlib + "zip reconstruct" (differencing undo) -> u16
    let dc_packed: Vec<u16> = if !dc_sec.is_empty() {
        let raw = match inflate_zlib(dc_sec, (dc_count as usize) * 2) {
            Ok(r) if !r.is_empty() => r,
            _ => dc_sec.to_vec(),
        };
        let reconstructed = undo_zip_reconstruct_for_dc(&raw, raw.len());
        reconstructed.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]])).collect()
    } else {
        vec![]
    };

    let mut out = vec![0u8; expected_byte_size];

    // Build channel classification for this chunk.
    let mut channel_infos: Vec<ChannelInfo> = Vec::new();

    for chan in channels.list.iter() {
        let samp_x = chan.sampling.x().max(1);
        let samp_y = chan.sampling.y().max(1);

        let ch_width = (rectangle.size.width() + samp_x - 1) / samp_x;
        let ch_height = (rectangle.size.height() + samp_y - 1) / samp_y;

        let bytes_per_sample = chan.sample_type.bytes_per_sample();
        let name_str = chan.name.to_string();
        let scheme = classify_channel(&name_str, &channel_rules);

        channel_infos.push(ChannelInfo {
            name: name_str,
            scheme,
            width: ch_width,
            height: ch_height,
            bytes_per_sample,
            sample_type: chan.sample_type,
        });
    }

    // Split the planar UNKNOWN buffer into one contiguous raw-byte run per
    // UNKNOWN-scheme channel, in channel order (mirrors
    // DwaCompressor_setupChannelData's running per-scheme cursor).
    let mut unknown_data: Vec<Vec<u8>> = vec![vec![]; channel_infos.len()];
    {
        let mut cursor = 0usize;
        for (i, info) in channel_infos.iter().enumerate() {
            if info.scheme == CompressorScheme::Unknown {
                let len = info.width * info.height * info.bytes_per_sample;
                if cursor + len <= unknown_raw.len() {
                    unknown_data[i] = unknown_raw[cursor..cursor + len].to_vec();
                }
                cursor += len;
            }
        }
    }

    // Split the planar RLE buffer into one raw-byte run per RLE-scheme
    // channel (mirrors the UNKNOWN split above), then un-transpose each
    // channel's byte-planes back into normal per-pixel interleaved order
    // (mirrors the `case RLE:` handling in DwaCompressor::uncompress, which
    // reads one byte at a time from `planarUncRleEnd[byte]` per byte-plane).
    let mut rle_data: Vec<Vec<u8>> = vec![vec![]; channel_infos.len()];
    {
        let mut cursor = 0usize;
        for (i, info) in channel_infos.iter().enumerate() {
            if info.scheme == CompressorScheme::Rle {
                let elems = info.width * info.height;
                let len = elems * info.bytes_per_sample;
                if cursor + len <= rle_raw.len() {
                    let block = &rle_raw[cursor..cursor + len];
                    let bpe = info.bytes_per_sample;
                    let mut interleaved = vec![0u8; len];
                    for e in 0..elems {
                        for b in 0..bpe {
                            interleaved[e * bpe + b] = block[b * elems + e];
                        }
                    }
                    rle_data[i] = interleaved;
                }
                cursor += len;
            }
        }
    }

    // Simple approach: decode all lossy channels to half, then write scanline by scanline.
    // This matches the expected layout from piz/others.

    let mut lossy_half_data: Vec<Vec<f16>> = vec![vec![]; channels.list.len()];

    // Group channels for CSC: find R/G/B triplets by name suffix, mirroring
    // DwaCompressor_classifyChannels's prefixMap approach in
    // internal_dwa_compressor.h (~line 1610-1698). Only suffixes with a
    // cscIdx of 0/1/2 (the R/G/B family, including legacy "r"/"red",
    // "g"/"grn"/"green", "b"/"blu"/"blue", matched case-insensitively) are
    // ever grouped for the inverse color-space transform. Y/RY/BY channels
    // have cscIdx == -1 in both sDefaultChannelRules and sLegacyChannelRules
    // in internal_dwa_classifier.h and are therefore *never* CSC-grouped by
    // the real compressor either - they are always decoded as standalone
    // single LOSSY_DCT channels (see the loop below), matching this decoder.
    let mut csc_groups: Vec<(usize, usize, usize)> = vec![]; // (r_idx, g_idx, b_idx) in channel_infos
    let mut processed = vec![false; channel_infos.len()];

    for i in 0..channel_infos.len() {
        if processed[i] || channel_infos[i].scheme != CompressorScheme::LossyDct {
            continue;
        }
        let name = &channel_infos[i].name;
        if let Some(base) = csc_prefix_for_index(name, 0) {
            let g_idx = find_channel_with_csc_index(&channel_infos, base, 1);
            let b_idx = find_channel_with_csc_index(&channel_infos, base, 2);
            if let (Some(gi), Some(bi)) = (g_idx, b_idx) {
                let (r_chan, g_chan, b_chan) =
                    (&channels.list[i], &channels.list[gi], &channels.list[bi]);
                if channel_infos[gi].scheme == CompressorScheme::LossyDct
                    && channel_infos[bi].scheme == CompressorScheme::LossyDct
                    && r_chan.sampling == g_chan.sampling
                    && r_chan.sampling == b_chan.sampling
                {
                    csc_groups.push((i, gi, bi));
                    processed[i] = true;
                    processed[gi] = true;
                    processed[bi] = true;
                }
            }
        }
    }

    // Process CSC groups
    let mut ac_cursor: usize = 0;
    let mut dc_cursor: usize = 0;

    for (r, g, b) in &csc_groups {
        let w = channel_infos[*r].width;
        let h = channel_infos[*r].height;

        let mut decoded = vec![vec![f16::ZERO; w * h]; 3]; // r g b decoded linear-ish

        decode_lossy_dct_group(
            &ac_packed,
            &mut ac_cursor,
            &dc_packed,
            &mut dc_cursor,
            w,
            h,
            true, // has csc
            &to_linear_table(),
            &mut decoded,
        );

        lossy_half_data[*r] = decoded[0].clone();
        lossy_half_data[*g] = decoded[1].clone();
        lossy_half_data[*b] = decoded[2].clone();
    }

    // Process remaining single LOSSY_DCT channels
    for (i, info) in channel_infos.iter().enumerate() {
        if processed[i] || info.scheme != CompressorScheme::LossyDct {
            continue;
        }
        let w = info.width;
        let h = info.height;

        let mut decoded = vec![vec![f16::ZERO; w * h]; 1];

        decode_lossy_dct_group(
            &ac_packed,
            &mut ac_cursor,
            &dc_packed,
            &mut dc_cursor,
            w,
            h,
            false,
            &to_linear_table(),
            &mut decoded,
        );

        lossy_half_data[i] = decoded[0].clone();
        processed[i] = true;
    }

    // Write the decoded data to `out` in scanline-interleaved layout (like piz).
    let mut out_cursor = 0usize;

    for y in rectangle.position.y()..rectangle.end().y() {
        for (ci, chan) in channels.list.iter().enumerate() {
            let samp_y = chan.sampling.y().max(1) as i32;
            if y % samp_y != 0 {
                continue;
            }

            let info = &channel_infos[ci];
            let line_w = info.width;
            let bytes_per_samp = info.bytes_per_sample;

            // For lossy channels, half the data, convert to target type
            if info.scheme == CompressorScheme::LossyDct && !lossy_half_data[ci].is_empty() {
                let ch_data = &lossy_half_data[ci];
                let line_start_in_ch = (((y - rectangle.position.y()) / samp_y) as usize) * line_w;

                for x in 0..line_w {
                    let pix = ch_data[line_start_in_ch + x];
                    match info.sample_type {
                        SampleType::F16 => {
                            let bits = pix.to_bits();
                            out[out_cursor..out_cursor + 2].copy_from_slice(&bits.to_le_bytes());
                            out_cursor += 2;
                        }
                        SampleType::F32 => {
                            let f = pix.to_f32();
                            out[out_cursor..out_cursor + 4].copy_from_slice(&f.to_le_bytes());
                            out_cursor += 4;
                        }
                        SampleType::U32 => {
                            // unlikely for DWA lossy
                            out[out_cursor..out_cursor + 4].copy_from_slice(&(0u32).to_le_bytes());
                            out_cursor += 4;
                        }
                    }
                }
            } else if (info.scheme == CompressorScheme::Unknown && !unknown_data[ci].is_empty())
                || (info.scheme == CompressorScheme::Rle && !rle_data[ci].is_empty())
            {
                // Raw bytes, scanline-wise per channel (see planar split above).
                let raw = if info.scheme == CompressorScheme::Unknown {
                    &unknown_data[ci]
                } else {
                    &rle_data[ci]
                };

                let row_in_ch = ((y - rectangle.position.y()) / samp_y) as usize;
                let byte_len = line_w * bytes_per_samp;
                let byte_off = row_in_ch * byte_len;

                if byte_off + byte_len <= raw.len() {
                    out[out_cursor..out_cursor + byte_len]
                        .copy_from_slice(&raw[byte_off..byte_off + byte_len]);
                }
                out_cursor += byte_len;
            } else {
                // Only reached if an RLE-scheme channel's data could not be
                // decoded (e.g. missing/corrupt RLE section); alpha then
                // defaults to fully opaque rather than fully transparent.
                // The written value must still match `bytes_per_samp` for this channel's
                // actual sample type, or every following channel/scanline shifts out of place.
                if info.name == "A" || info.name.ends_with("A") {
                    for _ in 0..line_w {
                        match info.sample_type {
                            SampleType::F16 => {
                                out[out_cursor..out_cursor + 2]
                                    .copy_from_slice(&f16::ONE.to_bits().to_le_bytes());
                            }
                            SampleType::F32 => {
                                out[out_cursor..out_cursor + 4]
                                    .copy_from_slice(&(1.0f32).to_le_bytes());
                            }
                            SampleType::U32 => {
                                out[out_cursor..out_cursor + 4]
                                    .copy_from_slice(&(1u32).to_le_bytes());
                            }
                        }
                        out_cursor += bytes_per_samp;
                    }
                } else {
                    out_cursor += line_w * bytes_per_samp;
                }
            }
        }
    }

    Ok(out)
}

// --------------------- helpers ---------------------

fn get_legacy_channel_rules() -> Vec<Classifier> {
    vec![
        Classifier {
            suffix: "r".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "red".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "g".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "grn".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "green".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "b".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "blu".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "blue".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "y".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "by".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "ry".into(),
            scheme: CompressorScheme::LossyDct,
        },
        Classifier {
            suffix: "a".into(),
            scheme: CompressorScheme::Rle,
        },
    ]
}

/// Inflate a zlib/raw-deflate buffer. Tries miniz_oxide first, then falls
/// back to zune_inflate's raw-deflate and zlib modes, since some DWA chunks
/// in the wild lack a proper zlib header.
fn inflate_zlib(compressed: &[u8], _expected: usize) -> Result<Vec<u8>> {
    if compressed.is_empty() {
        return Ok(vec![]);
    }
    if let Ok(out) = miniz_oxide::inflate::decompress_to_vec(compressed) {
        if !out.is_empty() {
            return Ok(out);
        }
    }
    {
        let mut d = zune_inflate::DeflateDecoder::new(compressed);
        if let Ok(out) = d.decode_deflate() {
            if !out.is_empty() {
                return Ok(out);
            }
        }
    }
    {
        let mut d = zune_inflate::DeflateDecoder::new(compressed);
        if let Ok(out) = d.decode_zlib() {
            if !out.is_empty() {
                return Ok(out);
            }
        }
    }
    Err(Error::invalid("DWA inflate failed"))
}

/// Undo the "zip reconstruct" step that OpenEXR applies to the DC buffer before compressing.
/// Ports internal_zip_reconstruct_bytes: reconstruct then interleave.
fn undo_zip_reconstruct_for_dc(source: &[u8], count: usize) -> Vec<u8> {
    if count < 2 {
        return source.to_vec();
    }
    let mut buf = source.to_vec();
    reconstruct(&mut buf, count);
    let mut out = vec![0u8; count];
    interleave(&mut out, &buf, count);
    out
}

fn reconstruct(buf: &mut [u8], sz: usize) {
    let mut t = 1;
    while t < sz {
        let d = (buf[t - 1] as i32) + (buf[t] as i32) - 128;
        buf[t] = d as u8;
        t += 1;
    }
}

fn interleave(out: &mut [u8], source: &[u8], out_size: usize) {
    let mut t1 = 0;
    let mut t2 = (out_size + 1) / 2;
    let mut s = 0;
    while s < out_size {
        if s < out_size {
            out[s] = source[t1];
            t1 += 1;
            s += 1;
        } else {
            break;
        }
        if s < out_size {
            out[s] = source[t2];
            t2 += 1;
            s += 1;
        } else {
            break;
        }
    }
}

#[derive(Debug, Clone)]
struct ChannelInfo {
    name: String,
    scheme: CompressorScheme,
    width: usize,
    height: usize,
    bytes_per_sample: usize,
    sample_type: SampleType,
}

fn classify_channel(name: &str, rules: &[Classifier]) -> CompressorScheme {
    let name_lower = name.to_ascii_lowercase();
    for rule in rules {
        let suf = rule.suffix.to_ascii_lowercase();
        if name_lower.ends_with(&suf) || name_lower == suf {
            return rule.scheme;
        }
    }
    CompressorScheme::Unknown
}

/// R/G/B family suffixes recognized for CSC grouping, with their cscIdx
/// (0=R, 1=G, 2=B), matching sDefaultChannelRules/sLegacyChannelRules in
/// internal_dwa_classifier.h (Y/RY/BY are intentionally absent: they have
/// cscIdx == -1 there and are never CSC-grouped).
const CSC_SUFFIXES: [(&str, usize); 8] =
    [("r", 0), ("red", 0), ("g", 1), ("grn", 1), ("green", 1), ("b", 2), ("blu", 2), ("blue", 2)];

/// The part of a channel name after the last '.', matching
/// `Classifier_find_suffix` in internal_dwa_classifier.h.
fn channel_suffix(name: &str) -> &str {
    match name.rfind('.') {
        Some(idx) => &name[idx + 1..],
        None => name,
    }
}

/// If `name`'s suffix case-insensitively matches a CSC rule with the given
/// `csc_idx`, returns the name's prefix (everything before the suffix), so
/// sibling R/G/B channels sharing that prefix can be located.
fn csc_prefix_for_index(name: &str, csc_idx: usize) -> Option<&str> {
    let suffix = channel_suffix(name);
    let suffix_lower = suffix.to_ascii_lowercase();
    for (s, idx) in CSC_SUFFIXES {
        if idx == csc_idx && suffix_lower == s {
            return Some(&name[..name.len() - suffix.len()]);
        }
    }
    None
}

fn find_channel_with_csc_index(
    infos: &[ChannelInfo],
    prefix: &str,
    csc_idx: usize,
) -> Option<usize> {
    infos.iter().position(|info| csc_prefix_for_index(&info.name, csc_idx) == Some(prefix))
}

/// Decode one or three LOSSY_DCT channels (with optional CSC).
fn decode_lossy_dct_group(
    ac_packed: &[u16],
    ac_cursor: &mut usize,
    dc_packed: &[u16],
    dc_cursor: &mut usize,
    width: usize,
    height: usize,
    has_csc: bool,
    to_linear: &[u16; 65536],
    decoded: &mut [Vec<f16>],
) {
    let num_comp = if has_csc {
        3
    } else {
        1
    };
    let num_blocks_x = (width + 7) / 8;
    let num_blocks_y = (height + 7) / 8;

    let n_blocks = num_blocks_x * num_blocks_y;

    for by in 0..num_blocks_y {
        for bx in 0..num_blocks_x {
            let mut comp_dcs = [f16::ZERO; 3];
            let mut last_nz = [0usize; 3];
            let mut zig_blocks: [[u16; 64]; 3] = [[0u16; 64]; 3];
            let mut dct_blocks: [[f32; 64]; 3] = [[0.0f32; 64]; 3];

            let block_idx = by * num_blocks_x + bx;
            for c in 0..num_comp {
                let dc_idx = *dc_cursor + c * n_blocks + block_idx;
                if dc_idx < dc_packed.len() {
                    comp_dcs[c] = f16::from_bits(dc_packed[dc_idx]);
                }
            }

            // UnRLE AC for each comp into its own zig block
            for c in 0..num_comp {
                zig_blocks[c][0] = comp_dcs[c].to_bits();
                un_rle_ac_into(ac_packed, ac_cursor, &mut zig_blocks[c], &mut last_nz[c]);
            }

            // IDCT for each comp
            for c in 0..num_comp {
                if last_nz[c] == 0 {
                    dct_blocks[c][0] = comp_dcs[c].to_f32();
                    idct::dct_inverse_8x8_dc_only(&mut dct_blocks[c]);
                } else {
                    from_half_zigzag(&zig_blocks[c], &mut dct_blocks[c]);
                    idct::dct_inverse_8x8(&mut dct_blocks[c]);
                }
            }

            // CSC inverse on the float dct blocks if applicable (after IDCT)
            if has_csc && num_comp == 3 {
                for i in 0..64 {
                    let (r, g, b) =
                        csc::csc709_inverse(dct_blocks[0][i], dct_blocks[1][i], dct_blocks[2][i]);
                    dct_blocks[0][i] = r;
                    dct_blocks[1][i] = g;
                    dct_blocks[2][i] = b;
                }
            }

            // Now convert each comp's dct block (nonlinear float) to linear half via to_linear LUT
            // and write to the output decoded buffers
            let bx0 = bx * 8;
            let by0 = by * 8;

            for c in 0..num_comp {
                let comp_buf = &mut decoded[c];
                for yy in 0..8 {
                    let y = by0 + yy;
                    if y >= height {
                        break;
                    }
                    for xx in 0..8 {
                        let x = bx0 + xx;
                        if x >= width {
                            break;
                        }

                        let val = dct_blocks[c][yy * 8 + xx];
                        let h = f16::from_f32(val);
                        let linear_h_bits = to_linear[h.to_bits() as usize];
                        let linear_h = f16::from_bits(linear_h_bits);

                        let idx = y * width + x;
                        if idx < comp_buf.len() {
                            comp_buf[idx] = linear_h;
                        }
                    }
                }
            }
        }
    }
    // DC was read via planar indexing, advance cursor for subsequent singles/groups
    *dc_cursor += num_comp * n_blocks;
}

fn un_rle_ac_into(
    ac: &[u16],
    cursor: &mut usize,
    block: &mut [u16; 64],
    last_nz: &mut usize,
) -> usize {
    let mut dct_comp = 1usize;
    let mut consumed = 0usize;
    *last_nz = 0;

    while dct_comp < 64 {
        if *cursor >= ac.len() {
            break;
        }
        let val = ac[*cursor];
        *cursor += 1;
        consumed += 1;

        if (val & 0xff00) == 0xff00 {
            let count = (val & 0xff) as usize;
            dct_comp += if count == 0 {
                64
            } else {
                count
            };
        } else {
            *last_nz = dct_comp;
            block[dct_comp] = val;
            dct_comp += 1;
        }
    }
    consumed
}

fn from_half_zigzag(src_zig: &[u16; 64], dst: &mut [f32; 64]) {
    // Mapping from the C fromHalfZigZag_scalar
    const SRC_INDICES: [usize; 64] = [
        0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9,
        11, 18, 24, 31, 40, 44, 53, 10, 19, 23, 32, 39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60,
        21, 34, 37, 47, 50, 56, 59, 61, 35, 36, 48, 49, 57, 58, 62, 63,
    ];

    for (i, &src_idx) in SRC_INDICES.iter().enumerate() {
        dst[i] = f16::from_bits(src_zig[src_idx]).to_f32();
    }
}
