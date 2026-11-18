//! Decodes the LJ2K files in `tests/images/valid/custom/htj2k` and compares
//! every sample with OpenEXR's own decoding of each, `<name>_decoded.exr`
//! (see `generate.py` there), by the rules of `lj2k_common`. OpenEXR 3.5.3
//! wrote the `<name>_lj2k_q<Q>.exr` files, and 3.5.1 the legacy
//! `<name>_lj2k351_q<Q>.exr` ones, whose lossy components carry the
//! transfer function outside the codestream; OpenEXR 3.5.3 decoded both.
//! Also checks which magic numbers mark which chunks, and that damaged
//! chunks of either kind never make exrs panic.
//!
//! Needs the `htj2k` feature: `cargo test --features htj2k`.

#![cfg(feature = "htj2k")]

#[path = "lj2k_common/mod.rs"]
mod lj2k_common;

use std::{
    convert::TryInto,
    io::Cursor,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
};

use exr::{meta::attribute::SampleType, prelude::*};
use lj2k_common::{compare_channel, lossy_channels, totals, ChannelInfo};
use rand::{rngs::StdRng, Rng, SeedableRng};

fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/htj2k")
}

/// Every LJ2K fixture, in name order.
fn lj2k_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path.file_name().unwrap().to_str().unwrap();
            (name.contains("_lj2k_q") || name.contains("_lj2k351_q"))
                && !name.ends_with("_decoded.exr")
        })
        .collect();
    files.sort();
    assert!(files.len() >= 18 + 14, "LJ2K fixtures missing: {:?}", files);
    files
}

/// Whether OpenEXR 3.5.0 or 3.5.1 wrote an LJ2K fixture.
fn is_legacy(path: &Path) -> bool {
    path.file_name().unwrap().to_str().unwrap().contains("_lj2k351_q")
}

fn decoded_file(lj2k_file: &Path) -> PathBuf {
    lj2k_file
        .with_file_name(format!("{}_decoded.exr", lj2k_file.file_stem().unwrap().to_str().unwrap()))
}

fn read_bytes(bytes: &[u8], pedantic: bool) -> Result<FlatImage> {
    let reader = read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes();
    if pedantic {
        reader.pedantic().from_buffered(Cursor::new(bytes))
    } else {
        reader.from_buffered(Cursor::new(bytes))
    }
}

fn read_file(path: &Path) -> FlatImage {
    let bytes = std::fs::read(path).unwrap();
    read_bytes(&bytes, true).unwrap_or_else(|error| panic!("{}: {}", path.display(), error))
}

/// The bits of every sample, so that NaN payloads and the sign of zero count.
fn sample_bits(samples: &FlatSamples) -> Vec<u32> {
    match samples {
        FlatSamples::F16(values) => values.iter().map(|v| u32::from(v.to_bits())).collect(),
        FlatSamples::F32(values) => values.iter().map(|v| v.to_bits()).collect(),
        FlatSamples::U32(values) => values.clone(),
    }
}

/// Gate: exrs decodes OpenEXR's LJ2K files, of 3.5.3 and of 3.5.1, as
/// OpenEXR 3.5.3 does, by the rules of `lj2k_common`: reversible channels
/// exactly, lossy ones within one unit in the last place. Prints how many
/// samples differ and by how much, for every lossy channel and every
/// channel that differs, and in total per type and writer. (OpenEXR 3.5.1
/// decodes the legacy files to the same bits as 3.5.3: `generate.py`
/// checks.)
#[test]
fn decodes_samples_as_openexr() {
    let mut reports = Vec::new();
    for path in lj2k_files() {
        let decoded = read_file(&path);
        let expected = read_file(&decoded_file(&path));
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(decoded.layer_data.len(), expected.layer_data.len(), "{name}");

        for (decoded, expected) in decoded.layer_data.iter().zip(&expected.layer_data) {
            assert_eq!(decoded.encoding.compression, Compression::LJ2K(None), "{name}");
            assert_eq!(decoded.size, expected.size, "{name}");
            assert_eq!(
                decoded.attributes.layer_position, expected.attributes.layer_position,
                "{name}"
            );
            let channels = &decoded.channel_data.list;
            let expected_channels = &expected.channel_data.list;
            assert_eq!(channels.len(), expected_channels.len(), "{name}");

            let infos: Vec<ChannelInfo> = channels
                .iter()
                .map(|channel| ChannelInfo {
                    name: channel.name.to_string(),
                    sample_type: match channel.sample_data {
                        FlatSamples::F16(_) => SampleType::F16,
                        FlatSamples::F32(_) => SampleType::F32,
                        FlatSamples::U32(_) => SampleType::U32,
                    },
                    sampling: (1, 1),
                })
                .collect();
            let lossy = lossy_channels(&infos);
            for ((channel, expected), (info, lossy)) in
                channels.iter().zip(expected_channels).zip(infos.iter().zip(lossy))
            {
                assert_eq!(channel.name, expected.name, "{name}");
                reports.push(compare_channel(
                    format!("{name} {}", info.name),
                    info.sample_type,
                    lossy,
                    &sample_bits(&channel.sample_data),
                    &sample_bits(&expected.sample_data),
                ));
            }
        }
    }

    for report in reports
        .iter()
        .filter(|report| report.lossy || report.differing > 0 || report.violation.is_some())
    {
        eprintln!("{report}");
    }
    for (writer, legacy) in [("3.5.3", false), ("3.5.1", true)] {
        let of_writer =
            reports.iter().filter(|report| report.label.contains("_lj2k351_q") == legacy);
        for sample_type in [SampleType::F16, SampleType::F32] {
            let (samples, differing) = totals(of_writer.clone(), sample_type);
            let max_ulps = of_writer
                .clone()
                .filter(|report| report.lossy && report.sample_type == sample_type)
                .map(|report| report.max_ulps)
                .max()
                .unwrap_or(0);
            eprintln!(
                "files of OpenEXR {writer}, lossy {sample_type:?} channels: {differing} of \
                 {samples} samples differ ({:.3} %), by at most {max_ulps} ULP",
                100.0 * differing as f64 / samples.max(1) as f64
            );
        }
    }
    let failures: Vec<String> = reports
        .iter()
        .filter(|report| report.violation.is_some())
        .map(ToString::to_string)
        .collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// The payload of every chunk of a single-part file: its start and size.
fn chunk_payloads(bytes: &[u8]) -> Vec<(usize, usize)> {
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
            let start = if tiled {
                offset + 20
            } else {
                offset + 8
            };
            (start, int(start - 4) as usize)
        })
        .collect()
}

/// OpenEXR 3.5.1 marks its LJ2K chunks with "HT", and 3.5.3 with "HL";
/// exrs undoes the legacy transfer function only on the former. Marking a
/// legacy file's chunks "HL" makes exrs decode the codestream's integers
/// as the bits of the samples, which changes the lossy channels and only
/// them.
#[test]
fn ht_magic_marks_legacy_lj2k() {
    for path in lj2k_files() {
        let bytes = std::fs::read(&path).unwrap();
        let magic = if is_legacy(&path) {
            b"HT"
        } else {
            b"HL"
        };
        let coded = chunk_payloads(&bytes)
            .into_iter()
            .filter(|&(start, _)| &bytes[start..start + 2] == magic)
            .count();
        assert!(coded > 0 || path.ends_with("rgb_half_lj2k_q150.exr"), "{}", path.display());
    }

    let name = "rgb_float_specials_lj2k351_q110.exr";
    let mut bytes = std::fs::read(dir().join(name)).unwrap();
    let legacy = read_bytes(&bytes, true).unwrap();
    for (start, _) in chunk_payloads(&bytes) {
        assert_eq!(&bytes[start..start + 2], b"HT");
        bytes[start + 1] = b'L';
    }
    let plain = read_bytes(&bytes, true).unwrap();
    let channels = &legacy.layer_data[0].channel_data.list;
    let plain_channels = &plain.layer_data[0].channel_data.list;
    for (channel, plain_channel) in channels.iter().zip(plain_channels) {
        let lossy = ["R", "G", "B"].contains(&channel.name.to_string().as_str());
        let same = sample_bits(&channel.sample_data) == sample_bits(&plain_channel.sample_data);
        assert_eq!(same, !lossy, "{name} {}", channel.name);
    }
}

/// HTJ2K chunks must carry "HT", and "HL" in them is damage.
#[test]
fn hl_magic_htj2k_is_invalid() {
    let mut bytes = std::fs::read(dir().join("y_half_htj2k256.exr")).unwrap();
    for (start, _) in chunk_payloads(&bytes) {
        assert_eq!(&bytes[start..start + 2], b"HT");
        bytes[start + 1] = b'L';
    }
    match read_bytes(&bytes, true) {
        Err(Error::Invalid(message)) => assert!(message.contains("magic"), "{}", message),
        other => panic!("{:?}", other.map(|_| ())),
    }
}

/// Damages the LJ2K chunks of each file, legacy ones included, and reads it.
/// Any outcome but a panic passes: the damage may go unnoticed, or be reported
/// as an error.
#[test]
fn damaged_chunks_do_not_panic() {
    let mut rng = StdRng::seed_from_u64(0x4C4A_324B);
    let mut failures = Vec::new();

    for path in lj2k_files() {
        let original = std::fs::read(&path).unwrap();
        // damage only coded chunks, past the header and offset table
        let coded: Vec<(usize, usize)> = chunk_payloads(&original)
            .into_iter()
            .filter(|&(start, _)| {
                let magic = &original[start..start + 2];
                magic == b"HL" || (is_legacy(&path) && magic == b"HT")
            })
            .collect();
        if coded.is_empty() {
            continue;
        }

        for round in 0..32 {
            let mut bytes = original.clone();
            let (start, size) = coded[rng.gen_range(0..coded.len())];
            let end = start + size;
            match round % 4 {
                0 => {
                    // cut the file inside the chunk
                    bytes.truncate(rng.gen_range(start..end));
                }
                1 => {
                    for _ in 0..rng.gen_range(1..8) {
                        let index = rng.gen_range(start..end);
                        bytes[index] = rng.gen();
                    }
                }
                2 => {
                    for _ in 0..rng.gen_range(1..4) {
                        let index = rng.gen_range(start..end);
                        bytes[index] ^= 1 << rng.gen_range(0..8);
                    }
                }
                _ => {
                    // shorten the chunk's codestream but not the file: the
                    // size field before the payload says fewer bytes
                    let new_size = rng.gen_range(1..size) as i32;
                    bytes[start - 4..start].copy_from_slice(&new_size.to_le_bytes());
                }
            }

            let pedantic = round % 2 == 0;
            let result = catch_unwind(AssertUnwindSafe(|| read_bytes(&bytes, pedantic)));
            if result.is_err() {
                failures.push(format!("{} round {}", path.display(), round));
            }
        }
    }

    assert!(failures.is_empty(), "panicked:\n{}", failures.join("\n"));
}
