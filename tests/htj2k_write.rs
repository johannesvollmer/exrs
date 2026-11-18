//! Gates for HTJ2K writing: exrs must write the chunks OpenEXR 3.5 writes,
//! byte for byte, and OpenEXR must read what exrs writes.
//!
//! For each test image:
//!
//! 1. exrs writes it with `HTJ2K32` and `HTJ2K256`; OpenEXR's `exrmetrics
//!    --convert -z none` converts that file back to uncompressed, and every
//!    sample must come out bit for bit as it went in.
//! 2. `exrmetrics --convert -z htj2k32` (and `htj2k256`) converts the
//!    uncompressed image, and each of its chunks must be byte for byte the
//!    chunk exrs wrote, but for the OpenJPH version that the codestream's
//!    comment names: exrs's says 0.33.0 and OpenEXR's 0.32.0, which is checked
//!    apart (see `htj2k_common`).
//! 3. exrs reads back what it wrote, without OpenEXR.
//!
//! Gates 1 and 2 need `exrmetrics` from OpenEXR 3.5 on the `PATH`, and are
//! skipped with a message without it. The committed HTJ2K fixtures, which
//! OpenEXR wrote (see `tests/images/valid/custom/htj2k/generate.py`), give a
//! version of gate 2 that always runs.
//!
//! exrs refuses subsampled channels in its image API, so images with them
//! are written chunk by chunk here: exrs compresses each chunk with
//! `Compression::compress_image_section_to_le`, and this file assembles the
//! file around the chunks. Every other image goes through exrs's image API.
//!
//! Needs the `htj2k` feature: `cargo test --features htj2k`.

#![cfg(feature = "htj2k")]

#[path = "htj2k_common/mod.rs"]
mod htj2k_common;

use std::{
    collections::BTreeMap,
    convert::TryInto,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
};

use exr::{
    compression::Compression,
    math::RoundingMode,
    meta::{
        attribute::{
            ChannelDescription, IntegerBounds, LevelMode, LineOrder, SampleType, TileDescription,
        },
        header::Header,
        BlockDescription,
    },
    prelude::*,
};

/// A channel of a test image.
#[derive(Clone, Debug)]
struct Channel {
    name: &'static str,
    sample_type: SampleType,
    sampling: (usize, usize),
}

const fn channel(name: &'static str, sample_type: SampleType) -> Channel {
    Channel {
        name,
        sample_type,
        sampling: (1, 1),
    }
}

const fn sampled(name: &'static str, sample_type: SampleType, x: usize, y: usize) -> Channel {
    Channel {
        name,
        sample_type,
        sampling: (x, y),
    }
}

use SampleType::{F16 as HALF, F32 as FLOAT, U32 as UINT};

/// How the samples vary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pattern {
    /// Ramps with special values here and there, as in `generate.py`, which
    /// compress well.
    Smooth,

    /// Random bits, which do not compress: the chunks are stored raw.
    Noise,
}

/// A test image: one layer.
#[derive(Clone, Debug)]
struct Case {
    name: &'static str,

    /// In any order; the file sorts them by name.
    channels: Vec<Channel>,

    origin: (i32, i32),
    size: (usize, usize),
    tile_size: Option<usize>,
    pattern: Pattern,
}

impl Case {
    fn new(name: &'static str, channels: &[Channel], size: (usize, usize)) -> Self {
        let mut channels = channels.to_vec();
        channels.sort_by_key(|channel| channel.name);
        Self {
            name,
            channels,
            origin: (0, 0),
            size,
            tile_size: None,
            pattern: Pattern::Smooth,
        }
    }

    fn at(self, x: i32, y: i32) -> Self {
        Self {
            origin: (x, y),
            ..self
        }
    }

    fn tiled(self, tile_size: usize) -> Self {
        Self {
            tile_size: Some(tile_size),
            ..self
        }
    }

    fn noise(self) -> Self {
        Self {
            pattern: Pattern::Noise,
            ..self
        }
    }

    fn is_subsampled(&self) -> bool {
        self.channels.iter().any(|channel| channel.sampling != (1, 1))
    }

    /// The bits of one sample at absolute pixel coordinates.
    fn sample(&self, index: usize, x: i32, y: i32) -> u32 {
        let channel = &self.channels[index];
        let x = x - self.origin.0;
        let y = y - self.origin.1;
        sample_bits(self.pattern, channel.sample_type, index, x, y)
    }

    /// The samples of a chunk, `rectangle` being relative to the data
    /// window, in OpenEXR's uncompressed layout, little-endian: line by
    /// line, and in each line every channel that has samples there.
    fn chunk_samples(&self, rectangle: IntegerBounds) -> Vec<u8> {
        let mut bytes = Vec::new();
        for relative_y in rectangle.position.y()..rectangle.end().y() {
            let y = relative_y + self.origin.1;
            for (index, channel) in self.channels.iter().enumerate() {
                let (x_sampling, y_sampling) =
                    (channel.sampling.0 as i32, channel.sampling.1 as i32);
                if y.rem_euclid(y_sampling) != 0 {
                    continue;
                }
                for relative_x in rectangle.position.x()..rectangle.end().x() {
                    let x = relative_x + self.origin.0;
                    if x.rem_euclid(x_sampling) != 0 {
                        continue;
                    }
                    let bits = self.sample(index, x, y);
                    match channel.sample_type {
                        HALF => bytes.extend_from_slice(&(bits as u16).to_le_bytes()),
                        _ => bytes.extend_from_slice(&bits.to_le_bytes()),
                    }
                }
            }
        }
        bytes
    }

    /// The chunks of the image, with their rectangles relative to the data
    /// window, keyed as in the file.
    fn chunks(&self, compression: Compression) -> Vec<(ChunkKey, IntegerBounds)> {
        let (width, height) = self.size;
        match self.tile_size {
            Some(tile) => {
                let mut chunks = Vec::new();
                for tile_y in 0..height.div_ceil(tile) {
                    for tile_x in 0..width.div_ceil(tile) {
                        let position = Vec2(tile_x * tile, tile_y * tile);
                        let size =
                            Vec2(tile.min(width - position.x()), tile.min(height - position.y()));
                        chunks.push((
                            ChunkKey::Tile(tile_x as i32, tile_y as i32),
                            IntegerBounds::new(position.to_i32(), size),
                        ));
                    }
                }
                chunks
            }
            None => {
                let lines = compression.scan_lines_per_block();
                (0..height)
                    .step_by(lines)
                    .map(|y| {
                        (
                            ChunkKey::Line(y as i32 + self.origin.1),
                            IntegerBounds::new(
                                Vec2(0, y as i32),
                                Vec2(width, lines.min(height - y)),
                            ),
                        )
                    })
                    .collect()
            }
        }
    }

    fn header(&self, compression: Compression) -> Header {
        let channels = self
            .channels
            .iter()
            .map(|channel| ChannelDescription {
                name: channel.name.into(),
                sample_type: channel.sample_type,
                quantize_linearly: false,
                sampling: Vec2(channel.sampling.0, channel.sampling.1),
            })
            .collect();
        let blocks = match self.tile_size {
            Some(tile) => BlockDescription::Tiles(TileDescription {
                tile_size: Vec2(tile, tile),
                level_mode: LevelMode::Singular,
                rounding_mode: RoundingMode::Down,
            }),
            None => BlockDescription::ScanLines,
        };
        Header::new("".into(), self.size, channels)
            .with_encoding(compression, blocks, LineOrder::Increasing)
            .with_position(Vec2(self.origin.0, self.origin.1))
    }

    /// The image as exrs's image API takes it. Not for subsampled images.
    fn image(&self, compression: Compression) -> SingleLayer {
        assert!(!self.is_subsampled());
        let (width, height) = self.size;
        let channels = self
            .channels
            .iter()
            .enumerate()
            .map(|(index, channel)| {
                let bits = (0..height as i32).flat_map(|y| {
                    (0..width as i32)
                        .map(move |x| self.sample(index, x + self.origin.0, y + self.origin.1))
                });
                let samples = match channel.sample_type {
                    HALF => FlatSamples::F16(bits.map(|b| f16::from_bits(b as u16)).collect()),
                    FLOAT => FlatSamples::F32(bits.map(f32::from_bits).collect()),
                    UINT => FlatSamples::U32(bits.collect()),
                };
                AnyChannel::new(channel.name, samples)
            })
            .collect();

        let encoding = Encoding {
            compression,
            blocks: match self.tile_size {
                Some(tile) => Blocks::Tiles(Vec2(tile, tile)),
                None => Blocks::ScanLines,
            },
            line_order: LineOrder::Increasing,
        };
        let attributes = LayerAttributes {
            layer_position: Vec2(self.origin.0, self.origin.1),
            ..LayerAttributes::default()
        };
        Image::from_layer(Layer::new(self.size, attributes, encoding, AnyChannels::sort(channels)))
    }

    /// The image written by exrs with `compression`.
    fn write_with_exrs(&self, compression: Compression) -> Vec<u8> {
        if self.is_subsampled() {
            let chunks = self.compress_with_exrs(compression).unwrap_or_else(|error| {
                panic!("{}: exrs cannot compress it: {}", self.name, error)
            });
            self.assemble(compression, &chunks)
        } else {
            let mut bytes = Vec::new();
            self.image(compression)
                .write()
                .non_parallel()
                .to_buffered(Cursor::new(&mut bytes))
                .unwrap_or_else(|error| panic!("{}: exrs cannot write it: {}", self.name, error));
            bytes
        }
    }

    /// Every chunk compressed by exrs, as `write_with_exrs` writes them.
    fn compress_with_exrs(&self, compression: Compression) -> exr::error::Result<Chunks> {
        let header = self.header(compression);
        self.chunks(compression)
            .into_iter()
            .map(|(key, rectangle)| {
                let samples = native_endian(self.chunk_samples(rectangle), &self.channels);
                let compressed =
                    compression.compress_image_section_to_le(&header, samples, rectangle)?;
                Ok((key, compressed))
            })
            .collect()
    }

    /// The image uncompressed, written here byte by byte.
    fn write_uncompressed(&self) -> Vec<u8> {
        let chunks = self
            .chunks(Compression::Uncompressed)
            .into_iter()
            .map(|(key, rectangle)| (key, self.chunk_samples(rectangle)))
            .collect();
        self.assemble(Compression::Uncompressed, &chunks)
    }

    /// A single-part file with the chunks given, in their order.
    fn assemble(&self, compression: Compression, chunks: &Chunks) -> Vec<u8> {
        fn attribute(target: &mut Vec<u8>, name: &str, kind: &str, value: &[u8]) {
            for text in [name, kind] {
                target.extend_from_slice(text.as_bytes());
                target.push(0);
            }
            target.extend_from_slice(&(value.len() as i32).to_le_bytes());
            target.extend_from_slice(value);
        }
        let int = |value: i32| value.to_le_bytes();

        let mut channel_list = Vec::new();
        for channel in &self.channels {
            channel_list.extend_from_slice(channel.name.as_bytes());
            channel_list.push(0);
            let kind = match channel.sample_type {
                UINT => 0,
                HALF => 1,
                FLOAT => 2,
            };
            channel_list.extend_from_slice(&int(kind));
            channel_list.extend_from_slice(&[0, 0, 0, 0]);
            channel_list.extend_from_slice(&int(channel.sampling.0 as i32));
            channel_list.extend_from_slice(&int(channel.sampling.1 as i32));
        }
        channel_list.push(0);

        let (x0, y0) = self.origin;
        let window: Vec<u8> = [x0, y0, x0 + self.size.0 as i32 - 1, y0 + self.size.1 as i32 - 1]
            .iter()
            .flat_map(|&value| int(value))
            .collect();
        let compression_code: u8 = match compression {
            Compression::Uncompressed => 0,
            Compression::HTJ2K256 => 10,
            Compression::HTJ2K32 => 11,
            _ => unreachable!(),
        };

        let mut bytes = vec![0x76, 0x2f, 0x31, 0x01];
        let flags: u32 = if self.tile_size.is_some() {
            2 | 0x200
        } else {
            2
        };
        bytes.extend_from_slice(&flags.to_le_bytes());
        attribute(&mut bytes, "channels", "chlist", &channel_list);
        attribute(&mut bytes, "compression", "compression", &[compression_code]);
        attribute(&mut bytes, "dataWindow", "box2i", &window);
        attribute(&mut bytes, "displayWindow", "box2i", &window);
        attribute(&mut bytes, "lineOrder", "lineOrder", &[0]);
        attribute(&mut bytes, "pixelAspectRatio", "float", &1.0_f32.to_le_bytes());
        attribute(&mut bytes, "screenWindowCenter", "v2f", &[0; 8]);
        attribute(&mut bytes, "screenWindowWidth", "float", &1.0_f32.to_le_bytes());
        if let Some(tile) = self.tile_size {
            let mut description = Vec::new();
            description.extend_from_slice(&(tile as u32).to_le_bytes());
            description.extend_from_slice(&(tile as u32).to_le_bytes());
            description.push(0);
            attribute(&mut bytes, "tiles", "tiledesc", &description);
        }
        bytes.push(0);

        let mut offset = bytes.len() + 8 * chunks.len();
        let mut table = Vec::new();
        let mut data = Vec::new();
        for (key, payload) in chunks {
            table.extend_from_slice(&(offset as u64).to_le_bytes());
            let start = data.len();
            match *key {
                ChunkKey::Line(y) => data.extend_from_slice(&int(y)),
                ChunkKey::Tile(x, y) => {
                    for value in [x, y, 0, 0] {
                        data.extend_from_slice(&int(value));
                    }
                }
            }
            data.extend_from_slice(&int(payload.len() as i32));
            data.extend_from_slice(payload);
            offset += data.len() - start;
        }
        bytes.extend_from_slice(&table);
        bytes.extend_from_slice(&data);
        bytes
    }

    /// Every sample of the image, little-endian, as `-z none` lays it out:
    /// one chunk per line or per tile.
    fn uncompressed_chunks(&self) -> BTreeMap<ChunkKey, Vec<u8>> {
        self.chunks(Compression::Uncompressed)
            .into_iter()
            .map(|(key, rectangle)| (key, self.chunk_samples(rectangle)))
            .collect()
    }
}

/// Where a chunk is: the first line of a block of lines, or a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ChunkKey {
    Line(i32),
    Tile(i32, i32),
}

type Chunks = Vec<(ChunkKey, Vec<u8>)>;

type SingleLayer = Image<Layer<AnyChannels<FlatSamples>>>;

/// Converts little-endian samples in OpenEXR's layout to native endianness.
fn native_endian(bytes: Vec<u8>, channels: &[Channel]) -> Vec<u8> {
    if cfg!(target_endian = "little") {
        return bytes;
    }
    // the samples of one chunk only ever come from `chunk_samples`, which
    // has the same sample size throughout when this matters
    let size = if channels.iter().all(|channel| channel.sample_type == HALF) {
        2
    } else {
        4
    };
    assert!(channels.iter().all(|channel| channel.sample_type.bytes_per_sample() == size));
    bytes.chunks_exact(size).flat_map(|sample| sample.iter().rev().copied()).collect()
}

/// splitmix64
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

const HALF_SPECIALS: [u32; 10] =
    [0x7C00, 0xFC00, 0x7E01, 0xFD23, 0x8000, 0x0001, 0x83FF, 0x7BFF, 0xFBFF, 0x0000];
const FLOAT_SPECIALS: [u32; 10] = [
    0x7F80_0000,
    0xFF80_0000,
    0x7FC0_0001,
    0xFFA5_A5A5,
    0x8000_0000,
    0x0000_0001,
    0x807F_FFFF,
    0x7F7F_FFFF,
    0xFF7F_FFFF,
    0x0000_0000,
];
const UINT_SPECIALS: [u32; 5] = [0xFFFF_FFFF, 0x8000_0000, 0x7FFF_FFFF, 0, 1];

/// The bits of a sample at `x`, `y` relative to the data window origin:
/// `sample_bytes` in `generate.py` for smooth images, so that the fixtures'
/// source images come out the same.
fn sample_bits(pattern: Pattern, sample_type: SampleType, channel: usize, x: i32, y: i32) -> u32 {
    if pattern == Pattern::Noise {
        let seed = (channel as u64) << 40 ^ (y as u64 & 0xFFFFF) << 20 ^ (x as u64 & 0xFFFFF);
        let bits = mix(seed) as u32;
        return match sample_type {
            HALF => bits & 0xFFFF,
            _ => bits,
        };
    }

    let channel_index = channel as i32;
    let special = (x * 7 + y * 13 + channel_index * 5).rem_euclid(193) == 0;
    let pick = (x + y + channel_index).rem_euclid(1 << 20) as usize;
    let (x, y) = (f64::from(x), f64::from(y));
    match sample_type {
        HALF if special => HALF_SPECIALS[pick % HALF_SPECIALS.len()],
        HALF => {
            let value = 0.25 + 0.031 * x - 0.017 * y + 0.5 * f64::from(channel_index);
            u32::from(f16::from_f64(value).to_bits())
        }
        FLOAT if special => FLOAT_SPECIALS[pick % FLOAT_SPECIALS.len()],
        FLOAT => {
            let sign = if channel_index % 2 == 1 {
                -1.0
            } else {
                1.0
            };
            let value = sign * (100.0 + x * 2.0_f64.powi(-14) + y * 2.0_f64.powi(-12));
            (value as f32).to_bits()
        }
        UINT if special => UINT_SPECIALS[pick % UINT_SPECIALS.len()],
        UINT => (1000.0 * x + y + 7.0 * f64::from(channel_index)) as u32,
    }
}

/// The images every gate runs on.
fn cases() -> Vec<Case> {
    vec![
        // the sources of the committed fixtures, as `generate.py` makes them
        Case::new(
            "rgba_half",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF), channel("A", HALF)],
            (61, 43),
        )
        .at(-5, -9),
        Case::new(
            "rgb_float_uint",
            &[
                channel("R", FLOAT),
                channel("G", FLOAT),
                channel("B", FLOAT),
                channel("Z", FLOAT),
                channel("id", UINT),
            ],
            (37, 70),
        )
        .at(3, 2),
        Case::new(
            "mixed_rgb",
            &[channel("R", HALF), channel("G", FLOAT), channel("B", HALF)],
            (29, 44),
        ),
        Case::new("y_half", &[channel("Y", HALF)], (40, 48)),
        Case::new(
            "tiled",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF), channel("Z", FLOAT)],
            (45, 44),
        )
        .tiled(16),
        // more
        Case::new("one_float", &[channel("depth", FLOAT)], (50, 300)),
        Case::new("one_uint", &[channel("id", UINT)], (33, 70)),
        Case::new(
            "rgb_half",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF)],
            (64, 300),
        ),
        Case::new(
            "rgb_float",
            &[channel("R", FLOAT), channel("G", FLOAT), channel("B", FLOAT)],
            (48, 40),
        ),
        Case::new(
            "mixed_types",
            &[channel("A", HALF), channel("depth", FLOAT), channel("id", UINT)],
            (40, 70),
        ),
        Case::new(
            "extra_channels",
            &[
                channel("A", HALF),
                channel("B", HALF),
                channel("G", HALF),
                channel("R", HALF),
                channel("Z", FLOAT),
                channel("diffuse.blue", FLOAT),
                channel("diffuse.green", FLOAT),
                channel("diffuse.red", FLOAT),
                channel("id", UINT),
                channel("normal.x", HALF),
            ],
            (36, 40),
        ),
        // R, G and B of different types: no colour transform, and the
        // red/green/blue triplet with a prefix is found instead
        Case::new(
            "prefixed_triplet",
            &[
                channel("R", HALF),
                channel("G", FLOAT),
                channel("B", HALF),
                channel("light.Red", HALF),
                channel("light.GREEN", HALF),
                channel("light.blue", HALF),
            ],
            (31, 33),
        ),
        Case::new(
            "luma_chroma",
            &[sampled("Y", HALF, 1, 1), sampled("RY", HALF, 2, 2), sampled("BY", HALF, 2, 2)],
            (64, 70),
        )
        .at(-8, -6),
        Case::new(
            "luma_chroma_alpha",
            &[
                sampled("A", HALF, 1, 1),
                sampled("Y", HALF, 1, 1),
                sampled("RY", HALF, 2, 2),
                sampled("BY", HALF, 2, 2),
            ],
            (40, 300),
        ),
        // subsampled R, G and B: planar, so no colour transform
        Case::new(
            "rgb_subsampled",
            &[sampled("R", FLOAT, 2, 4), sampled("G", FLOAT, 2, 4), sampled("B", FLOAT, 2, 4)],
            (32, 64),
        ),
        // y sampling 64 is above HTJ2K32's 32 lines: those chunks are raw
        Case::new(
            "y_sampling_64",
            &[
                channel("R", HALF),
                channel("G", HALF),
                channel("B", HALF),
                sampled("A", HALF, 1, 64),
            ],
            (40, 128),
        ),
        Case::new(
            "odd_size",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF), channel("Z", FLOAT)],
            (33, 291),
        )
        .at(-17, 5),
        Case::new(
            "smaller_than_a_chunk",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF)],
            (120, 20),
        ),
        // too small for a codestream to pay: stored raw
        Case::new("tiny", &[channel("R", HALF), channel("G", HALF), channel("B", HALF)], (7, 5)),
        Case::new("one_pixel", &[channel("Y", HALF)], (1, 1)),
        Case::new(
            "strip",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF), channel("A", HALF)],
            (8192, 32),
        ),
        Case::new("float_strip", &[channel("Z", FLOAT)], (8192, 40)),
        Case::new(
            "noise",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF), channel("Z", FLOAT)],
            (40, 70),
        )
        .noise(),
        Case::new(
            "noise_tiled",
            &[channel("R", HALF), channel("G", HALF), channel("B", HALF)],
            (40, 40),
        )
        .tiled(32)
        .noise(),
    ]
}

const METHODS: [(Compression, &str); 2] =
    [(Compression::HTJ2K32, "htj2k32"), (Compression::HTJ2K256, "htj2k256")];

/// Whether exrs stores the chunk raw rather than as an HTJ2K codestream on
/// purpose, where OpenEXR codes it: a channel's y sampling is above the
/// chunk height, where OpenEXR writes chunks it cannot read back.
fn raw_on_purpose(case: &Case, compression: Compression) -> bool {
    case.channels.iter().any(|channel| channel.sampling.1 > compression.scan_lines_per_block())
}

/// The chunks of a single-part file, keyed by position.
fn read_chunks(bytes: &[u8]) -> BTreeMap<ChunkKey, Vec<u8>> {
    let int = |at: usize| i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    let tiled = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) & 0x200 != 0;

    let mut position = 8;
    while bytes[position] != 0 {
        for _ in 0..2 {
            position += bytes[position..].iter().position(|&byte| byte == 0).unwrap() + 1;
        }
        position += 4 + int(position) as usize;
    }
    position += 1;

    let first_chunk = u64::from_le_bytes(bytes[position..position + 8].try_into().unwrap());
    let count = (first_chunk as usize - position) / 8;
    (0..count)
        .map(|index| {
            let at = position + 8 * index;
            let offset = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) as usize;
            let (key, start) = if tiled {
                (ChunkKey::Tile(int(offset), int(offset + 4)), offset + 20)
            } else {
                (ChunkKey::Line(int(offset)), offset + 8)
            };
            let size = int(start - 4) as usize;
            (key, bytes[start..start + size].to_vec())
        })
        .collect()
}

/// How a chunk exrs wrote differs from OpenEXR's, if it does. The OpenJPH
/// version in the codestream's comment is compared apart: each side must
/// name its own, and the rest must be identical.
fn difference(ours: &[u8], theirs: &[u8]) -> Option<String> {
    let (ours, theirs) = match htj2k_common::compare_normalised(ours, theirs) {
        Ok(normalised) => normalised,
        Err(message) => return Some(message),
    };
    if ours == theirs {
        return None;
    }
    let first = ours
        .iter()
        .zip(&theirs)
        .position(|(ours, theirs)| ours != theirs)
        .unwrap_or(ours.len().min(theirs.len()));
    Some(format!(
        "{} bytes, OpenEXR's {} bytes, first difference at byte {}",
        ours.len(),
        theirs.len(),
        first
    ))
}

fn is_htj2k(chunk: &[u8]) -> bool {
    chunk.starts_with(b"HT")
}

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("htj2k_write");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Whether `exrmetrics` runs; says why the test is skipped if not.
fn have_exrmetrics() -> bool {
    let found =
        Command::new("exrmetrics").arg("--version").output().is_ok_and(|out| out.status.success());
    if !found {
        // straight to stderr, which the test harness does not capture
        let _ = writeln!(std::io::stderr(), "skipped: exrmetrics (OpenEXR 3.5) is not on the PATH");
    }
    found
}

/// Converts `input` with `exrmetrics --convert -z <method>`.
fn exrmetrics_convert(
    input: &[u8],
    method: &str,
    name: &str,
) -> std::result::Result<Vec<u8>, String> {
    let dir = scratch_dir();
    let source = dir.join(format!("{name}_in.exr"));
    let target = dir.join(format!("{name}_{method}.exr"));
    std::fs::write(&source, input).unwrap();
    let _ = std::fs::remove_file(&target);
    let output = Command::new("exrmetrics")
        .args(["--convert", "-z", method])
        .arg(&source)
        .arg("-o")
        .arg(&target)
        .output()
        .unwrap();
    let result = if output.status.success() {
        Ok(std::fs::read(&target).unwrap())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    };
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&target);
    result
}

/// Gate 1: OpenEXR reads what exrs writes, bit for bit.
#[test]
fn openexr_reads_exrs_output_exactly() {
    if !have_exrmetrics() {
        return;
    }
    let mut failures = Vec::new();
    for case in cases() {
        let expected = case.uncompressed_chunks();
        for (compression, method) in METHODS {
            let label = format!("{} {}", case.name, method);
            let written = case.write_with_exrs(compression);
            match exrmetrics_convert(&written, "none", &format!("{}_{}", case.name, method)) {
                Err(message) => {
                    failures.push(format!("{label}: OpenEXR cannot read it: {message}"))
                }
                Ok(converted) => {
                    let decoded = read_chunks(&converted);
                    if decoded != expected {
                        failures.push(format!("{label}: OpenEXR reads other samples"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// Gate 2: exrs writes the chunks OpenEXR writes, byte for byte.
#[test]
fn chunks_match_openexr() {
    if !have_exrmetrics() {
        return;
    }
    let mut failures = Vec::new();
    for case in cases() {
        let source = case.write_uncompressed();
        for (compression, method) in METHODS {
            let label = format!("{} {}", case.name, method);
            let ours = read_chunks(&case.write_with_exrs(compression));
            let theirs = match exrmetrics_convert(&source, method, case.name) {
                Ok(converted) => read_chunks(&converted),
                Err(message) => {
                    failures.push(format!("{label}: OpenEXR cannot write it: {message}"));
                    continue;
                }
            };
            assert_eq!(
                ours.keys().collect::<Vec<_>>(),
                theirs.keys().collect::<Vec<_>>(),
                "{label}: chunks"
            );

            let coded = |chunks: &BTreeMap<ChunkKey, Vec<u8>>| {
                chunks.values().filter(|chunk| is_htj2k(chunk)).count()
            };
            eprintln!(
                "{label}: {} chunks, {} coded by exrs, {} by OpenEXR",
                ours.len(),
                coded(&ours),
                coded(&theirs)
            );

            for (key, our_chunk) in &ours {
                let their_chunk = &theirs[key];
                if raw_on_purpose(&case, compression) {
                    // OpenEXR codes these chunks, and then cannot read them:
                    // its encoder gives the channel ceil(32 / 64) = 1 line
                    // in every chunk, its decoder expects 32 / 64 = 0, and
                    // the chunks without a line of the channel hold wrong
                    // samples. exrs stores them raw instead.
                    assert!(is_htj2k(their_chunk), "{} {:?}: OpenEXR stored it raw", label, key);
                    assert!(!is_htj2k(our_chunk), "{} {:?}: exrs coded it", label, key);
                    continue;
                }
                if let Some(difference) = difference(our_chunk, their_chunk) {
                    failures.push(format!("{label} {key:?}: {difference}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

fn fixture_dir() -> &'static Path {
    Path::new("tests/images/valid/custom/htj2k")
}

/// Gate 2 without OpenEXR: the committed fixtures, which OpenEXR wrote.
/// exrs reads each uncompressed source, writes it with HTJ2K, and every
/// chunk must be the fixture's.
#[test]
fn chunks_match_committed_fixtures() {
    for name in ["rgba_half", "rgb_float_uint", "mixed_rgb", "y_half", "tiled"] {
        let case = cases().into_iter().find(|case| case.name == name).unwrap();
        let source = std::fs::read(fixture_dir().join(format!("{name}.exr"))).unwrap();
        assert_eq!(
            read_chunks(&source),
            case.uncompressed_chunks(),
            "{name}: the samples here differ from generate.py's"
        );

        let mut image = read()
            .no_deep_data()
            .largest_resolution_level()
            .all_channels()
            .first_valid_layer()
            .all_attributes()
            .from_buffered(Cursor::new(&source))
            .unwrap();

        for (compression, method) in METHODS {
            image.layer_data.encoding.compression = compression;
            let mut written = Vec::new();
            image.write().to_buffered(Cursor::new(&mut written)).unwrap();

            let fixture =
                std::fs::read(fixture_dir().join(format!("{name}_{method}.exr"))).unwrap();
            let (ours, theirs) = (read_chunks(&written), read_chunks(&fixture));
            assert_eq!(ours.len(), theirs.len(), "{name} {method}");
            for (key, chunk) in &theirs {
                assert!(is_htj2k(chunk));
                if let Some(difference) = difference(&ours[key], chunk) {
                    panic!("{} {} {:?}: {}", name, method, key, difference);
                }
            }
        }
    }
}

/// The bits of every sample, so that NaN payloads and the sign of zero count.
fn bits(samples: &FlatSamples) -> Vec<u32> {
    match samples {
        FlatSamples::F16(values) => values.iter().map(|v| u32::from(v.to_bits())).collect(),
        FlatSamples::F32(values) => values.iter().map(|v| v.to_bits()).collect(),
        FlatSamples::U32(values) => values.clone(),
    }
}

/// Gate 3: exrs reads back what it writes, without OpenEXR. Subsampled
/// images, which exrs's image API refuses, go chunk by chunk through the
/// compressor and the decompressor.
#[test]
fn exrs_round_trip() {
    for case in cases() {
        for (compression, method) in METHODS {
            let label = format!("{} {}", case.name, method);
            if case.is_subsampled() {
                let header = case.header(compression);
                let chunks = case.compress_with_exrs(compression).unwrap();
                for ((key, rectangle), (_, compressed)) in
                    case.chunks(compression).into_iter().zip(chunks)
                {
                    let expected = native_endian(case.chunk_samples(rectangle), &case.channels);
                    let decompressed = compression
                        .decompress_image_section_from_le(&header, compressed, rectangle, true)
                        .unwrap_or_else(|error| panic!("{} {:?}: {}", label, key, error));
                    assert!(decompressed == expected, "{} {:?}: samples differ", label, key);
                }
                continue;
            }

            let written = case.write_with_exrs(compression);
            let image = read()
                .no_deep_data()
                .largest_resolution_level()
                .all_channels()
                .first_valid_layer()
                .all_attributes()
                .pedantic()
                .from_buffered(Cursor::new(&written))
                .unwrap_or_else(|error| panic!("{}: {}", label, error));
            let expected = case.image(compression);

            assert_eq!(image.layer_data.encoding.compression, compression);
            assert_eq!(image.layer_data.size, expected.layer_data.size, "{label}");
            let channels = &image.layer_data.channel_data.list;
            let expected_channels = &expected.layer_data.channel_data.list;
            assert_eq!(channels.len(), expected_channels.len(), "{label}");
            for (channel, expected) in channels.iter().zip(expected_channels) {
                assert_eq!(channel.name, expected.name, "{label}");
                assert!(
                    bits(&channel.sample_data) == bits(&expected.sample_data),
                    "{label} {}: samples differ",
                    channel.name
                );
            }
        }
    }
}

/// The first chunk of `case`, compressed by exrs with `compression`.
fn single_chunk(case: &Case, compression: Compression) -> exr::error::Result<Vec<u8>> {
    let header = case.header(compression);
    let rectangle = case.chunks(compression)[0].1;
    let samples = native_endian(case.chunk_samples(rectangle), &case.channels);
    compression.compress_image_section_to_le(&header, samples, rectangle)
}

/// Sampling that is not a power of two is refused with a clear error, as
/// OpenEXR fails on it.
#[test]
fn sampling_not_a_power_of_two_is_refused() {
    let case = Case::new(
        "sampled_by_3",
        &[sampled("Y", HALF, 1, 1), sampled("RY", HALF, 3, 3), sampled("BY", HALF, 3, 3)],
        (48, 48),
    );
    for (compression, method) in METHODS {
        match single_chunk(&case, compression) {
            Err(Error::NotSupported(message)) => {
                assert!(message.contains("BY sampled 3x3"), "{}", message);
                assert!(message.contains("powers of two"), "{}", message);
            }
            other => panic!("{}: {:?}", method, other.map(|chunk| chunk.len())),
        }
    }

    if have_exrmetrics() {
        for (_, method) in METHODS {
            let converted = exrmetrics_convert(&case.write_uncompressed(), method, case.name);
            assert!(converted.is_err(), "OpenEXR now writes {} sampled by 3", method);
        }
    }
}

/// What the encoder refuses comes back as an error, not a panic: here a
/// sampling of 256, which a SIZ marker cannot hold, where it divides the
/// chunk height (HTJ2K256). With HTJ2K32 the chunks are stored raw.
#[test]
fn encoder_refusals_are_errors() {
    let case =
        Case::new("sampled_by_256", &[channel("Y", HALF), sampled("Z", HALF, 1, 256)], (16, 256));
    match single_chunk(&case, Compression::HTJ2K256) {
        Err(Error::NotSupported(message)) => {
            assert!(message.contains("HTJ2K encoder"), "{}", message);
        }
        other => panic!("{:?}", other.map(|chunk| chunk.len())),
    }

    let raw = single_chunk(&case, Compression::HTJ2K32).unwrap();
    let rectangle = case.chunks(Compression::HTJ2K32)[0].1;
    assert_eq!(raw, case.chunk_samples(rectangle));
}
