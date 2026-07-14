// DWA / DWAB (lossy DCT) decompression, ported from OpenEXRCores
// internal_dwa_compressor.h and alike (decoding only).
//
// A DWA chunk is: 11 u64 counters, the channel rules (version >= 2),
// then four sections (UNKNOWN, AC, DC, RLE). Channels are classified by
// name suffix + sample type into a compression scheme; R/G/B triplets
// sharing a prefix are additionally CSC'd (Y'CbCr). All LOSSY_DCT
// channel groups of a chunk consume the same planar AC/DC streams.

use std::{borrow::Cow, convert::TryInto, sync::OnceLock};

use half::f16;

use crate::{
    compression::ByteVec,
    error::{Error, Result},
    meta::attribute::{ChannelList, IntegerBounds, SampleType},
};

mod csc;
mod idct;
mod quantize;

#[cfg(test)]
mod tests;

/// Temporary coarse-phase timing, only compiled in with `--features dwa-profile`.
/// Not for production use: global atomics, reset/reported manually from a
/// single-threaded caller between runs.
#[cfg(feature = "dwa-profile")]
#[allow(missing_docs)]
pub mod profile {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    #[derive(Default)]
    pub struct Phase(AtomicU64);
    impl Phase {
        pub const fn new() -> Self { Self(AtomicU64::new(0)) }
        pub fn add(&self, nanos: u64) { self.0.fetch_add(nanos, Ordering::Relaxed); }
        pub fn get_ms(&self) -> f64 { self.0.load(Ordering::Relaxed) as f64 / 1_000_000.0 }
        pub fn reset(&self) { self.0.store(0, Ordering::Relaxed); }
    }

    pub static SETUP: Phase = Phase::new();
    pub static ENTROPY_AC: Phase = Phase::new();
    pub static ENTROPY_OTHER: Phase = Phase::new();
    pub static PLANAR_SPLIT: Phase = Phase::new();
    pub static DEQUANT: Phase = Phase::new();
    pub static IDCT: Phase = Phase::new();
    pub static CSC_LUT: Phase = Phase::new();
    pub static WRITE: Phase = Phase::new();
    pub static AC_TABLE: Phase = Phase::new(); // huffman table parse+build share of ENTROPY_AC
    pub static AC_COUNT: Phase = Phase::new(); // abused as a plain counter, not nanoseconds
    pub static AC_SLOW: Phase = Phase::new(); // counter: symbols taking the long-code scan path

    pub fn reset_all() {
        SETUP.reset(); ENTROPY_AC.reset(); ENTROPY_OTHER.reset();
        PLANAR_SPLIT.reset(); DEQUANT.reset(); IDCT.reset(); CSC_LUT.reset(); WRITE.reset();
        AC_TABLE.reset(); AC_COUNT.reset(); AC_SLOW.reset();
    }

    pub fn report() {
        eprintln!("--- dwa phase timings (sum across all chunks) ---");
        eprintln!("total ac symbols decoded: {}", AC_COUNT.0.load(Ordering::Relaxed));
        eprintln!("long-code scan hits:      {}", AC_SLOW.0.load(Ordering::Relaxed));
        eprintln!("setup           {:>8.2} ms", SETUP.get_ms());
        eprintln!("entropy: ac     {:>8.2} ms", ENTROPY_AC.get_ms());
        eprintln!("  - table build {:>8.2} ms", AC_TABLE.get_ms());
        eprintln!("entropy: other  {:>8.2} ms", ENTROPY_OTHER.get_ms());
        eprintln!("planar split    {:>8.2} ms", PLANAR_SPLIT.get_ms());
        eprintln!("dequant/un-rle  {:>8.2} ms", DEQUANT.get_ms());
        eprintln!("idct            {:>8.2} ms", IDCT.get_ms());
        eprintln!("csc + lut       {:>8.2} ms", CSC_LUT.get_ms());
        eprintln!("write_scanlines {:>8.2} ms", WRITE.get_ms());
    }

    pub struct Timer<'a> { start: Instant, phase: &'a Phase }
    impl<'a> Timer<'a> {
        pub fn start(phase: &'a Phase) -> Self { Self { start: Instant::now(), phase } }
    }
    impl<'a> Drop for Timer<'a> {
        fn drop(&mut self) { self.phase.add(self.start.elapsed().as_nanos() as u64); }
    }
}

#[cfg(feature = "dwa-profile")]
macro_rules! timed {
    ($phase:expr, $body:expr) => {{
        let _t = profile::Timer::start(&$phase);
        $body
    }};
}
#[cfg(not(feature = "dwa-profile"))]
macro_rules! timed {
    ($phase:expr, $body:expr) => {
        $body
    };
}

#[cfg(feature = "simd-benches")]
#[allow(missing_docs)]
#[doc(hidden)]
pub mod simd_bench_support {
    pub use super::idct::simd_bench_support::*;
}

#[cfg(any(feature = "avx2-tests", feature = "sse2-tests"))]
#[allow(missing_docs)]
#[doc(hidden)]
pub mod simd_test_support {
    pub use super::idct::simd_test_support::*;
}

#[derive(Debug, Clone, Copy)]
enum AcCompression {
    StaticHuffman,
    Deflate,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CompressorScheme {
    Unknown,
    LossyDct,
    Rle,
}

/// The 11 little-endian u64 counters at the start of every DWA chunk
/// (`DataSizesSingle` in internal_dwa_compressor.h), in on-disk order.
struct DwaHeader {
    version: u64,
    unknown_uncompressed_size: usize,
    unknown_compressed_size: usize,
    ac_compressed_size: usize,
    dc_compressed_size: usize,
    rle_compressed_size: usize,
    rle_uncompressed_size: usize,
    rle_raw_size: usize,
    ac_count: usize,
    dc_count: usize,
    ac_compression: AcCompression,
}

impl DwaHeader {
    fn parse(input: &mut &[u8]) -> Result<Self> {
        fn counter(input: &mut &[u8]) -> Result<u64> {
            let (bytes, rest) = input
                .split_first_chunk::<8>()
                .ok_or_else(|| Error::invalid("truncated DWA header"))?;
            *input = rest;
            Ok(u64::from_le_bytes(*bytes))
        }

        // the C parser rejects counters with the top bit set
        fn size(value: u64) -> Result<usize> {
            if value > (i64::MAX as u64) {
                return Err(Error::invalid("DWA counter out of range"));
            }
            value.try_into().map_err(|_| Error::invalid("DWA counter out of range"))
        }

        Ok(Self {
            version: counter(input)?,
            unknown_uncompressed_size: size(counter(input)?)?,
            unknown_compressed_size: size(counter(input)?)?,
            ac_compressed_size: size(counter(input)?)?,
            dc_compressed_size: size(counter(input)?)?,
            rle_compressed_size: size(counter(input)?)?,
            rle_uncompressed_size: size(counter(input)?)?,
            rle_raw_size: size(counter(input)?)?,
            ac_count: size(counter(input)?)?,
            dc_count: size(counter(input)?)?,
            ac_compression: match counter(input)? {
                0 => AcCompression::StaticHuffman,
                1 => AcCompression::Deflate,
                _ => {
                    return Err(Error::invalid("unknown DWA AC compression mode"));
                }
            },
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        // Keep the on-disk layout identical to the decoder's parse order.
        let counters = [
            self.version,
            self.unknown_uncompressed_size as u64,
            self.unknown_compressed_size as u64,
            self.ac_compressed_size as u64,
            self.dc_compressed_size as u64,
            self.rle_compressed_size as u64,
            self.rle_uncompressed_size as u64,
            self.rle_raw_size as u64,
            self.ac_count as u64,
            self.dc_count as u64,
            match self.ac_compression {
                AcCompression::StaticHuffman => 0u64,
                AcCompression::Deflate => 1u64,
            },
        ];

        for counter in counters {
            out.extend_from_slice(&counter.to_le_bytes());
        }
    }
}

/// One channel classification rule (`Classifier` in
/// internal_dwa_classifier.h): matches a channel by name suffix and sample
/// type, and assigns its compression scheme.
struct Rule {
    suffix: Cow<'static, str>,
    scheme: CompressorScheme,
    sample_type: SampleType,
    /// "Some(0/1/2)" marks this suffix as the R/G/B member of a potential
    /// CSC triplet; "None" (like Y/RY/BY/A) is never CSC-grouped.
    csc_index: Option<usize>,
    case_insensitive: bool,
}

impl Rule {
    /// "Classifier_match" exact suffix comparison, plus type equality.
    fn matches(&self, suffix: &str, sample_type: SampleType) -> bool {
        self.sample_type == sample_type
            && (if self.case_insensitive {
                suffix.eq_ignore_ascii_case(&self.suffix)
            } else {
                suffix == self.suffix
            })
    }

    fn serialized_size(&self) -> usize {
        self.suffix.len() + 1 + 2
    }

    /// "Classifier_write": a NUL-terminated suffix, a packed flags byte, and
    /// a pixel type byte. The inverse of `parse_rule`.
    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        out.extend_from_slice(self.suffix.as_bytes());
        out.push(0);

        let csc_bits = match self.csc_index {
            None => 0,
            Some(index @ 0..=2) => index + 1,
            Some(_) => return Err(Error::invalid("DWA channel rule csc index out of range")),
        };

        let scheme_bits = match self.scheme {
            CompressorScheme::Unknown => 0,
            CompressorScheme::LossyDct => 1,
            CompressorScheme::Rle => 2,
        };

        let sample_type = match self.sample_type {
            SampleType::U32 => 0,
            SampleType::F16 => 1,
            SampleType::F32 => 2,
        };

        out.push(
            ((csc_bits as u8) << 4) | ((scheme_bits as u8) << 2) | u8::from(self.case_insensitive),
        );
        out.push(sample_type);
        Ok(())
    }
}

/// Current OpenEXR encoder rules for version-2 chunks. Unlike the legacy
/// decoder fallback, these are case-sensitive and use canonical uppercase
/// channel suffixes.
fn default_channel_rules() -> Vec<Rule> {
    // OpenEXR's current encoder emits a small canonical rule table rather
    // than serializing the whole channel list. Only channels matching one of
    // these suffix/type pairs need to be recorded in the chunk header.
    let lossy: [(&'static str, Option<usize>); 6] =
        [("R", Some(0)), ("G", Some(1)), ("B", Some(2)), ("Y", None), ("BY", None), ("RY", None)];

    let mut rules = Vec::with_capacity(15);
    for (suffix, csc_index) in lossy {
        for sample_type in [SampleType::F16, SampleType::F32] {
            rules.push(Rule {
                suffix: Cow::Borrowed(suffix),
                scheme: CompressorScheme::LossyDct,
                sample_type,
                csc_index,
                case_insensitive: false,
            });
        }
    }
    for sample_type in [SampleType::U32, SampleType::F16, SampleType::F32] {
        rules.push(Rule {
            suffix: Cow::Borrowed("A"),
            scheme: CompressorScheme::Rle,
            sample_type,
            csc_index: None,
            case_insensitive: false,
        });
    }
    rules
}

/// Encoder-side companion to `parse_channel_rules`: writes a u16 byte count
/// including the size field itself, followed by only the default rules that
/// match at least one channel in this chunk's channel list.
fn write_relevant_channel_rules(rules: &[Rule], channels: &ChannelList) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    for rule in rules {
        let relevant = channels.list.iter().any(|channel| {
            let name = channel.name.to_string();
            rule.matches(channel_suffix(&name), channel.sample_type)
        });

        if relevant {
            payload.reserve(rule.serialized_size());
            rule.write(&mut payload)?;
        }
    }

    let total_size = payload
        .len()
        .checked_add(2)
        .ok_or_else(|| Error::invalid("DWA channel rules too large"))?;
    let total_size: u16 =
        total_size.try_into().map_err(|_| Error::invalid("DWA channel rules too large"))?;

    let mut out = Vec::with_capacity(total_size as usize);
    out.extend_from_slice(&total_size.to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// "sLegacyChannelRules", implied by chunk versions <2.
fn legacy_channel_rules() -> Vec<Rule> {
    let lossy: [(&'static str, Option<usize>); 11] = [
        ("r", Some(0)),
        ("red", Some(0)),
        ("g", Some(1)),
        ("grn", Some(1)),
        ("green", Some(1)),
        ("b", Some(2)),
        ("blu", Some(2)),
        ("blue", Some(2)),
        ("y", None),
        ("by", None),
        ("ry", None),
    ];

    let mut rules = Vec::with_capacity(25);
    for (suffix, csc_index) in lossy {
        for sample_type in [SampleType::F16, SampleType::F32] {
            rules.push(Rule {
                suffix: Cow::Borrowed(suffix),
                scheme: CompressorScheme::LossyDct,
                sample_type,
                csc_index,
                case_insensitive: true,
            });
        }
    }
    for sample_type in [SampleType::U32, SampleType::F16, SampleType::F32] {
        rules.push(Rule {
            suffix: Cow::Borrowed("a"),
            scheme: CompressorScheme::Rle,
            sample_type,
            csc_index: None,
            case_insensitive: true,
        });
    }
    rules
}

/// Version >= 2 chunks embed the rules they were encoded with, prefixed by
/// a u16 little-endian total size that includes the size field itself
/// (`DwaCompressor_readChannelRules`).
fn parse_channel_rules(input: &mut &[u8]) -> Result<Vec<Rule>> {
    let (size_bytes, rest) = input
        .split_first_chunk::<2>()
        .ok_or_else(|| Error::invalid("truncated DWA channel rules"))?;
    let total_size = u16::from_le_bytes(*size_bytes) as usize;

    let rules_size =
        total_size.checked_sub(2).ok_or_else(|| Error::invalid("truncated DWA channel rules"))?;
    if rules_size > rest.len() {
        return Err(Error::invalid("truncated DWA channel rules"));
    }

    let mut rules_data = &rest[..rules_size];
    *input = &rest[rules_size..];

    let mut rules = Vec::new();
    while !rules_data.is_empty() {
        rules.push(parse_rule(&mut rules_data)?);
    }
    Ok(rules)
}

/// One serialized rule ("Classifier_read"): a NUL-terminated suffix
/// (at most 128 chars), a packed flags byte, and a pixel type byte.
fn parse_rule(data: &mut &[u8]) -> Result<Rule> {
    let corrupt = || Error::invalid("corrupt DWA channel rule");

    let suffix_len = data.iter().position(|&byte| byte == 0).ok_or_else(corrupt)?;
    if suffix_len > 128 {
        return Err(corrupt());
    }
    let suffix = String::from_utf8_lossy(&data[..suffix_len]).into_owned();

    let rest = &data[suffix_len + 1..];
    let (chunk, rest) = rest.split_first_chunk::<2>().ok_or_else(corrupt)?;
    let [flags, type_byte] = *chunk;
    *data = rest;

    Ok(Rule {
        suffix: Cow::Owned(suffix),
        // flags layout: high nibble = cscIdx + 1, bits 2-3 = scheme, bit 0 = case-insensitive
        csc_index: match ((flags >> 4) as i32) - 1 {
            -1 => None,
            index @ 0..=2 => Some(index as usize),
            _ => {
                return Err(corrupt());
            }
        },
        scheme: match (flags >> 2) & 3 {
            0 => CompressorScheme::Unknown,
            1 => CompressorScheme::LossyDct,
            2 => CompressorScheme::Rle,
            _ => {
                return Err(corrupt());
            }
        },
        case_insensitive: (flags & 1) != 0,
        sample_type: match type_byte {
            0 => SampleType::U32,
            1 => SampleType::F16,
            2 => SampleType::F32,
            _ => {
                return Err(corrupt());
            }
        },
    })
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
    /// Channels marked linear (e.g. depth/Z) skip the nonlinear transfer
    /// curve before/after the DCT; see `channel_half_samples` and
    /// `decode_lossy_dct_group`.
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
        miniz_oxide::deflate::compress_to_vec_zlib(&unknown_planar, 9)
    };

    let ac_compression = AcCompression::StaticHuffman;
    let ac_compressed = if ac_values.is_empty() {
        Vec::new()
    } else {
        crate::compression::piz::huffman::compress(&ac_values)?
    };

    let dc_compressed = if dc_values.is_empty() {
        Vec::new()
    } else {
        let mut dc_bytes = u16s_to_le_bytes(&dc_values);
        zip_deconstruct_bytes(&mut dc_bytes);
        miniz_oxide::deflate::compress_to_vec_zlib(&dc_bytes, 9)
    };

    let (rle_uncompressed_size, rle_compressed) = if rle_raw.is_empty() {
        (0, Vec::new())
    } else {
        let rle_tokens = super::rle::pack_rle_tokens(&rle_raw);
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&rle_tokens, 9);
        (rle_tokens.len(), compressed)
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

    let mut out = Vec::with_capacity(
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
    Ok(out)
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

    // the writer stores chunks raw when compression would not have helped
    if compressed_le.len() == expected_byte_size {
        return crate::compression::convert_little_endian_to_current(
            compressed_le,
            channels,
            rectangle,
        );
    }

    let mut input = compressed_le.as_slice();

    let (header, rules, channel_infos, csc_groups) = timed!(profile::SETUP, {
        let header = DwaHeader::parse(&mut input)?;

        let rules = if header.version < 2 {
            legacy_channel_rules()
        } else {
            parse_channel_rules(&mut input)?
        };

        let (channel_infos, csc_groups) = classify_channels(channels, rectangle, &rules);
        Result::Ok((header, rules, channel_infos, csc_groups))
    })?;
    let _ = &rules;

    if channel_infos.iter().any(|info| {
        info.scheme == CompressorScheme::LossyDct && info.sample_type == SampleType::U32
    }) {
        return Err(Error::unsupported("DWA lossy DCT compression of u32 channels"));
    }

    let [unknown_section, ac_section, dc_section, rle_section] = split_sections(input, &header)?;

    let ac_packed = timed!(profile::ENTROPY_AC, decode_ac_section(ac_section, &header))?;

    let (unknown_planar, dc_packed, rle_planar) = timed!(profile::ENTROPY_OTHER, {
        let unknown_planar = decode_unknown_section(unknown_section, &header)?;
        let dc_packed = decode_dc_section(dc_section, &header)?;
        let rle_planar = decode_rle_section(rle_section, &header)?;
        Result::Ok((unknown_planar, dc_packed, rle_planar))
    })?;

    let (unknown_bytes, rle_bytes) = timed!(profile::PLANAR_SPLIT, {
        let unknown_bytes =
            split_planar_channels(&channel_infos, CompressorScheme::Unknown, &unknown_planar)?;
        let rle_bytes: Vec<Vec<u8>> =
            split_planar_channels(&channel_infos, CompressorScheme::Rle, &rle_planar)?
                .into_iter()
                .zip(&channel_infos)
                .map(|(planar, info)| interleave_byte_planes(&planar, info.bytes_per_sample))
                .collect();
        Result::Ok((unknown_bytes, rle_bytes))
    })?;

    let lossy_samples = decode_lossy_channels(&channel_infos, &csc_groups, &ac_packed, &dc_packed)?;

    let out = timed!(profile::WRITE, write_scanlines(
        channels,
        &channel_infos,
        rectangle,
        &lossy_samples,
        &unknown_bytes,
        &rle_bytes,
        expected_byte_size,
    ))?;

    crate::compression::convert_little_endian_to_current(out, channels, rectangle)
}

// --------------------- decoding ---------------------

/// Split the data after header + rules into the four sections, in on-disk
/// order. Errors on truncation like the C parser.
fn split_sections<'d>(data: &'d [u8], header: &DwaHeader) -> Result<[&'d [u8]; 4]> {
    let mut rest = data;
    let mut take = |length: usize| -> Result<&'d [u8]> {
        if length > rest.len() {
            return Err(Error::invalid("truncated DWA section"));
        }
        let (section, remaining) = rest.split_at(length);
        rest = remaining;
        Ok(section)
    };

    Ok([
        take(header.unknown_compressed_size)?,
        take(header.ac_compressed_size)?,
        take(header.dc_compressed_size)?,
        take(header.rle_compressed_size)?,
    ])
}

fn inflate(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    let options = zune_inflate::DeflateOptions::default()
        .set_limit(expected_size)
        .set_size_hint(expected_size);

    let inflated = zune_inflate::DeflateDecoder::new_with_options(compressed, options)
        .decode_zlib()
        .map_err(|_| Error::invalid("DWA zlib data malformed"))?;

    if inflated.len() != expected_size {
        return Err(Error::invalid("DWA zlib data size mismatch"));
    }
    Ok(inflated)
}

/// UNKNOWN section: raw (non-DCT-compressible) channel data,
/// zlib-compressed, planar in channel order.
fn decode_unknown_section(section: &[u8], header: &DwaHeader) -> Result<Vec<u8>> {
    if header.unknown_uncompressed_size == 0 {
        return Ok(vec![]);
    }
    inflate(section, header.unknown_uncompressed_size)
}

/// AC section: RLE DCT coefficients as u16, entropy coded with either the
/// PIZ static Huffman coder or zlib.
fn decode_ac_section(section: &[u8], header: &DwaHeader) -> Result<Vec<u16>> {
    if header.ac_count == 0 {
        return Ok(vec![]);
    }

    match header.ac_compression {
        AcCompression::StaticHuffman => {
            #[cfg(feature = "dwa-profile")]
            profile::AC_COUNT.add(header.ac_count as u64);
            crate::compression::piz::huffman::decompress(section, header.ac_count)
        }
        AcCompression::Deflate => {
            let bytes = inflate(section, header.ac_count * 2)?;
            Ok(bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect())
        }
    }
}

/// DC section: one u16 (half bits) per 8x8 block, zlib-compressed after
/// the "zip reconstruct" transform (differencing + byte deinterleave).
fn decode_dc_section(section: &[u8], header: &DwaHeader) -> Result<Vec<u16>> {
    if header.dc_count == 0 {
        return Ok(vec![]);
    }

    let bytes = inflate(section, header.dc_count * 2)?;
    let bytes = undo_zip_reconstruct(&bytes);
    Ok(bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect())
}

/// RLE section: zlib, then classic byte-oriented RLE. Result is planar per
/// channel, each channel further split into byte planes.
fn decode_rle_section(section: &[u8], header: &DwaHeader) -> Result<Vec<u8>> {
    if header.rle_raw_size == 0 {
        return Ok(vec![]);
    }
    let inflated = inflate(section, header.rle_uncompressed_size)?;
    super::rle::unpack_rle_tokens(&inflated, header.rle_raw_size, false)
}

/// Ports "internal_zip_reconstruct_bytes": undo differencing, then
/// interleave the two buffer halves.
fn undo_zip_reconstruct(source: &[u8]) -> Vec<u8> {
    if source.len() < 2 {
        return source.to_vec();
    }

    let mut deltas = source.to_vec();
    for index in 1..deltas.len() {
        deltas[index] = ((deltas[index - 1] as i32) + (deltas[index] as i32) - 128) as u8;
    }

    let (first_half, second_half) = deltas.split_at((deltas.len() + 1) / 2);
    let mut out = vec![0u8; deltas.len()];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = if index % 2 == 0 {
            first_half[index / 2]
        } else {
            second_half[index / 2]
        };
    }
    out
}

/// Split a planar buffer into one byte run per channel of the given scheme,
/// in channel order (mirrors "DwaCompressor_setupChannelData"s running
/// per-scheme cursor). Other schemes get an empty vec.
fn split_planar_channels(
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
fn interleave_byte_planes(planar: &[u8], bytes_per_sample: usize) -> Vec<u8> {
    let sample_count = planar.len() / bytes_per_sample;
    let mut interleaved = vec![0u8; planar.len()];
    for sample in 0..sample_count {
        for byte in 0..bytes_per_sample {
            interleaved[sample * bytes_per_sample + byte] = planar[byte * sample_count + sample];
        }
    }
    interleaved
}

// --------------------- lossy DCT decoding ---------------------

/// One of the chunk-global u16 streams (AC or DC). All channel groups of a
/// chunk consume the same stream, so the cursor carries across groups.
struct PackedStream<'v> {
    values: &'v [u16],
    cursor: usize,
}

impl<'v> PackedStream<'v> {
    fn new(values: &'v [u16]) -> Self {
        Self {
            values,
            cursor: 0,
        }
    }

    fn next(&mut self) -> Option<u16> {
        let value = self.values.get(self.cursor).copied();
        self.cursor += 1;
        value
    }

    /// Value at "offset" past the cursor, without consuming (the DC stream
    /// is indexed planar per group and advanced once at group end).
    fn peek_at(&self, offset: usize) -> Option<u16> {
        self.values.get(self.cursor + offset).copied()
    }

    fn advance(&mut self, count: usize) {
        self.cursor += count;
    }
}

/// Decode all LOSSY_DCT channels: first every CSC group, then the
/// standalone channels, both in channel order - the order in which the
/// encoder appended them to the shared AC/DC streams.
fn decode_lossy_channels(
    infos: &[ChannelInfo],
    csc_groups: &[[usize; 3]],
    ac_packed: &[u16],
    dc_packed: &[u16],
) -> Result<Vec<Vec<f16>>> {
    let mut ac = PackedStream::new(ac_packed);
    let mut dc = PackedStream::new(dc_packed);

    let mut samples: Vec<Vec<f16>> = vec![vec![]; infos.len()];
    let mut grouped = vec![false; infos.len()];

    for &group in csc_groups {
        // all three channels have identical sampling, hence identical size
        let info = &infos[group[0]];
        let mut decoded: [Vec<f16>; 3] =
            std::array::from_fn(|_| vec![f16::ZERO; info.width * info.height]);

        // CSC'd R/G/B triplets are always stored nonlinear, regardless of
        // `quantize_linearly` (that flag only applies to standalone channels
        // like depth/Z).
        decode_lossy_dct_group(
            &mut ac,
            &mut dc,
            info.width,
            info.height,
            Some(to_linear_table()),
            &mut decoded,
        )?;

        for (&channel, channel_samples) in group.iter().zip(decoded) {
            samples[channel] = channel_samples;
            grouped[channel] = true;
        }
    }

    for (index, info) in infos.iter().enumerate() {
        if grouped[index] || info.scheme != CompressorScheme::LossyDct {
            continue;
        }
        let mut decoded = [vec![f16::ZERO; info.width * info.height]];
        let to_linear = (!info.quantize_linearly).then(to_linear_table);
        decode_lossy_dct_group(&mut ac, &mut dc, info.width, info.height, to_linear, &mut decoded)?;

        let [channel_samples] = decoded;
        samples[index] = channel_samples;
    }

    Ok(samples)
}

/// Decode one standalone channel (decoded.len() == 1) or one CSC'd R/G/B
/// triplet (decoded.len() == 3): per 8x8 block and component, read the
/// DC value, un-RLE the AC values, inverse-DCT
fn decode_lossy_dct_group(
    ac: &mut PackedStream<'_>,
    dc: &mut PackedStream<'_>,
    width: usize,
    height: usize,
    to_linear: Option<&[u16; 65536]>,
    decoded: &mut [Vec<f16>],
) -> Result<()> {
    let components = decoded.len();
    let blocks_x = (width + 7) / 8;
    let blocks_y = (height + 7) / 8;
    let block_count = blocks_x * blocks_y;

    // Buffered for the whole group, rather than one block at a time, so the
    // inverse DCT below can run as a single batch over every block that
    // needs it (see idct::dct_inverse_8x8_batch's doc comment).
    let mut dct_blocks: Vec<[[f32; 64]; 3]> = vec![[[0.0f32; 64]; 3]; block_count];
    let mut needs_idct: Vec<[bool; 3]> = vec![[false; 3]; block_count];

    timed!(profile::DEQUANT, {
        for block_y in 0..blocks_y {
            for block_x in 0..blocks_x {
                let block_index = block_y * blocks_x + block_x;

                for component in 0..components {
                    let mut zig_block = [0u16; 64];

                    // the DC stream is planar: all of component 0's blocks,
                    // then all of component 1's, ...
                    zig_block[0] = dc
                        .peek_at(component * block_count + block_index)
                        .ok_or_else(|| Error::invalid("truncated DWA DC data"))?;

                    let last_non_zero = un_rle_ac(ac, &mut zig_block)?;

                    let dct_block = &mut dct_blocks[block_index][component];
                    if last_non_zero == 0 {
                        // DC-only block
                        dct_block[0] = f16::from_bits(zig_block[0]).to_f32();
                        idct::dct_inverse_8x8_dc_only(dct_block);
                    } else {
                        from_half_zigzag(&zig_block, dct_block);
                        needs_idct[block_index][component] = true;
                    }
                }
            }
        }
        Result::Ok(())
    })?;

    timed!(profile::IDCT, idct::dct_inverse_8x8_batch(
        dct_blocks
            .iter_mut()
            .zip(needs_idct.iter())
            .flat_map(|(blocks, flags)| blocks.iter_mut().zip(flags.iter()))
            .filter_map(|(block, &needed)| needed.then_some(block)),
    ));

    timed!(profile::CSC_LUT, {
    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            let block_index = block_y * blocks_x + block_x;
            let dct_blocks = &mut dct_blocks[block_index];

            if components == 3 {
                for i in 0..64 {
                    let (r, g, b) =
                        csc::csc709_inverse(dct_blocks[0][i], dct_blocks[1][i], dct_blocks[2][i]);
                    dct_blocks[0][i] = r;
                    dct_blocks[1][i] = g;
                    dct_blocks[2][i] = b;
                }
            }

            // nonlinear float -> linear half via LUT, cropped at the image edge
            for (component, output) in decoded.iter_mut().enumerate() {
                for y in block_y * 8..(block_y * 8 + 8).min(height) {
                    for x in block_x * 8..(block_x * 8 + 8).min(width) {
                        let value =
                            dct_blocks[component][(y - block_y * 8) * 8 + (x - block_x * 8)];
                        let nonlinear = f16::from_f32(value);
                        output[y * width + x] = f16::from_bits(match to_linear {
                            Some(to_linear) => to_linear[nonlinear.to_bits() as usize],
                            None => nonlinear.to_bits(),
                        });
                    }
                }
            }
        }
    }
    });

    dc.advance(components * block_count);
    Ok(())
}

/// Un-RLE one 8x8 block of AC values into block[1..]
/// (`LossyDctDecoder_unRleAc`): a value with high byte 0xff encodes a run
/// of `low byte` zeros (0 meaning "rest of the block"); anything else is a
/// literal. Returns the index of the last non-zero value, 0 if none
fn un_rle_ac(ac: &mut PackedStream<'_>, block: &mut [u16; 64]) -> Result<usize> {
    let mut last_non_zero = 0;
    let mut position = 1;

    while position < 64 {
        let value = ac.next().ok_or_else(|| Error::invalid("truncated DWA AC data"))?;

        if (value & 0xff00) == 0xff00 {
            // run of zeros - the block is pre-zeroed, just skip ahead
            let count = (value & 0xff) as usize;
            position += if count == 0 {
                64
            } else {
                count
            };
        } else {
            last_non_zero = position;
            block[position] = value;
            position += 1;
        }
    }

    Ok(last_non_zero)
}

/// Undo the zig-zag coefficient order (C "fromHalfZigZag_scalar"),
/// converting half bits to f32.
fn from_half_zigzag(zig_zag: &[u16; 64], dst: &mut [f32; 64]) {
    const SRC_INDICES: [usize; 64] = [
        0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9,
        11, 18, 24, 31, 40, 44, 53, 10, 19, 23, 32, 39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60,
        21, 34, 37, 47, 50, 56, 59, 61, 35, 36, 48, 49, 57, 58, 62, 63,
    ];

    for (slot, &src_index) in dst.iter_mut().zip(SRC_INDICES.iter()) {
        *slot = f16::from_bits(zig_zag[src_index]).to_f32();
    }
}

/// The stored nonlinear --> linear lookup table for all half bit patterns
fn to_linear_table() -> &'static [u16; 65536] {
    static TABLE: OnceLock<[u16; 65536]> = OnceLock::new();

    TABLE.get_or_init(|| {
        let mut table = [0u16; 65536];
        for (bits, slot) in table.iter_mut().enumerate() {
            *slot = dwa_convert_to_linear(f16::from_bits(bits as u16)).to_bits();
        }
        table
    })
}

fn dwa_convert_to_linear(x: f16) -> f16 {
    let value = x.to_f32();
    if !value.is_finite() {
        return f16::ZERO;
    }

    let sign = if value < 0.0 {
        -1.0
    } else {
        1.0
    };
    let value = value.abs();

    let linear = if value <= 1.0 {
        value.powf(2.2)
    } else {
        // exp(2.2) ^ (value - 1) == exp(2.2 * (value - 1))
        (9.02501329156_f32).powf(value - 1.0)
    };

    f16::from_f32(sign * linear)
}

/// The stored linear --> nonlinear lookup table for all half bit patterns;
/// the encoder-side inverse of `to_linear_table`.
fn to_nonlinear_table() -> &'static [u16; 65536] {
    static TABLE: OnceLock<[u16; 65536]> = OnceLock::new();

    TABLE.get_or_init(|| {
        let mut table = [0u16; 65536];
        for (bits, slot) in table.iter_mut().enumerate() {
            *slot = dwa_convert_to_nonlinear(f16::from_bits(bits as u16)).to_bits();
        }
        table
    })
}

fn dwa_convert_to_nonlinear(x: f16) -> f16 {
    // Inverse of `dwa_convert_to_linear`.
    let value = x.to_f32();
    if !value.is_finite() {
        return f16::ZERO;
    }

    let sign = if value < 0.0 {
        -1.0
    } else {
        1.0
    };
    let value = value.abs();

    let nonlinear = if value <= 1.0 {
        value.powf(1.0 / 2.2)
    } else {
        1.0 + value.ln() / 9.02501329156_f32.ln()
    };

    f16::from_f32(sign * nonlinear)
}

// --------------------- output layout ---------------------

/// Interleave the per-channel decoded data into the scanline layout the
/// rest of exrs expects: rows of "y" ascending, channels in list order
/// within each row, samples little-endian.
fn write_scanlines(
    channels: &ChannelList,
    infos: &[ChannelInfo],
    rectangle: IntegerBounds,
    lossy_samples: &[Vec<f16>],
    unknown_bytes: &[Vec<u8>],
    rle_bytes: &[Vec<u8>],
    expected_byte_size: usize,
) -> Result<ByteVec> {
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

// --------------------- encoding: channel layout ---------------------

/// The encoder-side inverse of `write_scanlines`: slice the interleaved
/// scanline buffer back into one byte run per channel, so the per-scheme
/// packing below matches the C reference's running cursors.
fn split_scanline_channels(
    data: &[u8],
    channels: &ChannelList,
    infos: &[ChannelInfo],
    rectangle: IntegerBounds,
) -> Result<Vec<Vec<u8>>> {
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

/// UNKNOWN channels stay planar and are concatenated in channel order
/// before the zlib step.
fn pack_unknown_channels(
    infos: &[ChannelInfo],
    channel_bytes: &[Vec<u8>],
    scheme: CompressorScheme,
) -> Vec<u8> {
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

/// RLE channels are repacked into byte planes first, then byte-RLE'd and
/// zlib-compressed by the caller.
fn pack_rle_channels(infos: &[ChannelInfo], channel_bytes: &[Vec<u8>]) -> Vec<u8> {
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

fn u16s_to_le_bytes(values: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 2);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// Encoder-side companion applied to the DC byte stream before zlib:
/// byte-fragment separation followed by successive differencing.
fn zip_deconstruct_bytes(bytes: &mut [u8]) {
    crate::compression::optimize_bytes::separate_bytes_fragments(bytes);
    crate::compression::optimize_bytes::samples_to_differences(bytes);
}

// --------------------- lossy DCT encoding ---------------------

fn encode_lossy_channels(
    infos: &[ChannelInfo],
    csc_groups: &[[usize; 3]],
    channel_bytes: &[Vec<u8>],
    quant_base_error: f32,
) -> Result<(Vec<u16>, Vec<u16>)> {
    // Lossy chunks use shared AC/DC streams. CSC triplets consume the streams
    // first, then standalone LOSSY_DCT channels continue from the same cursors.
    let mut ac = Vec::new();
    let mut dc = Vec::new();
    let mut grouped = vec![false; infos.len()];

    for &group in csc_groups {
        let info = &infos[group[0]];
        let components = group
            .iter()
            .map(|&channel| channel_half_samples(&channel_bytes[channel], &infos[channel], true))
            .collect::<Result<Vec<_>>>()?;

        encode_lossy_dct_group(
            &components,
            info.width,
            info.height,
            quant_base_error,
            &mut ac,
            &mut dc,
        )?;

        for &channel in &group {
            grouped[channel] = true;
        }
    }

    for (index, info) in infos.iter().enumerate() {
        if grouped[index] || info.scheme != CompressorScheme::LossyDct {
            continue;
        }

        let apply_nonlinear = !info.quantize_linearly;
        let samples = channel_half_samples(&channel_bytes[index], info, apply_nonlinear)?;
        encode_lossy_dct_group(
            &[samples],
            info.width,
            info.height,
            quant_base_error,
            &mut ac,
            &mut dc,
        )?;
    }

    Ok((ac, dc))
}

fn channel_half_samples(
    bytes: &[u8],
    info: &ChannelInfo,
    apply_nonlinear: bool,
) -> Result<Vec<u16>> {
    // OpenEXR stores lossy input as half precision internally before DCT.
    // F32 channels are clamped to the finite half range and demoted here.
    let mut samples = Vec::with_capacity(info.width * info.height);

    match info.sample_type {
        SampleType::F16 => {
            let chunks = bytes.chunks_exact(2);
            if !chunks.remainder().is_empty() {
                return Err(Error::invalid("DWA f16 channel data size"));
            }
            samples.extend(chunks.map(|pair| u16::from_le_bytes([pair[0], pair[1]])));
        }
        SampleType::F32 => {
            let chunks = bytes.chunks_exact(4);
            if !chunks.remainder().is_empty() {
                return Err(Error::invalid("DWA f32 channel data size"));
            }
            samples.extend(chunks.map(|quad| {
                let mut value = f32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]);
                value = value.clamp(-65504.0, 65504.0);
                f16::from_f32(value).to_bits()
            }));
        }
        SampleType::U32 => {
            return Err(Error::unsupported("DWA lossy DCT compression of u32 channels"));
        }
    }

    if samples.len() != info.width * info.height {
        return Err(Error::invalid("DWA lossy channel data size mismatch"));
    }

    if apply_nonlinear {
        let to_nonlinear = to_nonlinear_table();
        for sample in &mut samples {
            *sample = to_nonlinear[*sample as usize];
        }
    }

    Ok(samples)
}

fn encode_lossy_dct_group(
    components: &[Vec<u16>],
    width: usize,
    height: usize,
    quant_base_error: f32,
    ac: &mut Vec<u16>,
    dc: &mut Vec<u16>,
) -> Result<()> {
    if width == 0 || height == 0 {
        return Ok(());
    }

    let component_count = components.len();
    if component_count != 1 && component_count != 3 {
        return Err(Error::invalid("invalid DWA lossy component count"));
    }

    for component in components {
        if component.len() != width * height {
            return Err(Error::invalid("DWA lossy component size mismatch"));
        }
    }

    // Mirror the source block edges so partial 8x8 blocks behave the same way
    // as the reference encoder.
    let quant_tables = quantize::QuantTables::new(quant_base_error);
    let blocks_x = (width + 7) / 8;
    let blocks_y = (height + 7) / 8;
    let block_count = blocks_x * blocks_y;
    let mut group_dc: Vec<Vec<u16>> =
        (0..component_count).map(|_| Vec::with_capacity(block_count)).collect();

    let mut row_blocks: Vec<[[f32; 64]; 3]> = vec![[[0.0; 64]; 3]; blocks_x];

    for block_y in 0..blocks_y {
        for block_x in 0..blocks_x {
            for component_index in 0..component_count {
                let block = &mut row_blocks[block_x][component_index];
                for y in 0..8 {
                    let src_y = mirror_index(block_y * 8 + y, height);
                    for x in 0..8 {
                        let src_x = mirror_index(block_x * 8 + x, width);
                        let bits = components[component_index][src_y * width + src_x];
                        block[y * 8 + x] = f16::from_bits(bits).to_f32();
                    }
                }
            }

            if component_count == 3 {
                // CSC is performed in nonlinear space for the RGB triplet.
                let dct_blocks = &mut row_blocks[block_x];
                for i in 0..64 {
                    let (y, by, ry) =
                        csc::csc709_forward(dct_blocks[0][i], dct_blocks[1][i], dct_blocks[2][i]);
                    dct_blocks[0][i] = y;
                    dct_blocks[1][i] = by;
                    dct_blocks[2][i] = ry;
                }
            }
        }

        idct::dct_forward_8x8_batch(
            row_blocks.iter_mut().flat_map(|blocks| blocks[..component_count].iter_mut()),
        );

        for block_x in 0..blocks_x {
            for component_index in 0..component_count {
                let block = &mut row_blocks[block_x][component_index];
                let (tolerances, half_tolerances) = if component_index == 0 {
                    (&quant_tables.y, &quant_tables.half_y)
                } else {
                    (&quant_tables.cbcr, &quant_tables.half_cbcr)
                };

                let half_zig =
                    quantize::quantize_coefficients_to_zigzag(block, tolerances, half_tolerances);
                group_dc[component_index].push(half_zig[0]);
                rle_ac(&half_zig, ac);
            }
        }
    }

    for component_dc in group_dc {
        dc.extend(component_dc);
    }

    Ok(())
}

fn mirror_index(index: usize, length: usize) -> usize {
    // The C encoder mirrors out-of-bounds coordinates back into the image
    // rather than clamping them. This keeps edge blocks symmetrical.
    debug_assert_ne!(length, 0);
    let mut value = index as isize;
    let length = length as isize;

    if value >= length {
        value = length - (value - (length - 1));
    }
    if value < 0 {
        value = length - 1;
    }

    value as usize
}

/// The AC stream uses a simple token format: literals are emitted as-is,
/// and runs of zeroes are encoded as 0xffxx tokens. 0xff00 marks EOB.
fn rle_ac(block: &[u16; 64], ac: &mut Vec<u16>) {
    let mut dct_comp = 1;

    while dct_comp < 64 {
        if block[dct_comp] != 0 {
            ac.push(block[dct_comp]);
            dct_comp += 1;
            continue;
        }

        let mut run_len = 1;
        while dct_comp + run_len < 64 && block[dct_comp + run_len] == 0 {
            run_len += 1;
        }

        if run_len == 1 {
            ac.push(block[dct_comp]);
        } else if run_len + dct_comp == 64 {
            ac.push(0xff00);
        } else {
            ac.push(0xff00 | run_len as u16);
        }

        dct_comp += run_len;
    }
}
