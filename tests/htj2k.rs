//! Decodes the HTJ2K files in `tests/images/valid/custom/htj2k`, which
//! OpenEXR wrote, and checks every sample bit for bit against the
//! uncompressed file each was converted from. See `generate.py` there.

use std::{
    io::Cursor,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
};

use exr::prelude::*;
use rand::{rngs::StdRng, Rng, SeedableRng};

const CASES: [&str; 5] = ["rgba_half", "rgb_float_uint", "mixed_rgb", "y_half", "tiled"];
const METHODS: [&str; 2] = ["htj2k32", "htj2k256"];

fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/htj2k")
}

fn compressed_files() -> impl Iterator<Item = PathBuf> {
    CASES.iter().flat_map(|case| {
        METHODS.iter().map(move |method| dir().join(format!("{case}_{method}.exr")))
    })
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

#[test]
fn decodes_samples_exactly() {
    for case in CASES {
        let expected = read_file(&dir().join(format!("{case}.exr")));
        for method in METHODS {
            let path = dir().join(format!("{case}_{method}.exr"));
            let decoded = read_file(&path);

            let compression = decoded.layer_data[0].encoding.compression;
            assert!(
                matches!(compression, Compression::HTJ2K32 | Compression::HTJ2K256),
                "{}: {:?}",
                path.display(),
                compression
            );
            assert_eq!(decoded.layer_data.len(), expected.layer_data.len());

            for (decoded, expected) in decoded.layer_data.iter().zip(&expected.layer_data) {
                assert_eq!(decoded.size, expected.size, "{}", path.display());
                assert_eq!(
                    decoded.attributes.layer_position,
                    expected.attributes.layer_position,
                    "{}",
                    path.display()
                );
                let decoded_channels = &decoded.channel_data.list;
                let expected_channels = &expected.channel_data.list;
                assert_eq!(decoded_channels.len(), expected_channels.len());
                for (decoded, expected) in decoded_channels.iter().zip(expected_channels) {
                    assert_eq!(decoded.name, expected.name, "{}", path.display());
                    let (decoded_bits, expected_bits) =
                        (sample_bits(&decoded.sample_data), sample_bits(&expected.sample_data));
                    assert_eq!(decoded_bits.len(), expected_bits.len());
                    if let Some(index) = (0..expected_bits.len())
                        .find(|&index| decoded_bits[index] != expected_bits[index])
                    {
                        panic!(
                            "{} channel {}: sample {} is {:#x}, expected {:#x}",
                            path.display(),
                            decoded.name,
                            index,
                            decoded_bits[index],
                            expected_bits[index]
                        );
                    }
                }
            }
        }
    }
}

/// Damages the chunks of each file and reads it. Any outcome but a panic
/// passes: the damage may go unnoticed, or be reported as an error.
#[test]
fn damaged_chunks_do_not_panic() {
    let mut rng = StdRng::seed_from_u64(0x4854_4A32);
    let mut failures = Vec::new();

    for path in compressed_files() {
        let original = std::fs::read(&path).unwrap();
        // leave the header and offset table alone: the chunks start with
        // the first HTJ2K chunk header
        let first_chunk =
            original.windows(2).position(|bytes| bytes == b"HT").expect("an HTJ2K chunk");

        for round in 0..48 {
            let mut bytes = original.clone();
            match round % 3 {
                0 => {
                    let end = rng.gen_range(first_chunk..bytes.len());
                    bytes.truncate(end);
                }
                1 => {
                    for _ in 0..rng.gen_range(1..8) {
                        let index = rng.gen_range(first_chunk..bytes.len());
                        bytes[index] = rng.gen();
                    }
                }
                _ => {
                    for _ in 0..rng.gen_range(1..4) {
                        let index = rng.gen_range(first_chunk..bytes.len());
                        bytes[index] ^= 1 << rng.gen_range(0..8);
                    }
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
