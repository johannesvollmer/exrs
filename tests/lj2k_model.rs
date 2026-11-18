//! exrs's LJ2K (and some HTJ2K256) chunks against OpenEXR's compressor on
//! generic OpenJPH, byte for byte, without OpenEXR.
//!
//! OpenEXR's lossy output depends on the CPU: the OpenJPH it links picks
//! AVX2 kernels at run time where it can, and those round some
//! irreversible float conversions differently from its generic code. The
//! htj2k crate follows the generic code, and so do exrs's LJ2K chunks: they
//! must be those of `ht_apply_impl` driving generic OpenJPH 0.33.0.
//!
//! The reference is a model of that compressor written for the htj2k
//! crate's OpenEXR gates, which writes OpenEXR's chunks byte for byte (500
//! chunks from OpenEXR's `internal_ht.cpp` compiled against generic OpenJPH,
//! every quality from 1 to 150). It is not part of exrs. Its codestreams for
//! the chunks here are recorded in
//! `tests/images/valid/custom/htj2k/model_chunks.txt`, as sizes and FNV-1a
//! hashes. Each chunk is given by the options of the model's `lj2k_enc`
//! command: size, channels in file order with their type and sampling,
//! quality, first line, and how the samples are made. This file makes the
//! same samples (with the model's splitmix64 generator, all integer, so no
//! floating point can differ), compresses them with exrs and compares.
//!
//! Needs the `htj2k` feature: `cargo test --features htj2k`.

#![cfg(feature = "htj2k")]

use exr::{
    compression::Compression,
    math::Vec2,
    meta::{
        attribute::{ChannelDescription, IntegerBounds, LineOrder, SampleType},
        header::Header,
        BlockDescription,
    },
};

/// One channel of a chunk.
struct Channel {
    name: String,
    sample_type: SampleType,
    sampling: (i32, i32),
    /// Samples per line.
    width: i32,
    /// Lines in the chunk.
    height: i32,
}

/// A chunk, as an `lj2k_enc` command line gives it.
struct Chunk {
    width: i32,
    height: i32,
    start_y: i32,
    channels: Vec<Channel>,
    compression: Compression,
    noise: bool,
    hdr: bool,
    specials: bool,
    seed: u64,
}

impl Chunk {
    fn parse(options: &str) -> Self {
        let mut chunk = Self {
            width: 0,
            height: 0,
            start_y: 0,
            channels: Vec::new(),
            compression: Compression::LJ2K(None),
            noise: false,
            hdr: false,
            specials: false,
            seed: 1,
        };
        let mut quality = None;
        let mut ht256 = false;
        let mut words = options.split_whitespace();
        while let Some(option) = words.next() {
            let mut value = || words.next().unwrap();
            match option {
                "--size" => {
                    let (width, height) = value().split_once('x').unwrap();
                    chunk.width = width.parse().unwrap();
                    chunk.height = height.parse().unwrap();
                }
                "--chan" => {
                    let mut fields = value().split(':');
                    let name = fields.next().unwrap().to_string();
                    let sample_type = match fields.next().unwrap() {
                        "h" => SampleType::F16,
                        "f" => SampleType::F32,
                        _ => SampleType::U32,
                    };
                    let sampling = fields.next().map_or((1, 1), |sampling| {
                        let (x, y) = sampling.split_once('x').unwrap();
                        (x.parse().unwrap(), y.parse().unwrap())
                    });
                    chunk.channels.push(Channel {
                        name,
                        sample_type,
                        sampling,
                        width: 0,
                        height: 0,
                    });
                }
                "--quality" => quality = Some(value().parse::<f64>().unwrap() as f32),
                "--start-y" => chunk.start_y = value().parse().unwrap(),
                "--seed" => chunk.seed = value().parse().unwrap(),
                "--pattern" => chunk.noise = value() == "noise",
                "--hdr" => chunk.hdr = true,
                "--specials" => chunk.specials = true,
                "--comp" => ht256 = value() == "ht256",
                _ => panic!("unknown option {}", option),
            }
        }
        chunk.compression = if ht256 {
            Compression::HTJ2K256
        } else {
            Compression::LJ2K(quality)
        };
        for channel in &mut chunk.channels {
            channel.width = chunk.width / channel.sampling.0;
            channel.height = (chunk.start_y..chunk.start_y + chunk.height)
                .filter(|y| y % channel.sampling.1 == 0)
                .count() as i32;
        }
        chunk
    }

    /// The bits of each sample, per channel and line by line, as the model
    /// makes them (`synthesise` and `sample` of `lj2k_enc.cpp`).
    fn samples(&self) -> Vec<Vec<u32>> {
        let mut out = Vec::new();
        for (index, channel) in self.channels.iter().enumerate() {
            let mut state = self
                .seed
                .wrapping_mul(0x0000_0100_0000_01B3)
                .wrapping_add((index as u64 + 1).wrapping_mul(0x9E37_79B9));
            let mut samples = Vec::new();
            for line in 0..channel.height as u32 {
                for x in 0..channel.width as u32 {
                    let mut bits = self.sample(channel, index as u32, x, line, &mut state);
                    if self.specials && line == 0 && channel.sample_type != SampleType::U32 && x < 3
                    {
                        bits = match channel.sample_type {
                            SampleType::F16 => [0x7C00, 0x7E00, 0xFC00][x as usize],
                            _ => [0x7F80_0000, 0x7FC0_0000, 0xFF80_0000][x as usize],
                        };
                    }
                    samples.push(bits);
                }
            }
            out.push(samples);
        }
        out
    }

    fn sample(&self, channel: &Channel, index: u32, x: u32, y: u32, state: &mut u64) -> u32 {
        let random = splitmix64(state);
        if channel.sample_type == SampleType::U32 {
            return if self.noise {
                random as u32
            } else {
                x / 4 + (y / 4) * 1000 + index
            };
        }
        let (low, span, mut kx, mut ky, sign) = if channel.sample_type == SampleType::F16 {
            (0x1000_u32, 0x4C00_u32 - 0x1000, 3_u32, 5_u32, 0x8000_u32)
        } else {
            let high = if self.hdr {
                0x4980_0000_u32
            } else {
                0x4180_0000
            };
            (0x3A00_0000, high - 0x3A00_0000, 3 << 13, 5 << 13, 0x8000_0000)
        };
        if self.hdr && channel.sample_type == SampleType::F32 {
            kx <<= 1;
            ky <<= 1;
        }
        let (bits, negative) = if self.noise {
            (low + (random % u64::from(span)) as u32, (random >> 40) & 7 == 0)
        } else {
            let ramp = u64::from(x) * u64::from(kx)
                + u64::from(y) * u64::from(ky)
                + u64::from(index) * 911 * u64::from(kx)
                + (random & 3);
            (low + (ramp % u64::from(span)) as u32, (x / 8 + y / 8) % 7 == 0)
        };
        if negative {
            bits | sign
        } else {
            bits
        }
    }

    /// The chunk compressed by exrs.
    fn compress(&self) -> Vec<u8> {
        let channels = self
            .channels
            .iter()
            .map(|channel| ChannelDescription {
                name: channel.name.as_str().into(),
                sample_type: channel.sample_type,
                quantize_linearly: false,
                sampling: Vec2(channel.sampling.0 as usize, channel.sampling.1 as usize),
            })
            .collect();
        let size = (self.width as usize, self.height as usize);
        let header = Header::new("".into(), size, channels)
            .with_encoding(self.compression, BlockDescription::ScanLines, LineOrder::Increasing)
            .with_position(Vec2(0, self.start_y));

        // line by line, and in each line the channels that have one there
        let samples = self.samples();
        let mut bytes = Vec::new();
        let mut next_line = vec![0_usize; self.channels.len()];
        for y in self.start_y..self.start_y + self.height {
            for (index, channel) in self.channels.iter().enumerate() {
                if y.rem_euclid(channel.sampling.1) != 0 {
                    continue;
                }
                let width = channel.width as usize;
                let start = next_line[index] * width;
                for &bits in &samples[index][start..start + width] {
                    match channel.sample_type {
                        SampleType::F16 => bytes.extend_from_slice(&(bits as u16).to_ne_bytes()),
                        _ => bytes.extend_from_slice(&bits.to_ne_bytes()),
                    }
                }
                next_line[index] += 1;
            }
        }
        let rectangle = IntegerBounds::new(Vec2(0, 0), Vec2(size.0, size.1));
        self.compression.compress_image_section_to_le(&header, bytes, rectangle).unwrap()
    }

    /// The size of the uncompressed chunk.
    fn raw_size(&self) -> usize {
        self.channels
            .iter()
            .map(|channel| {
                channel.sample_type.bytes_per_sample() * (channel.width * channel.height) as usize
            })
            .sum()
    }
}

/// splitmix64, as in `lj2k_enc.cpp`.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &byte in bytes {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[test]
fn chunks_match_the_model() {
    let text = std::fs::read_to_string("tests/images/valid/custom/htj2k/model_chunks.txt").unwrap();
    let mut failures = Vec::new();
    let (mut coded, mut raw) = (0, 0);
    for line in text.lines().filter(|line| !line.starts_with('#')) {
        let (options, expected) = line.split_once(" = ").unwrap();
        let chunk = Chunk::parse(options);
        let compressed = chunk.compress();
        let is_raw = compressed.len() == chunk.raw_size();

        let ours = if is_raw {
            "raw".to_string()
        } else {
            // the chunk header: magic number, payload size, channel map
            let codestream = &compressed[6 + 2 + 2 * chunk.channels.len()..];
            format!("{} {:016x}", codestream.len(), fnv1a(codestream))
        };
        if ours == expected {
            if is_raw {
                raw += 1;
            } else {
                coded += 1;
            }
        } else {
            failures.push(format!("{options}: exrs {ours}, the model {expected}"));
        }
    }
    eprintln!("identical to the model: {coded} coded chunks, {raw} raw ones");
    assert!(coded >= 100 && raw >= 10, "too few chunks");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
