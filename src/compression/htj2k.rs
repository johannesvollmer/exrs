//! High-Throughput JPEG 2000 (HTJ2K, ITU-T T.814 | ISO/IEC 15444-15), as
//! `OpenEXR` writes it for `HTJ2K32` and `HTJ2K256`, which are lossless, and
//! for `LJ2K`, which is lossy.
//!
//! Each chunk holds one HTJ2K codestream, which the `htj2k` crate encodes
//! and decodes, behind a short header that maps the codestream's components
//! to the channels of the chunk. This follows `ht_apply_impl` and
//! `ht_undo_impl` in `OpenEXR`'s `internal_ht.cpp`, and `make_channel_map`,
//! `write_header` and `read_header` in `internal_ht_common.cpp`, so that the
//! chunks exrs writes are byte for byte those `OpenEXR` 3.5 writes, but for
//! the `OpenJPH` version that the codestream's first comment names.
//!
//! The chunk header, all fields big-endian:
//!
//! ```text
//! u16  magic                      "HT", or "HL" for LJ2K
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
//! The codestream lists R, G and B first when the channels include such a
//! triplet of one type and sampling, and then uses the reversible colour
//! transform on them, unless a channel is subsampled. Subsampled channels
//! make the codestream planar: each component is coded with its channel's
//! sampling, one after the other.
//!
//! `LJ2K` codes such an R, G and B triplet lossily when its channels are
//! HALF or FLOAT and not subsampled: with the irreversible 9/7 wavelet and
//! colour transforms, and NLT type 4 tables that map the bits of the
//! samples through a transfer function. The tables are in the codestream,
//! so the decoder needs nothing else. Its other channels are coded as
//! above. A lossy codestream not coded planar is padded to a size of 1
//! modulo 32 each way (see [`padded_length`]), and the padding is dropped
//! when decoding.
//!
//! `OpenEXR` 3.5.0 and 3.5.1 wrote `LJ2K` chunks with the "HT" magic number,
//! and coded their lossy components without NLT tables: they mapped each
//! sample through the transfer function to a signed integer of the
//! component's depth before coding. Like `OpenEXR` 3.5.2 and later, the
//! decoder reads them and undoes that mapping on the lossy components
//! after decoding the codestream (see [`write_legacy_samples`]). exrs
//! writes only the later form.

use std::{borrow::Cow, convert::TryFrom, io::Write};

use ::htj2k::{codestream::Encoder, params::nonlinearity, types::Point};

use super::*;
use crate::{math::Vec2, meta::attribute::ChannelDescription};

/// "HT", the magic number of `HTJ2K32` and `HTJ2K256` chunks
/// (`HeaderMagic::V1` in `OpenEXR`). `OpenEXR` 3.5.0 and 3.5.1 also wrote it
/// on `LJ2K` chunks, which they coded with the transfer function outside the
/// codestream.
const MAGIC: u16 = u16::from_be_bytes(*b"HT");

/// "HL", the magic number of `LJ2K` chunks as `OpenEXR` writes them after
/// 3.5.1 (`HeaderMagic::V2`), with the transfer function in NLT type 4
/// tables of the codestream.
const MAGIC_LJ2K: u16 = u16::from_be_bytes(*b"HL");

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

/// The comment `OpenJPH` writes at the start of every codestream, which names
/// its version. It is informative only: no reader looks at it. exrs names
/// `OpenJPH` 0.33.0, the version the `htj2k` crate ports, in the form
/// `OpenJPH` writes it, so the comment says which coder the codestream
/// follows. (The `htj2k` crate's own default names the crate.) `OpenEXR`
/// builds linked with another `OpenJPH`, such as 0.32.0, name that one, and
/// their chunks differ from exrs's in these bytes.
const OPENJPH_VERSION_COMMENT: &str = "OpenJPH Ver 0.33.0.";

/// The wavelet decomposition levels `OpenEXR` codes (`HT_NUM_DECOMPS`).
const DECOMPOSITION_LEVELS: u32 = 5;

/// The code-block size `OpenEXR` codes, width and height.
const BLOCK_SIZE: (u32, u32) = (128, 32);

/// Compresses one chunk of native-endian samples, in the usual layout (see
/// [`decompress`]), into little-endian bytes: the chunk header and an HTJ2K
/// codestream, coded as `OpenEXR`'s `ht_apply_impl` codes it.
///
/// The result may be larger than the samples; the caller stores the
/// samples instead then, as `OpenEXR` does. `None` asks the caller to store
/// them in any case: see [`must_store_raw`].
///
/// `chunk_lines` is the height of a full chunk: 32 for `HTJ2K32` and 256
/// for `HTJ2K256` and `LJ2K`, or the tile height. `lj2k_quality` is the
/// quality of `LJ2K`, and `None` for `HTJ2K32` and `HTJ2K256`.
///
/// Refuses channel sampling that is not a power of two, as `OpenEXR` fails
/// on it: the encoder cannot subsample by other factors with RPCL.
pub fn compress(
    channels: &ChannelList,
    uncompressed: &[u8],
    rectangle: IntegerBounds,
    chunk_lines: usize,
    lj2k_quality: Option<f32>,
) -> Result<Option<ByteVec>> {
    if let Some(quality) = lj2k_quality {
        Compression::LJ2K(Some(quality)).lj2k_quality()?;
    }
    for channel in &channels.list {
        let Vec2(x, y) = channel.sampling;
        if !x.is_power_of_two() || !y.is_power_of_two() {
            return Err(Error::unsupported(format!(
                "HTJ2K compression of channel {} sampled {}x{}: HTJ2K, as OpenEXR \
                 writes it, needs sampling factors that are powers of two",
                channel.name, x, y
            )));
        }
    }
    if must_store_raw(channels, chunk_lines) {
        return Ok(None);
    }

    let (layout, byte_size) = chunk_layout(channels, rectangle)?;
    if byte_size != uncompressed.len() {
        return Err(Error::invalid("uncompressed chunk size"));
    }

    let width = rectangle.size.width();
    let height = rectangle.size.height();

    // The codestream gives a component ceil(height / sampling) lines, as
    // the image offset is 0, which must be the lines of its channel. With
    // the sampling dividing the chunk height, they differ only if the data
    // window does not start on a multiple of the sampling, which is invalid.
    for (channel, channel_layout) in channels.list.iter().zip(&layout) {
        if channel_layout.lines.len() != height.div_ceil(channel.sampling.y()) {
            return Err(Error::invalid("data window not aligned to the channel sampling"));
        }
    }

    let (rgb, file_channels) = channel_map(channels);
    let planar = is_planar(channels);
    let lossy = lj2k_quality.and_then(|quality| {
        lossy_triplet(channels, &file_channels, rgb).then(|| Lossy {
            quality,
            max_linear: chunk_max_linear(channels, &file_channels, &layout, uncompressed),
        })
    });

    // A lossy chunk is padded to 1 modulo 32 each way, unless planar or too
    // large to pad, as `ht_apply_impl` does
    let (coded_width, coded_height) = match lossy {
        Some(_) if !planar => padded_length(width, DECOMPOSITION_LEVELS)
            .zip(padded_length(height, DECOMPOSITION_LEVELS))
            .unwrap_or((width, height)),
        _ => (width, height),
    };
    let padded = (coded_width, coded_height) != (width, height);
    let image_size = Point::new(
        u32::try_from(coded_width).map_err(|_| Error::unsupported("HTJ2K chunk width"))?,
        u32::try_from(coded_height).map_err(|_| Error::unsupported("HTJ2K chunk height"))?,
    );

    let mut compressed = Vec::with_capacity(uncompressed.len());
    let magic = if lj2k_quality.is_some() {
        MAGIC_LJ2K
    } else {
        MAGIC
    };
    write_chunk_header(&mut compressed, magic, &file_channels)?;

    let mut encoder = new_encoder(channels, &file_channels, rgb, planar, image_size, lossy)
        .map_err(encoder_error)?;

    // A padded codestream says so in a comment, so that it describes itself
    // when taken out of the file. No decoder needs it. The text is made
    // without allocating: the padding is at most 31 each way.
    let padding_comment = if padded {
        let mut text = [0_u8; 64];
        let rows = coded_height - height;
        let cols = coded_width - width;
        let mut cursor = &mut text[..];
        write!(cursor, "OpenEXR LJ2K padding: rows={rows} cols={cols}")
            .map_err(|_| Error::invalid("LJ2K padding comment"))?;
        let length = 64 - cursor.len();
        let text = std::str::from_utf8(&text[..length])
            .map_err(|_| Error::invalid("LJ2K padding comment"))?;
        let mut comment = ::htj2k::params::CommentExchange::new();
        comment.set_string(text).map_err(encoder_error)?;
        Some(comment)
    } else {
        None
    };
    encoder.write_headers(&mut compressed, padding_comment.as_slice()).map_err(encoder_error)?;

    // The encoder asks for the lines in the order it codes them: planar,
    // all the lines of one component after the other, or else one line of
    // each component in turn. The lines of padding repeat the last line of
    // the chunk, and the samples of padding the last sample of their line.
    let mut next_line = vec![0_usize; file_channels.len()];
    while let Some((component, line)) = encoder.exchange().map_err(encoder_error)? {
        let component = component as usize;
        let file_channel = file_channels[component];
        let sample_type = channels.list[file_channel].sample_type;
        let channel_layout = &layout[file_channel];

        let index = next_line[component];
        next_line[component] += 1;
        let index = if padded {
            index.min(channel_layout.lines.len().saturating_sub(1))
        } else {
            index
        };
        let start = channel_layout
            .lines
            .get(index)
            .copied()
            .ok_or_else(|| Error::invalid("HTJ2K encoder asked for too many lines"))?;

        let line_width = if padded {
            coded_width
        } else {
            channel_layout.width
        };
        if line.len() != line_width {
            return Err(Error::invalid("HTJ2K encoder line size"));
        }
        let (samples, padding) = line.split_at_mut(channel_layout.width);
        let size = sample_type.bytes_per_sample() * channel_layout.width;
        read_samples(samples, &uncompressed[start..start + size], sample_type);
        if let Some(&last) = samples.last() {
            padding.fill(last);
        }
    }
    encoder.flush(&mut compressed).map_err(encoder_error)?;

    Ok(Some(compressed))
}

/// Whether the chunks must be stored uncompressed because a channel's y
/// sampling is larger than the chunk height or does not divide it.
///
/// `OpenEXR` codes such chunks, but its encoder and decoder disagree on
/// their size: the codestream gives a channel ceil(height / sampling) lines
/// where the chunk may have none, and the decoder refuses a channel with
/// fewer than height / sampling lines, rounded down. So `OpenEXR` writes them
/// without error and then cannot read them, and some hold wrong samples.
/// Uncompressed chunks stay readable by every reader. With power-of-two
/// sampling, this is a y sampling of 64 or more with `HTJ2K32`, or 512 or
/// more with `HTJ2K256` and `LJ2K`.
fn must_store_raw(channels: &ChannelList, chunk_lines: usize) -> bool {
    channels.list.iter().any(|channel| {
        let sampling = channel.sampling.y();
        sampling > chunk_lines || chunk_lines % sampling != 0
    })
}

/// How `LJ2K` codes the lossy R, G and B components of a chunk.
#[derive(Clone, Copy, Debug)]
struct Lossy {
    /// From 1 to 150.
    quality: f32,

    /// The largest magnitude the NLT table of FLOAT samples spans: half's
    /// largest, or more if the chunk's samples exceed it. See
    /// [`chunk_max_linear`].
    max_linear: f64,
}

/// An encoder set up for one chunk with `ht_apply_impl`'s calls, in the
/// same order: 128x32 code blocks, 5 decomposition levels, RPCL, a TLM
/// marker and tile parts at each resolution, at image offset 0, and either
/// lossless (reversible, NLT type 3 on HALF and FLOAT channels) or, with
/// `lossy`, irreversible on components 0, 1 and 2 (NLT type 4) and
/// reversible on the others.
fn new_encoder(
    channels: &ChannelList,
    file_channels: &[usize],
    rgb: bool,
    planar: bool,
    image_size: Point,
    lossy: Option<Lossy>,
) -> ::htj2k::Result<Encoder> {
    let mut encoder = Encoder::new();
    encoder.set_version_comment(Some(OPENJPH_VERSION_COMMENT))?;

    // at most 65535 components, which `write_chunk_header` checks
    let component_count = u32::try_from(file_channels.len()).unwrap_or(u32::MAX);
    encoder.access_siz_mut()?.set_num_components(component_count);
    encoder.set_planar(planar)?;
    encoder.request_tlm_marker(true)?;
    encoder.set_tilepart_divisions(true, false)?;

    let cod = encoder.access_cod_mut()?;
    cod.set_color_transform(rgb && !planar);
    cod.set_block_dims(BLOCK_SIZE.0, BLOCK_SIZE.1)?;
    cod.set_num_decomposition(DECOMPOSITION_LEVELS)?;
    cod.set_progression_order("RPCL")?;

    let siz = encoder.access_siz_mut()?;
    siz.set_image_offset(Point::new(0, 0));
    siz.set_image_extent(image_size);

    let float_table = if let Some(lossy) = lossy {
        encoder.access_cod_mut()?.set_reversible(false);
        set_quantisation(&mut encoder, lossy, channels.list[file_channels[0]].sample_type)?;
        let cod = encoder.access_cod_mut()?;
        for component in 3..component_count {
            cod.set_reversible_comp(component, true);
        }
        Some(float_nlt_table(lossy.max_linear))
    } else {
        encoder.access_cod_mut()?.set_reversible(true);
        None
    };

    for (component, &file_channel) in (0_u32..).zip(file_channels) {
        let channel = &channels.list[file_channel];
        let is_float = channel.sample_type != SampleType::U32;
        if is_float && encoder.access_cod().is_reversible_comp(component) {
            encoder
                .access_nlt_mut()?
                .set_nonlinear_transform(component, nonlinearity::OJPH_NLT_BINARY_COMPLEMENT_NLT)?;
        } else if is_float {
            // the transfer function as a table: OpenEXR's 16-bit table for
            // HALF, and its 32-bit table for the chunk's range for FLOAT
            let (bits, table): (u8, &[u32]) = match channel.sample_type {
                SampleType::F16 => (16, half_nlt_table()),
                _ => (32, float_table.as_deref().unwrap_or_else(|| float_nlt_table_half_range())),
            };
            encoder.access_nlt_mut()?.set_nonlinear_transform_lut(
                component,
                bits,
                true,
                0,
                u32::MAX,
                bits,
                // the tables have 513 and 257 points
                u16::try_from(table.len()).unwrap_or(u16::MAX),
                table,
                nonlinearity::OJPH_NLT_BINARY_COMPLEMENT_PLUS_LUT,
                false,
                0,
            )?;
        }

        // sampling above u32::MAX becomes 0, which the encoder refuses
        let sampling = Point::new(
            u32::try_from(channel.sampling.x()).unwrap_or(0),
            u32::try_from(channel.sampling.y()).unwrap_or(0),
        );
        let bit_depth = if channel.sample_type == SampleType::F16 {
            16
        } else {
            32
        };
        encoder.access_siz_mut()?.set_component(component, sampling, bit_depth, is_float)?;
    }

    Ok(encoder)
}

/// Whether `LJ2K` codes components 0, 1 and 2 lossily, as `ht_apply_impl`
/// decides: they must be an R, G and B triplet (see [`channel_map`]) of
/// HALF or FLOAT channels that are not subsampled. (`OpenEXR` also requires
/// their names to be those of colours, which a triplet's always are.)
fn lossy_triplet(channels: &ChannelList, file_channels: &[usize], rgb: bool) -> bool {
    rgb && file_channels[..3].iter().all(|&file_channel| {
        let channel = &channels.list[file_channel];
        channel.sample_type != SampleType::U32 && channel.sampling == Vec2(1, 1)
    })
}

/// The quantisation of the lossy components for `quality`, as
/// `ht_apply_impl` sets it. Below 97 it is `OpenJPH`'s Qfactor, rounded to an
/// integer. From 97 to 150, which `OpenEXR` adds for its deeper samples, it
/// is an explicit step size for the luma component, and that step divided
/// by the colour transform's gain for each chroma component. For FLOAT
/// samples whose table spans more than half's range, the step is scaled so
/// that it means the same in the transfer function's domain.
// The casts and the arithmetic are `OpenEXR`'s, which this must reproduce
// bit for bit: a fused multiply-add or `exp2` would round differently.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]
fn set_quantisation(
    encoder: &mut Encoder,
    lossy: Lossy,
    sample_type: SampleType,
) -> ::htj2k::Result<()> {
    let quality = f64::from(lossy.quality);
    let qcd = encoder.access_qcd_mut()?;
    if quality < 97.0 {
        // `(ui8) std::lround (qfactor)`: halves away from zero
        qcd.set_qfactor(f32::from(quality.round() as u8))?;
    } else {
        let scaled = (quality - 97.0) / 53.0;
        let mut delta = 0.005 * 2_f64.powf(-28.478 * scaled) + 0.001 * 2_f64.powf(-10.534 * scaled);
        if sample_type == SampleType::F32 {
            delta *= tf_from_linear(HALF_MAX) / tf_from_linear(lossy.max_linear);
        }
        // luma, then the two chroma components, each in its own QCC marker
        qcd.set_irrev_quant(delta as f32);
        qcd.set_irrev_quant_comp(1, (delta / (3.2584_f64 / 3.0).sqrt()) as f32);
        qcd.set_irrev_quant_comp(2, (delta / (2.4756_f64 / 3.0).sqrt()) as f32);
    }
    Ok(())
}

/// `HALF_MAX`, the largest finite HALF value.
const HALF_MAX: f64 = 65504.0;

/// The range the NLT table of lossy FLOAT components spans for a chunk
/// (`chunk_max_linear_float` in `OpenEXR`): the largest finite magnitude of
/// the FLOAT samples of components 0, 1 and 2, with a margin of 5 %, if
/// that exceeds half's largest value, and otherwise half's largest value.
fn chunk_max_linear(
    channels: &ChannelList,
    file_channels: &[usize],
    layout: &[ChannelLayout],
    uncompressed: &[u8],
) -> f64 {
    let mut max = 0.0_f64;
    for &file_channel in &file_channels[..3.min(file_channels.len())] {
        if channels.list[file_channel].sample_type != SampleType::F32 {
            continue;
        }
        let channel_layout = &layout[file_channel];
        for &start in &channel_layout.lines {
            let line = &uncompressed[start..start + 4 * channel_layout.width];
            for bytes in line.chunks_exact(4) {
                let value = f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if value.is_finite() {
                    max = max.max(f64::from(value).abs());
                }
            }
        }
    }
    if max > HALF_MAX {
        max * 1.05
    } else {
        HALF_MAX
    }
}

/// `OpenEXR`'s NLT type 4 table for lossy HALF components, built once.
fn half_nlt_table() -> &'static [u32] {
    static TABLE: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| build_nlt_table_16(513))
}

/// `OpenEXR`'s NLT type 4 table for lossy FLOAT components within half's
/// range, which most chunks use, built once.
fn float_nlt_table_half_range() -> &'static [u32] {
    static TABLE: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| build_nlt_table_32(FLOAT_NLT_POINTS, HALF_MAX))
}

/// The points of `OpenEXR`'s FLOAT tables.
const FLOAT_NLT_POINTS: u32 = 257;

/// The NLT type 4 table for lossy FLOAT components spanning `max_linear`:
/// the shared one within half's range, or one built for the chunk.
fn float_nlt_table(max_linear: f64) -> Cow<'static, [u32]> {
    if max_linear > HALF_MAX {
        Cow::Owned(build_nlt_table_32(FLOAT_NLT_POINTS, max_linear))
    } else {
        Cow::Borrowed(float_nlt_table_half_range())
    }
}

/// `tf_from_linear` in `OpenEXR`: a power of 1 / 2.2 up to 1, logarithmic
/// above, odd. The C++ constants are `float` literals, so they are rounded
/// to `f32` before the arithmetic in `f64`.
fn tf_from_linear(x: f64) -> f64 {
    let sign = if x < 0.0 {
        -1.0
    } else {
        1.0
    };
    let v = x.abs();
    if v <= 1.0 {
        sign * v.powf(f64::from(1.0_f32 / 2.2_f32))
    } else {
        sign * (v.ln() / f64::from(2.2_f32) + 1.0)
    }
}

/// `tf_to_linear` in `OpenEXR`, the inverse of [`tf_from_linear`].
fn tf_to_linear(x: f64) -> f64 {
    let sign = if x < 0.0 {
        -1.0
    } else {
        1.0
    };
    let v = x.abs();
    if v <= 1.0 {
        sign * v.powf(f64::from(2.2_f32))
    } else {
        sign * (f64::from(2.2_f32) * (v - 1.0)).exp()
    }
}

/// Imath's `imath_float_to_half`: to nearest even, overflowing to
/// infinity. The tables and legacy `LJ2K` samples must use exactly this
/// rounding.
// The casts and the arithmetic are `OpenEXR`'s, which this must reproduce
// bit for bit: a fused multiply-add or `exp2` would round differently.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]
fn float_to_half_bits(f: f32) -> u16 {
    let i = f.to_bits();
    let mut ui = i & !0x8000_0000;
    let sign = ((i >> 16) & 0x8000) as u16;
    if ui >= 0x3880_0000 {
        if ui >= 0x7f80_0000 {
            let infinity = sign | 0x7c00;
            if ui == 0x7f80_0000 {
                return infinity;
            }
            let mantissa = ((ui & 0x7f_ffff) >> 13) as u16;
            return infinity | mantissa | u16::from(mantissa == 0);
        }
        if ui > 0x477f_efff {
            return sign | 0x7c00;
        }
        ui -= 0x3800_0000;
        ui = (ui + 0x0000_0fff + ((ui >> 13) & 1)) >> 13;
        return sign | ui as u16;
    }
    if ui < 0x3300_0001 {
        return sign;
    }
    let exponent = ui >> 23;
    let shift = 0x7e - exponent;
    let mantissa = 0x80_0000 | (ui & 0x7f_ffff);
    let rest = mantissa << (32 - shift);
    let mut half = sign | (mantissa >> shift) as u16;
    if rest > 0x8000_0000 || (rest == 0x8000_0000 && (half & 1) != 0) {
        half += 1;
    }
    half
}

/// `nlt_smag` in `OpenEXR`: the binary complement of NLT type 3, from the
/// bits of a sample as a signed integer to sign and magnitude.
const fn nlt_sign_magnitude(value: i64, bit_depth: u32) -> i64 {
    if value >= 0 {
        value
    } else {
        -value - ((1_i64 << (bit_depth - 1)) + 1)
    }
}

/// `build_nlt_lut_16` in `OpenEXR`: the HALF bits of `points` values of the
/// transfer function, evenly spaced over the 16-bit range, mapped to
/// unsigned table entries.
// The casts and the arithmetic are `OpenEXR`'s, which this must reproduce
// bit for bit: a fused multiply-add or `exp2` would round differently.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]
fn build_nlt_table_16(points: u32) -> Vec<u32> {
    const LIMIT: f64 = 65536.0;
    let factor = 32767.0 / tf_from_linear(HALF_MAX);
    let delta = 2.0 * 32767.0 / f64::from(points - 1);
    (0..points)
        .map(|index| {
            let linear = tf_to_linear((-32767.0 + f64::from(index) * delta) / factor);
            let bits = i64::from(float_to_half_bits(linear as f32) as i16);
            let z = nlt_sign_magnitude(bits, 16) as f64 / LIMIT + 0.5;
            // `llround`, then truncated to the table's 16 bits
            u32::from((z * (LIMIT - 1.0)).round() as i64 as u16)
        })
        .collect()
}

/// `build_nlt_lut_32` in `OpenEXR`: as [`build_nlt_table_16`] for FLOAT
/// bits, spanning up to `max_linear`.
// The casts and the arithmetic are `OpenEXR`'s, which this must reproduce
// bit for bit: a fused multiply-add or `exp2` would round differently.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]
fn build_nlt_table_32(points: u32, max_linear: f64) -> Vec<u32> {
    const LIMIT: f64 = 4_294_967_296.0;
    let range = f64::from(i32::MAX);
    let factor = range / tf_from_linear(max_linear);
    let delta = 2.0 * range / f64::from(points - 1);
    (0..points)
        .map(|index| {
            let linear = tf_to_linear((-range + f64::from(index) * delta) / factor);
            let bits = i64::from((linear as f32).to_bits() as i32);
            let z = nlt_sign_magnitude(bits, 32) as f64 / LIMIT + 0.5;
            (z * (LIMIT - 1.0)).round() as i64 as u32
        })
        .collect()
}

/// The codestream component order, `make_channel_map` in `OpenEXR`: the file
/// channel of each component, and whether components 0, 1 and 2 are an R, G
/// and B triplet, which the colour transform can then decorrelate.
///
/// A triplet is "R", "G" and "B", or "red", "green" and "blue", in any case
/// and file order, either as whole names or after the last "." of names
/// with one prefix, and of one sample type and sampling. Without one, the
/// components are the channels in file order.
fn channel_map(channels: &ChannelList) -> (bool, Vec<usize>) {
    struct Triplet<'n> {
        names: [&'static [u8]; 3],
        channels: [Option<usize>; 3],
        prefix: Option<&'n [u8]>,
    }

    let mut triplets = [
        Triplet {
            names: [b"r", b"g", b"b"],
            channels: [None; 3],
            prefix: None,
        },
        Triplet {
            names: [b"red", b"green", b"blue"],
            channels: [None; 3],
            prefix: None,
        },
    ];

    // Each channel goes to the first triplet whose prefix it shares (any,
    // until the triplet has a channel) and that still lacks its colour.
    for (index, channel) in channels.list.iter().enumerate() {
        let name = channel.name.bytes();
        let (prefix, suffix) = name
            .iter()
            .rposition(|&byte| byte == b'.')
            .map_or((&name[..0], name), |dot| (&name[..dot], &name[dot + 1..]));

        for triplet in &mut triplets {
            if triplet.prefix.is_some_and(|triplet_prefix| triplet_prefix != prefix) {
                continue;
            }
            let colour = (0..3).find(|&colour| {
                triplet.channels[colour].is_none()
                    && suffix.eq_ignore_ascii_case(triplet.names[colour])
            });
            if let Some(colour) = colour {
                triplet.channels[colour] = Some(index);
                triplet.prefix = Some(prefix);
                break;
            }
        }
    }

    let rgb = triplets.iter().find_map(|triplet| match triplet.channels {
        [Some(r), Some(g), Some(b)] => {
            let [r_channel, g_channel, b_channel] = [r, g, b].map(|index| &channels.list[index]);
            let alike = |other: &ChannelDescription| {
                other.sample_type == r_channel.sample_type && other.sampling == r_channel.sampling
            };
            (alike(g_channel) && alike(b_channel)).then_some([r, g, b])
        }
        _ => None,
    });

    let all = 0..channels.list.len();
    rgb.map_or_else(
        || (false, all.clone().collect()),
        |rgb| {
            let others = all.clone().filter(|index| !rgb.contains(index));
            (true, rgb.iter().copied().chain(others).collect())
        },
    )
}

/// Appends the chunk header (see the module documentation) for components
/// coding `file_channels`.
fn write_chunk_header(target: &mut ByteVec, magic: u16, file_channels: &[usize]) -> UnitResult {
    let too_many = || Error::unsupported("HTJ2K compression of more than 65535 channels");
    let count = u16::try_from(file_channels.len()).map_err(|_| too_many())?;

    let payload_size = 2 + 2 * u32::from(count);
    target.extend_from_slice(&magic.to_be_bytes());
    target.extend_from_slice(&payload_size.to_be_bytes());
    target.extend_from_slice(&count.to_be_bytes());
    for &file_channel in file_channels {
        let index = u16::try_from(file_channel).map_err(|_| too_many())?;
        target.extend_from_slice(&index.to_be_bytes());
    }
    Ok(())
}

/// Loads native-endian samples of `sample_type` for the encoder: the bits
/// of each sample in an `i32`, a HALF sample sign-extended from 16 bits.
fn read_samples(target: &mut [i32], source: &[u8], sample_type: SampleType) {
    match sample_type {
        SampleType::F16 => {
            for (sample, bytes) in target.iter_mut().zip(source.chunks_exact(2)) {
                *sample = i32::from(i16::from_ne_bytes([bytes[0], bytes[1]]));
            }
        }
        SampleType::F32 | SampleType::U32 => {
            for (sample, bytes) in target.iter_mut().zip(source.chunks_exact(4)) {
                *sample = i32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            }
        }
    }
}

#[allow(clippy::needless_pass_by_value)] // for `map_err`
fn encoder_error(error: ::htj2k::Error) -> Error {
    Error::unsupported(format!("HTJ2K encoder: {}", error.message()))
}

/// Decompresses one chunk into a new buffer of native-endian samples, in the
/// usual layout: line by line, and within each line the channels in order.
///
/// `lj2k` tells `LJ2K` chunks from `HTJ2K32` and `HTJ2K256` ones, which
/// differ in their magic number.
pub fn decompress(
    channels: &ChannelList,
    compressed: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    lj2k: bool,
) -> Result<ByteVec> {
    let mut decompressed = vec![0_u8; expected_byte_size];
    decompress_into(channels, &compressed, rectangle, &mut decompressed, lj2k)?;
    Ok(decompressed)
}

/// Decompresses one chunk into `decompressed`, which must be exactly the
/// size of the chunk's samples. See [`decompress`].
fn decompress_into(
    channels: &ChannelList,
    compressed: &[u8],
    rectangle: IntegerBounds,
    decompressed: &mut [u8],
    lj2k: bool,
) -> UnitResult {
    let channel_count = channels.list.len();
    let width = rectangle.size.width();
    let height = rectangle.size.height();

    let (magic, file_channels, codestream) = read_chunk_header(compressed, channel_count)?;
    // An LJ2K chunk with the "HT" magic number is one of OpenEXR 3.5.0 or
    // 3.5.1, whose lossy components hold samples mapped through the legacy
    // transfer function (see `legacy_half_bits`)
    let legacy = match (magic, lj2k) {
        (MAGIC, false) | (MAGIC_LJ2K, true) => false,
        (MAGIC, true) => true,
        _ => return Err(Error::invalid("HTJ2K chunk header magic number")),
    };

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

    // The codestream is either exactly the chunk, or, coded lossily, the
    // chunk padded to the size `padded_length` gives (see there).
    let siz = decoder.access_siz();
    let extent = siz.get_image_extent();
    let offset = siz.get_image_offset();
    let image_size = extent.x.checked_sub(offset.x).zip(extent.y.checked_sub(offset.y));
    let Some((image_width, image_height)) = image_size.map(|(w, h)| (w as usize, h as usize))
    else {
        return Err(Error::invalid("HTJ2K image size does not match the chunk"));
    };
    if siz.get_num_components() as usize != channel_count {
        return Err(Error::invalid("HTJ2K component count does not match the channels"));
    }

    let planar = is_planar(channels);
    let padded = (image_width, image_height) != (width, height);
    if padded {
        // As `ht_undo_impl`: only an irreversible codestream may be padded,
        // with no more decomposition levels than OpenEXR codes, which bounds
        // the padding, and never with subsampled channels.
        let cod = decoder.access_cod();
        let levels = cod.get_num_decompositions();
        if cod.is_reversible()
            || planar
            || Some(image_width) != padded_length(width, levels)
            || Some(image_height) != padded_length(height, levels)
        {
            return Err(Error::invalid("HTJ2K image size does not match the chunk"));
        }
    }

    // Each component must have the sampling and the size of its channel.
    // OpenEXR codes a chunk at image offset 0, so a component has
    // ceil(size / sampling) samples each way, which is the size of the
    // channel when the chunk starts on a multiple of its sampling. In a
    // padded codestream no channel is subsampled, and every component has
    // the padded size.
    for (component, &file_channel) in (0_u32..).zip(&file_channels) {
        let sampling = siz.get_downsampling(component);
        let channel_layout = &layout[file_channel];
        let (component_width, component_height) = if padded {
            (image_width, image_height)
        } else {
            (channel_layout.width, channel_layout.lines.len())
        };
        if Vec2(sampling.x as usize, sampling.y as usize) != channels.list[file_channel].sampling
            || siz.get_recon_width(component) as usize != component_width
            || siz.get_recon_height(component) as usize != component_height
        {
            return Err(Error::invalid("HTJ2K component size does not match the chunk"));
        }
    }

    // OpenEXR codes subsampled channels one after the other, and otherwise
    // one line of each component in turn, which is one line of the chunk.
    decoder.set_planar(planar);
    decoder.create().map_err(codestream_error)?;

    // `line` is `None` for the lines of padding, which are dropped.
    let mut pull_line = |component: usize, line: Option<usize>| -> UnitResult {
        // as `ht_undo_impl`: by the coding of the component, whatever its
        // channel (`read_chunk_header` bounds the components to u16)
        let legacy_lossy = legacy
            && !decoder
                .access_cod()
                .is_reversible_comp(u32::try_from(component).unwrap_or(u32::MAX));
        let (pulled, samples) = decoder
            .pull()
            .map_err(codestream_error)?
            .ok_or_else(|| Error::invalid("HTJ2K codestream ended early"))?;
        if pulled as usize != component || !samples.is_i32() {
            return Err(Error::invalid("HTJ2K line order"));
        }
        let Some(line) = line else {
            return Ok(());
        };

        let file_channel = file_channels[component];
        let channel_layout = &layout[file_channel];
        let samples = samples
            .i32()
            .get(..channel_layout.width)
            .ok_or_else(|| Error::invalid("HTJ2K line size"))?;

        let sample_type = channels.list[file_channel].sample_type;
        let start = channel_layout.lines[line];
        let size = sample_type.bytes_per_sample() * channel_layout.width;
        let target = &mut decompressed[start..start + size];
        write_samples(target, samples, sample_type, legacy_lossy);
        Ok(())
    };

    if planar {
        for (component, &file_channel) in file_channels.iter().enumerate() {
            for line in 0..layout[file_channel].lines.len() {
                pull_line(component, Some(line))?;
            }
        }
    } else {
        for line in 0..image_height {
            let line = (line < height).then_some(line);
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

/// Reads the chunk header. Returns its magic number, the file channel of
/// each codestream component and the codestream.
fn read_chunk_header(compressed: &[u8], channel_count: usize) -> Result<(u16, Vec<usize>, &[u8])> {
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

    let magic = read_u16(take(2)?);
    if magic != MAGIC && magic != MAGIC_LJ2K {
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
    Ok((magic, file_channels, codestream))
}

/// Stores decoded samples as native-endian samples of `sample_type`. The
/// decoder returns the bits of each sample in an `i32`; a HALF sample is in
/// its low 16 bits. `legacy_lossy` says the samples are those of a lossy
/// component of a legacy `LJ2K` chunk instead: see
/// [`write_legacy_samples`].
fn write_samples(target: &mut [u8], samples: &[i32], sample_type: SampleType, legacy_lossy: bool) {
    if legacy_lossy {
        return write_legacy_samples(target, samples, sample_type);
    }
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

/// Stores the decoded samples of a lossy component of a legacy `LJ2K` chunk
/// (`OpenEXR` 3.5.0 or 3.5.1) as native-endian samples of `sample_type`,
/// undoing the transfer function those versions applied before coding, as
/// `decode_line` in `OpenEXR`'s `internal_ht.cpp` does: HALF samples with
/// [`legacy_half_bits`], any other type, as FLOAT, with
/// [`legacy_float_bits`]. (Those versions never coded a UINT channel
/// lossily, so only a damaged chunk gets here with one.)
fn write_legacy_samples(target: &mut [u8], samples: &[i32], sample_type: SampleType) {
    match sample_type {
        SampleType::F16 => {
            for (bytes, &sample) in target.chunks_exact_mut(2).zip(samples) {
                bytes.copy_from_slice(&legacy_half_bits(sample).to_ne_bytes());
            }
        }
        SampleType::F32 | SampleType::U32 => {
            for (bytes, &sample) in target.chunks_exact_mut(4).zip(samples) {
                bytes.copy_from_slice(&legacy_float_bits(sample).to_ne_bytes());
            }
        }
    }
}

/// `LEGACY_INT16_HALF_FACTOR` in `OpenEXR`: 32767 / `tf_from_linear(HALF_MAX)`,
/// the scale from the transfer function's domain to legacy 16-bit samples.
const LEGACY_HALF_FACTOR: f64 = 5_424.238_088_666_29;

/// `LEGACY_INT32_FLOAT_FACTOR` in `OpenEXR`: `i32::MAX` /
/// `tf_from_linear(f32::MAX)`, as [`LEGACY_HALF_FACTOR`] for 32 bits.
const LEGACY_FLOAT_FACTOR: f64 = 51_961_246.180_338;

/// `legacy_int16_to_half` in `OpenEXR`: a decoded 16-bit sample of a legacy
/// lossy component, clamped to ±32767, scaled back to the transfer
/// function's domain and taken to linear, as the bits of a HALF value.
///
/// `OpenEXR` returns the `f64` as an Imath `half`, which converts it to
/// `f32` first and that to HALF, rounding twice, to nearest even each time.
fn legacy_half_bits(sample: i32) -> u16 {
    let sample = sample.clamp(-32767, 32767);
    // the rounding of the C++ conversions, which this must reproduce
    #[allow(clippy::cast_possible_truncation)]
    let linear = tf_to_linear(f64::from(sample) / LEGACY_HALF_FACTOR) as f32;
    float_to_half_bits(linear)
}

/// `legacy_int32_to_float` in `OpenEXR`: as [`legacy_half_bits`] for a 32-bit
/// sample, without clamping it, to the bits of a FLOAT value clamped to
/// ±`f32::MAX`.
fn legacy_float_bits(sample: i32) -> u32 {
    let linear = tf_to_linear(f64::from(sample) / LEGACY_FLOAT_FACTOR);
    let max = f64::from(f32::MAX);
    let value = if linear > max {
        f32::MAX
    } else if linear < -max {
        -f32::MAX
    } else {
        // to nearest, as the C++ cast
        #[allow(clippy::cast_possible_truncation)]
        let value = linear as f32;
        value
    };
    value.to_bits()
}

/// The length a chunk dimension of `length` samples is padded to when coded
/// lossily with `levels` decomposition levels (`ht_padded_length` in
/// `OpenEXR`): the smallest length of at least `length` that is 1 modulo
/// 2^`levels`. `None` above [`DECOMPOSITION_LEVELS`] levels, which bounds
/// the padding, or if the padded length does not fit in an `i32`.
///
/// With the image at offset 0, the symmetric extension of the wavelet then
/// folds on a low-pass sample at every level, so that the quantisation
/// error does not show as a step along the bottom and right edges of each
/// chunk. `OpenEXR` repeats the last line and column of the chunk up to that
/// size, and drops them when decoding.
fn padded_length(length: usize, levels: u32) -> Option<usize> {
    if levels > DECOMPOSITION_LEVELS {
        return None;
    }
    let modulus = 1_usize << levels;
    let padded = length.checked_add((modulus + 1 - length % modulus) % modulus)?;
    i32::try_from(padded).is_ok().then_some(padded)
}

fn codestream_error(error: ::htj2k::Error) -> Error {
    Error::invalid(format!("HTJ2K codestream: {}", error.message()))
}

#[cfg(test)]
mod test {
    use super::*;

    const H: SampleType = SampleType::F16;
    const F: SampleType = SampleType::F32;
    const U: SampleType = SampleType::U32;

    fn map(list: &[(&str, SampleType, usize)]) -> (bool, Vec<usize>) {
        let channels = list
            .iter()
            .map(|&(name, sample_type, sampling)| ChannelDescription {
                name: name.into(),
                sample_type,
                quantize_linearly: false,
                sampling: Vec2(sampling, sampling),
            })
            .collect();
        channel_map(&ChannelList::new(channels))
    }

    #[test]
    fn rgb_goes_first() {
        let abgr = [("A", H, 1), ("B", H, 1), ("G", H, 1), ("R", H, 1)];
        assert_eq!(map(&abgr), (true, vec![3, 2, 1, 0]));
        let lower = [("b", F, 1), ("g", F, 1), ("r", F, 1), ("z", F, 1)];
        assert_eq!(map(&lower), (true, vec![2, 1, 0, 3]));
        let words = [("Blue", U, 1), ("GREEN", U, 1), ("id", U, 1), ("red", U, 1)];
        assert_eq!(map(&words), (true, vec![3, 1, 0, 2]));
    }

    #[test]
    fn rgb_must_match() {
        // types differ, sampling differs
        assert_eq!(map(&[("B", H, 1), ("G", F, 1), ("R", H, 1)]), (false, vec![0, 1, 2]));
        assert_eq!(map(&[("B", H, 2), ("G", H, 1), ("R", H, 1)]), (false, vec![0, 1, 2]));
        // no blue, and luminance and chroma are no triplet
        assert_eq!(map(&[("G", H, 1), ("R", H, 1)]), (false, vec![0, 1]));
        assert_eq!(map(&[("BY", H, 2), ("RY", H, 2), ("Y", H, 1)]), (false, vec![0, 1, 2]));
        // a colour is a whole name or the suffix after the last "."
        let joined = [("MainB", H, 1), ("MainG", H, 1), ("MainR", H, 1)];
        assert_eq!(map(&joined), (false, vec![0, 1, 2]));
    }

    #[test]
    fn rgb_shares_a_prefix() {
        let layers = [("diffuse.B", H, 1), ("diffuse.G", H, 1), ("diffuse.R", H, 1), ("x.R", H, 1)];
        assert_eq!(map(&layers), (true, vec![2, 1, 0, 3]));
        // the first prefix to match a colour claims the triplet
        let mixed = [("a.B", H, 1), ("b.G", H, 1), ("b.R", H, 1), ("c.B", H, 1)];
        assert_eq!(map(&mixed), (false, vec![0, 1, 2, 3]));
        // the second triplet counts when the first is incomplete
        let words = [("R", H, 1), ("x.blue", F, 1), ("x.green", F, 1), ("x.red", F, 1)];
        assert_eq!(map(&words), (true, vec![3, 2, 1, 0]));
    }

    #[test]
    fn chunk_header_round_trip() {
        let mut chunk = Vec::new();
        write_chunk_header(&mut chunk, MAGIC, &[3, 2, 1, 0]).unwrap();
        assert_eq!(chunk, [b'H', b'T', 0, 0, 0, 10, 0, 4, 0, 3, 0, 2, 0, 1, 0, 0]);
        chunk.extend_from_slice(&[0xFF, 0x4F]);
        let (magic, file_channels, codestream) = read_chunk_header(&chunk, 4).unwrap();
        assert_eq!(magic, MAGIC);
        assert_eq!(file_channels, [3, 2, 1, 0]);
        assert_eq!(codestream, [0xFF, 0x4F]);
    }

    #[test]
    fn legacy_transfer_function() {
        // zero, odd symmetry, and the clamp of 16-bit samples
        assert_eq!(legacy_half_bits(0), 0);
        assert_eq!(legacy_float_bits(0), 0);
        for sample in [1, 77, 5424, 20000, 32767] {
            assert_eq!(legacy_half_bits(-sample), legacy_half_bits(sample) | 0x8000);
        }
        assert_eq!(legacy_half_bits(40000), legacy_half_bits(32767));
        assert_eq!(legacy_half_bits(i32::MIN), legacy_half_bits(-32767));

        // the factors map the largest samples to the largest values: HALF's
        // and, clamped, FLOAT's (values from OpenEXR 3.5.3's functions)
        assert_eq!(legacy_half_bits(32767), 0x7BFF);
        assert_eq!(legacy_half_bits(5424), 0x3C00);
        assert_eq!(legacy_float_bits(i32::MAX), 0x7F7F_FFFF);
        assert_eq!(legacy_float_bits(i32::MIN), 0xFF7F_FFFF);
        assert_eq!(legacy_float_bits(1), 0x2343_9FFF);
        assert_eq!(legacy_float_bits(-1), 0xA343_9FFF);
    }

    #[test]
    fn padded_lengths() {
        // 1 modulo 32, as `ht_padded_length` gives
        assert_eq!(padded_length(0, 5), Some(1));
        assert_eq!(padded_length(1, 5), Some(1));
        assert_eq!(padded_length(2, 5), Some(33));
        assert_eq!(padded_length(32, 5), Some(33));
        assert_eq!(padded_length(33, 5), Some(33));
        assert_eq!(padded_length(256, 5), Some(257));
        assert_eq!(padded_length(256, 0), Some(256));
        assert_eq!(padded_length(256, 1), Some(257));
        assert_eq!(padded_length(256, 6), None);
        let max = i32::MAX as usize;
        assert_eq!(padded_length(max - 31, 5), Some(max - 30));
        assert_eq!(padded_length(max - 30, 5), Some(max - 30));
        assert_eq!(padded_length(max - 29, 5), None);
    }
}
