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

    let unknown_planar = decode_unknown_section(unknown_section, &header)?;
    let ac_packed = decode_ac_section(ac_section, &header)?;
    let dc_packed = decode_dc_section(dc_section, &header)?;
    let rle_planar = decode_rle_section(rle_section, &header)?;

    let unknown_bytes =
        split_planar_channels(&channel_infos, CompressorScheme::Unknown, &unknown_planar)?;
    let rle_bytes: Vec<Vec<u8>> =
        split_planar_channels(&channel_infos, CompressorScheme::Rle, &rle_planar)?
            .into_iter()
            .zip(&channel_infos)
            .map(|(planar, info)| interleave_byte_planes(&planar, info.bytes_per_sample))
            .collect();

    let lossy_samples = decode_lossy_channels(&channel_infos, &csc_groups, &ac_packed, &dc_packed)?;

    let out = write_scanlines(
        channels,
        &channel_infos,
        rectangle,
        &lossy_samples,
        &unknown_bytes,
        &rle_bytes,
        expected_byte_size,
    )?;

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
    let to_linear = to_linear_table();
    let mut ac = PackedStream::new(ac_packed);
    let mut dc = PackedStream::new(dc_packed);

    let mut samples: Vec<Vec<f16>> = vec![vec![]; infos.len()];
    let mut grouped = vec![false; infos.len()];

    for &group in csc_groups {
        // all three channels have identical sampling, hence identical size
        let info = &infos[group[0]];
        let mut decoded: [Vec<f16>; 3] =
            std::array::from_fn(|_| vec![f16::ZERO; info.width * info.height]);

        decode_lossy_dct_group(&mut ac, &mut dc, info.width, info.height, to_linear, &mut decoded)?;

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
    to_linear: &[u16; 65536],
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

    idct::dct_inverse_8x8_batch(
        dct_blocks
            .iter_mut()
            .zip(needs_idct.iter())
            .flat_map(|(blocks, flags)| blocks.iter_mut().zip(flags.iter()))
            .filter_map(|(block, &needed)| needed.then_some(block)),
    );

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
                        output[y * width + x] =
                            f16::from_bits(to_linear[nonlinear.to_bits() as usize]);
                    }
                }
            }
        }
    }

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
