//! Gates for LJ2K writing, on the uncompressed sources of the fixtures in
//! `tests/images/valid/custom/htj2k` (see `generate.py` there).
//!
//! exrs's LJ2K chunks are those of OpenEXR 3.5.3's compressor on OpenJPH's
//! generic code, which the htj2k crate follows; `tests/lj2k_model.rs`
//! checks that byte for byte. The installed OpenEXR differs from that in
//! two ways that exrs does not follow, so the chunk comparisons here allow
//! them, and report them rather than fail:
//!
//! - It links OpenJPH 0.32.0, which picks its AVX2 kernels at run time where it
//!   can. They round some irreversible float conversions differently from the
//!   generic code, so the coded data of many lossy chunks differ, and with them
//!   the tile-part lengths in the TLM marker and at times whether a small chunk
//!   pays to code or is stored raw.
//! - At qualities from 97, where OpenEXR asks for step sizes of their own for
//!   components 1 and 2, OpenJPH 0.32.0 writes no QCC marker segments for them,
//!   and its QCD holds the step sizes of component 2, for components 0 and 1 as
//!   well. exrs writes the QCD and the two QCCs that OpenJPH 0.33.0 writes. The
//!   installed decoder reads both.
//!
//! Everything else must be identical: the chunk header, every other marker
//! segment of the main header (among them the SIZ, COD, NLT tables and the
//! padding comment), the quantisation below quality 97, raw chunks, and
//! every chunk that codes no lossy component. The one exception is the
//! OpenJPH version the codestream's first comment names, 0.33.0 in exrs's
//! chunks and 0.32.0 in OpenEXR's: it is checked apart and blanked out
//! before the chunks are compared (see `htj2k_common`).
//!
//! When these tests were written, the model of `tests/lj2k_model.rs` was
//! also run, outside the repository, on every chunk the tests here write
//! (each source at each quality of `QUALITIES`): exrs's chunks were the
//! model's, all 144 of them, and 31 of OpenEXR's 60 coded chunks were not.
//! So each difference these tests count is the installed OpenEXR's.
//!
//! The decoded samples follow the rules of `lj2k_common`: reversible
//! channels exactly, lossy ones within one unit in the last place.
//!
//! The tests that run OpenEXR need its `exrmetrics` on the `PATH` and are
//! skipped with a message without it.
//!
//! Needs the `htj2k` feature: `cargo test --features htj2k`.

#![cfg(feature = "htj2k")]

#[path = "htj2k_common/mod.rs"]
mod htj2k_common;
#[path = "lj2k_common/mod.rs"]
mod lj2k_common;

use std::{
    collections::BTreeMap,
    convert::TryInto,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
};

use exr::{meta::attribute::SampleType, prelude::*};
use lj2k_common::{compare_channel, lossy_channels, totals, ChannelInfo, ChannelReport};

fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/htj2k")
}

/// The qualities the tests against OpenEXR write each source with: each
/// class of quality, as in `generate.py`.
const QUALITIES: [f32; 6] = [1.0, 50.5, 96.0, 97.0, 110.0, 150.0];

/// The uncompressed sources, by name.
fn sources() -> Vec<(String, Vec<u8>)> {
    let mut sources: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter_map(|path| {
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            let stem = name.strip_suffix(".exr")?;
            let compressed = ["_htj2k", "_lj2k"].iter().any(|method| stem.contains(method));
            (!compressed).then(|| (stem.to_string(), std::fs::read(&path).unwrap()))
        })
        .collect();
    sources.sort();
    assert!(sources.len() >= 11, "sources missing: {:?}", sources.len());
    sources
}

/// Every LJ2K fixture: its source's name, its quality and its path.
fn fixtures() -> Vec<(String, f32, PathBuf)> {
    let mut fixtures: Vec<(String, f32, PathBuf)> = std::fs::read_dir(dir())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter_map(|path| {
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            let stem = name.strip_suffix(".exr")?;
            if stem.ends_with("_decoded") {
                return None;
            }
            let (source, quality) = stem.split_once("_lj2k_q")?;
            let quality = quality.replace('_', ".").parse().unwrap();
            Some((source.to_string(), quality, path))
        })
        .collect();
    fixtures.sort_by(|a, b| a.2.cmp(&b.2));
    assert!(fixtures.len() >= 18, "LJ2K fixtures missing: {:?}", fixtures);
    fixtures
}

fn read_bytes(bytes: &[u8]) -> Result<FlatImage> {
    read()
        .no_deep_data()
        .largest_resolution_level()
        .all_channels()
        .all_layers()
        .all_attributes()
        .pedantic()
        .from_buffered(Cursor::new(bytes))
}

/// `source` written by exrs with LJ2K at `quality`.
fn write_lj2k(source: &[u8], quality: f32) -> Result<Vec<u8>> {
    let mut image = read_bytes(source).unwrap();
    for layer in &mut image.layer_data {
        layer.encoding.compression = Compression::LJ2K(Some(quality));
    }
    let mut bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut bytes))?;
    Ok(bytes)
}

/// The channels of the single layer of a file, in file order, as
/// `lj2k_common` takes them.
fn channel_infos(image: &FlatImage) -> Vec<ChannelInfo> {
    assert_eq!(image.layer_data.len(), 1);
    image.layer_data[0]
        .channel_data
        .list
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
        .collect()
}

/// The bits of every sample, so that NaN payloads and the sign of zero count.
fn sample_bits(samples: &FlatSamples) -> Vec<u32> {
    match samples {
        FlatSamples::F16(values) => values.iter().map(|v| u32::from(v.to_bits())).collect(),
        FlatSamples::F32(values) => values.iter().map(|v| v.to_bits()).collect(),
        FlatSamples::U32(values) => values.clone(),
    }
}

/// Compares the samples of two decodings of one file, channel by channel,
/// by the rules of `lj2k_common`.
fn compare_images(label: &str, ours: &FlatImage, theirs: &FlatImage) -> Vec<ChannelReport> {
    let infos = channel_infos(theirs);
    let lossy = lossy_channels(&infos);
    let (ours, theirs) = (&ours.layer_data[0], &theirs.layer_data[0]);
    assert_eq!(ours.size, theirs.size, "{label}");
    assert_eq!(ours.channel_data.list.len(), infos.len(), "{label}");
    ours.channel_data
        .list
        .iter()
        .zip(&theirs.channel_data.list)
        .zip(infos.iter().zip(lossy))
        .map(|((ours, theirs), (info, lossy))| {
            assert_eq!(ours.name, theirs.name, "{label}");
            compare_channel(
                format!("{label} {}", info.name),
                info.sample_type,
                lossy,
                &sample_bits(&ours.sample_data),
                &sample_bits(&theirs.sample_data),
            )
        })
        .collect()
}

/// Where a chunk is: (0, first line) or a tile's (x, y).
type ChunkKey = (i32, i32);

/// The chunks of a single-part file without mip maps, by position.
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

    // the table ends where the first chunk in the file starts, which need
    // not be the chunk of its first entry
    let mut offsets = Vec::new();
    let mut table_end = usize::MAX;
    while position + 8 * offsets.len() < table_end {
        let at = position + 8 * offsets.len();
        let offset = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap()) as usize;
        table_end = table_end.min(offset);
        offsets.push(offset);
    }
    offsets
        .into_iter()
        .map(|offset| {
            let (key, start) = if tiled {
                ((int(offset), int(offset + 4)), offset + 20)
            } else {
                ((0, int(offset)), offset + 8)
            };
            let size = int(start - 4) as usize;
            (key, bytes[start..start + size].to_vec())
        })
        .collect()
}

/// An LJ2K chunk taken apart: the chunk header (magic number, sizes and
/// channel map), the marker segments of the codestream's main header,
/// each with its marker, and the rest, from the first tile part on.
struct CodedChunk<'c> {
    header: &'c [u8],
    segments: Vec<(u16, &'c [u8])>,
    rest: &'c [u8],
}

const TLM: u16 = 0xFF55;
const QCD: u16 = 0xFF5C;
const QCC: u16 = 0xFF5D;

impl<'c> CodedChunk<'c> {
    /// `None` for a chunk stored raw.
    fn parse(chunk: &'c [u8]) -> Option<Self> {
        if !chunk.starts_with(b"HL") {
            return None;
        }
        let u16_at = |at: usize| u16::from_be_bytes([chunk[at], chunk[at + 1]]);
        let payload = u32::from_be_bytes(chunk[2..6].try_into().unwrap()) as usize;
        let (header, codestream) = chunk.split_at(6 + payload);
        let mut position = header.len();
        assert_eq!(u16_at(position), 0xFF4F, "SOC");
        position += 2;
        let mut segments = Vec::new();
        while u16_at(position) != 0xFF90 {
            let length = usize::from(u16_at(position + 2));
            segments.push((u16_at(position), &chunk[position..position + 2 + length]));
            position += 2 + length;
        }
        let rest = &chunk[position..];
        debug_assert!(codestream.len() >= rest.len());
        Some(Self {
            header,
            segments,
            rest,
        })
    }

    fn segments_but(&self, markers: &[u16]) -> Vec<&'c [u8]> {
        self.segments
            .iter()
            .filter(|(marker, _)| !markers.contains(marker))
            .map(|&(_, segment)| segment)
            .collect()
    }

    fn segment(&self, marker: u16, component: Option<u8>) -> Option<&'c [u8]> {
        self.segments
            .iter()
            .find(|&&(found, segment)| {
                found == marker && component.is_none_or(|component| segment[4] == component)
            })
            .map(|&(_, segment)| segment)
    }
}

/// How a chunk exrs wrote compares with OpenEXR's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Comparison {
    Identical,
    /// Coded data and tile-part lengths differ: the AVX2 kernels.
    DataDiffers,
    /// As `DataDiffers`, and the quantisation is OpenJPH 0.32.0's.
    QuantisationAndDataDiffer,
    /// One of the two is stored raw: the AVX2 kernels gave another size.
    RawOrNot,
}

/// Compares two chunks of one position by the rules in the file's docs;
/// `Err` says what breaks them.
fn compare_chunks<'c>(
    ours: &'c [u8],
    theirs: &'c [u8],
    lossy: bool,
    quality: f32,
) -> std::result::Result<Comparison, String> {
    if ours == theirs {
        return Ok(Comparison::Identical);
    }
    if !lossy {
        return Err("a chunk that codes nothing lossily differs".to_string());
    }
    let (ours, theirs) = match (CodedChunk::parse(ours), CodedChunk::parse(theirs)) {
        (None, None) => return Err("raw chunks differ".to_string()),
        (Some(_), None) | (None, Some(_)) => return Ok(Comparison::RawOrNot),
        (Some(ours), Some(theirs)) => (ours, theirs),
    };
    if ours.header != theirs.header {
        return Err("chunk headers differ".to_string());
    }
    if ours.segments_but(&[TLM, QCD, QCC]) != theirs.segments_but(&[TLM, QCD, QCC]) {
        return Err("marker segments other than TLM, QCD and QCC differ".to_string());
    }

    // the QCD and QCC segments, or only the QCCs of components from 3 on
    let quantisation = |chunk: &CodedChunk<'c>, from_component_3: bool| -> Vec<&'c [u8]> {
        chunk
            .segments
            .iter()
            .filter(|&&(marker, segment)| match marker {
                QCD => !from_component_3,
                QCC => !from_component_3 || segment[4] > 2,
                _ => false,
            })
            .map(|&(_, segment)| segment)
            .collect()
    };
    if quantisation(&ours, false) == quantisation(&theirs, false) {
        if ours.rest == theirs.rest {
            return Err("only the TLM segment differs".to_string());
        }
        return Ok(Comparison::DataDiffers);
    }
    if quality < 97.0 {
        return Err("the quantisation differs below quality 97".to_string());
    }

    // OpenJPH 0.32.0: no QCC for components 1 and 2, and a QCD with the
    // step sizes of component 2, which exrs writes in its QCC for component
    // 2 (after the component index); the QCCs of the reversible components
    // from 3 on are exrs's
    let (Some(our_qcc), Some(their_qcd)) = (ours.segment(QCC, Some(2)), theirs.segment(QCD, None))
    else {
        return Err("quantisation segments are missing".to_string());
    };
    let as_openjph_0_32 = theirs.segment(QCC, Some(1)).is_none()
        && theirs.segment(QCC, Some(2)).is_none()
        && their_qcd[4..] == our_qcc[5..]
        && quantisation(&ours, true) == quantisation(&theirs, true);
    if !as_openjph_0_32 {
        return Err("the quantisation differs other than as OpenJPH 0.32.0's".to_string());
    }
    Ok(Comparison::QuantisationAndDataDiffer)
}

/// Compares every chunk of two files, with the OpenJPH version of their
/// comments checked and blanked out first. Returns how many chunks compare
/// how, and pushes what breaks the rules onto `failures`.
fn compare_files(
    label: &str,
    ours: &[u8],
    theirs: &[u8],
    lossy: bool,
    quality: f32,
    failures: &mut Vec<String>,
) -> BTreeMap<Comparison, usize> {
    let (ours, theirs) = (read_chunks(ours), read_chunks(theirs));
    assert_eq!(
        ours.keys().collect::<Vec<_>>(),
        theirs.keys().collect::<Vec<_>>(),
        "{label}: chunk positions"
    );
    let mut counts = BTreeMap::new();
    for (key, our_chunk) in &ours {
        let (our_chunk, their_chunk) =
            match htj2k_common::compare_normalised(our_chunk, &theirs[key]) {
                Ok(normalised) => normalised,
                Err(message) => {
                    failures.push(format!("{label} {key:?}: {message}"));
                    continue;
                }
            };
        match compare_chunks(&our_chunk, &their_chunk, lossy, quality) {
            Ok(comparison) => *counts.entry(comparison).or_insert(0) += 1,
            Err(message) => failures.push(format!("{label} {key:?}: {message}")),
        }
    }
    counts
}

fn print_counts(label: &str, counts: &BTreeMap<Comparison, usize>) {
    let total: usize = counts.values().sum();
    let identical = counts.get(&Comparison::Identical).copied().unwrap_or(0);
    let rest: Vec<String> = counts
        .iter()
        .filter(|(comparison, _)| **comparison != Comparison::Identical)
        .map(|(comparison, count)| format!("{count} {comparison:?}"))
        .collect();
    eprintln!("{label}: {identical} of {total} chunks identical; {}", rest.join(", "));
}

/// Whether `source` codes anything lossily with LJ2K.
fn has_lossy_channels(source: &FlatImage) -> bool {
    lossy_channels(&channel_infos(source)).contains(&true)
}

/// Gate 2 without OpenEXR: the committed LJ2K fixtures, which OpenEXR
/// wrote. exrs writes each source at the fixture's quality, and its chunks
/// must compare with the fixture's by the rules in the file's docs.
#[test]
fn chunks_match_committed_fixtures() {
    let mut failures = Vec::new();
    let mut all = BTreeMap::new();
    for (name, quality, path) in fixtures() {
        let source = std::fs::read(dir().join(format!("{name}.exr"))).unwrap();
        let lossy = has_lossy_channels(&read_bytes(&source).unwrap());
        let ours = write_lj2k(&source, quality).unwrap();
        let theirs = std::fs::read(&path).unwrap();
        let label = path.file_name().unwrap().to_str().unwrap();
        let counts = compare_files(label, &ours, &theirs, lossy, quality, &mut failures);
        print_counts(label, &counts);
        for (comparison, count) in counts {
            *all.entry(comparison).or_insert(0) += count;
        }
    }
    print_counts("all fixtures", &all);
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// exrs reads back what it writes, without OpenEXR: the channels LJ2K codes
/// reversibly come back exactly, and the lossy ones finite where the source
/// is.
#[test]
fn exrs_round_trip() {
    for (name, source) in sources() {
        let original = read_bytes(&source).unwrap();
        let lossy = lossy_channels(&channel_infos(&original));
        for quality in QUALITIES {
            let label = format!("{name} q{quality}");
            let written = write_lj2k(&source, quality).unwrap();
            let decoded =
                read_bytes(&written).unwrap_or_else(|error| panic!("{}: {}", label, error));
            assert_eq!(decoded.layer_data[0].encoding.compression, Compression::LJ2K(None));

            let (decoded, original) = (&decoded.layer_data[0], &original.layer_data[0]);
            assert_eq!(decoded.size, original.size, "{label}");
            for ((channel, expected), &lossy) in
                decoded.channel_data.list.iter().zip(&original.channel_data.list).zip(&lossy)
            {
                let (ours, theirs) =
                    (sample_bits(&channel.sample_data), sample_bits(&expected.sample_data));
                if !lossy {
                    assert!(ours == theirs, "{} {}: samples differ", label, channel.name);
                    continue;
                }
                // how near the source lossy samples come depends on the
                // quality (at quality 1 they are far off); the tests against
                // OpenEXR's decoding check them. Here: finite where the
                // source is finite and not near the largest floats, which
                // may round to infinity, in OpenEXR as well.
                let ours = channel.sample_data.values_as_f32();
                for (index, (ours, theirs)) in
                    ours.zip(expected.sample_data.values_as_f32()).enumerate()
                {
                    assert!(
                        ours.is_finite() || theirs.is_nan() || theirs.abs() >= 1e37,
                        "{} {} sample {}: {} for {}",
                        label,
                        channel.name,
                        index,
                        ours,
                        theirs
                    );
                }
            }
        }
    }
}

/// A bad quality is an error when exrs writes, never clamped.
#[test]
fn bad_quality_is_refused() {
    let (_, source) = sources().into_iter().find(|(name, _)| name == "rgb_half").unwrap();
    for quality in [0.0, 0.5, 150.5, 151.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        match write_lj2k(&source, quality) {
            Err(Error::Invalid(message)) => {
                assert!(message.contains("LJ2K quality"), "{}", message);
            }
            other => panic!("{}: {:?}", quality, other.map(|bytes| bytes.len())),
        }
    }
}

fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("lj2k_write");
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

/// Converts `input` with `exrmetrics --convert` and the arguments given.
fn exrmetrics_convert(input: &[u8], arguments: &[&str], name: &str) -> Vec<u8> {
    let dir = scratch_dir();
    let source = dir.join(format!("{name}_in.exr"));
    let target = dir.join(format!("{name}_out.exr"));
    std::fs::write(&source, input).unwrap();
    let _ = std::fs::remove_file(&target);
    let output = Command::new("exrmetrics")
        .arg("--convert")
        .args(arguments)
        .arg(&source)
        .arg("-o")
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{name}: exrmetrics {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let result = std::fs::read(&target).unwrap();
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&target);
    result
}

/// Prints the totals of lossy samples that differ, per sample type.
fn print_totals(label: &str, reports: &[ChannelReport]) {
    for sample_type in [SampleType::F16, SampleType::F32] {
        let (samples, differing) = totals(reports, sample_type);
        if samples > 0 {
            eprintln!(
                "{label}, lossy {sample_type:?}: {differing} of {samples} samples differ ({:.3} %)",
                100.0 * differing as f64 / samples as f64
            );
        }
    }
}

/// The gates against the installed OpenEXR, on every source at every
/// quality of `QUALITIES`:
///
/// 1. exrs decodes OpenEXR's LJ2K file as OpenEXR does (`-z none`);
/// 2. exrs's chunks compare with OpenEXR's by the rules in the file's docs;
/// 3. OpenEXR decodes exrs's LJ2K file as exrs does.
///
/// Gates 1 and 3 follow the rules of `lj2k_common`, and measure the
/// decoders. The test also measures the encoders, without a rule: exrs's
/// decoding of exrs's file against exrs's decoding of OpenEXR's. It prints
/// how many samples differ, per file and channel and in total per quality
/// and sample type.
#[test]
fn lj2k_against_openexr() {
    if !have_exrmetrics() {
        return;
    }
    let mut failures = Vec::new();
    let mut chunk_counts = BTreeMap::new();
    let mut by_quality: BTreeMap<String, [Vec<ChannelReport>; 3]> = BTreeMap::new();

    for (name, source) in sources() {
        let lossy = has_lossy_channels(&read_bytes(&source).unwrap());
        for quality in QUALITIES {
            let quality_text = format!("{quality}");
            let label = format!("{name} q{quality_text}");
            let ours = write_lj2k(&source, quality).unwrap();
            let theirs = exrmetrics_convert(&source, &["-z", "lj2k", "-l", &quality_text], &name);

            // gate 2
            let counts = compare_files(&label, &ours, &theirs, lossy, quality, &mut failures);
            print_counts(&label, &counts);
            for (comparison, count) in counts {
                *chunk_counts.entry(comparison).or_insert(0) += count;
            }

            let our_decoding = read_bytes(&ours).unwrap();
            let their_decoding = read_bytes(&theirs).unwrap();
            let openexr_decodes_ours =
                read_bytes(&exrmetrics_convert(&ours, &["-z", "none"], &name));
            let openexr_decodes_theirs =
                read_bytes(&exrmetrics_convert(&theirs, &["-z", "none"], &name));
            let openexr_decodes_ours = openexr_decodes_ours.unwrap();
            let openexr_decodes_theirs = openexr_decodes_theirs.unwrap();

            let reports = by_quality.entry(quality_text.clone()).or_default();
            let gate_1 = compare_images(&label, &their_decoding, &openexr_decodes_theirs);
            let gate_3 = compare_images(&label, &our_decoding, &openexr_decodes_ours);
            let encoders = compare_images(&label, &our_decoding, &their_decoding);
            for (kind, found) in
                [("decoding OpenEXR's file", &gate_1), ("decoding exrs's file", &gate_3)]
            {
                for report in found {
                    if report.differing > 0 || report.violation.is_some() {
                        eprintln!("  {kind}: {report}");
                    }
                    if report.violation.is_some() {
                        failures.push(format!("{kind}: {report}"));
                    }
                }
            }
            for report in &encoders {
                if report.differing > 0 {
                    eprintln!(
                        "  encoders: {} ({} of {} samples)",
                        report.label, report.differing, report.samples
                    );
                }
            }
            reports[0].extend(gate_1);
            reports[1].extend(gate_3);
            reports[2].extend(encoders);
        }
    }

    print_counts("all chunks", &chunk_counts);
    for (quality, [gate_1, gate_3, encoders]) in &by_quality {
        print_totals(&format!("q{quality}, exrs and OpenEXR decoding OpenEXR's file"), gate_1);
        print_totals(&format!("q{quality}, exrs and OpenEXR decoding exrs's file"), gate_3);
        print_totals(&format!("q{quality}, exrs decoding exrs's and OpenEXR's files"), encoders);
    }
    let all = |index: usize| -> Vec<ChannelReport> {
        by_quality.values().flat_map(|reports| reports[index].clone()).collect()
    };
    print_totals("all, exrs and OpenEXR decoding OpenEXR's file", &all(0));
    print_totals("all, exrs and OpenEXR decoding exrs's file", &all(1));
    print_totals("all, exrs decoding exrs's and OpenEXR's files", &all(2));
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
