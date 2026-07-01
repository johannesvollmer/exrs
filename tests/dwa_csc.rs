// Cross-checks exrs's DWA decoder against the real OpenEXR C++ library for
// images containing more than one LOSSY_DCT channel group per chunk:
//
// - y_ry_by_dwaa.exr:    three standalone LOSSY_DCT channels (Y, RY, BY).
//   None of these are ever CSC-grouped (they have cscIdx == -1 in
//   internal_dwa_classifier.h's sDefaultChannelRules/sLegacyChannelRules),
//   so decoding them exercises three standalone-group decodes in sequence.
// - rgb_plus_y_dwaa.exr: an R/G/B CSC triplet followed by a standalone Y
//   channel, exercising the transition from a 3-component CSC group to a
//   subsequent standalone group.
//
// Both scenarios previously triggered a real bug: the DC coefficient buffer
// is planar across *all* LOSSY_DCT groups in the chunk (CSC groups first,
// then standalone channels, each contributing `num_components * num_blocks`
// values), but the decoder read every group starting from offset 0 in that
// buffer instead of from the running cursor - so every group after the
// first read another group's DC values. See the `dc_cursor` fix in
// `decode_lossy_dct_group` (src/compression/dwa/mod.rs).
//
// The fixtures in tests/images/valid/custom/dwa_csc/ were written with the
// real OpenEXR library (see generate.py in that directory); the accompanying
// .bin files are that same library's own decoded pixel values (f32 little
// endian, one plane per channel in the order listed below), so this test has
// no Python/OpenEXR runtime dependency - it just compares exrs's output to a
// frozen reference decode.

use std::path::Path;

use exr::prelude::*;

fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/dwa_csc")
}

fn read_ground_truth(bin_path: &Path, channel_count: usize, pixel_count: usize) -> Vec<Vec<f32>> {
    let bytes = std::fs::read(bin_path).expect("ground truth file missing");
    assert_eq!(bytes.len(), channel_count * pixel_count * 4, "ground truth size mismatch");

    bytes
        .chunks_exact(pixel_count * 4)
        .map(|channel_bytes| {
            channel_bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        })
        .collect()
}

// LOSSY_DCT is, as the name says, lossy: exrs's scalar IDCT (src/compression/
// dwa/idct.rs) does not reproduce the reference implementation bit-for-bit in
// every last case (a known, separately tracked, pre-existing rounding
// difference affecting an occasional sample - see the DWA fixtures in
// across_compression.rs, which have one such mismatch each). This test's
// purpose is to catch *structural* bugs (like the DC cursor bug above), which
// produce large, widespread differences, not to chase that last-bit rounding
// gap - so a tiny fraction of samples are allowed a small tolerance.
const MAX_ALLOWED_DIFF: f32 = 0.001;
const MAX_ALLOWED_MISMATCH_FRACTION: f64 = 0.01;

fn check_against_real_openexr(exr_name: &str, bin_name: &str, channel_names: &[&str]) {
    let image = read_first_flat_layer_from_file(dir().join(exr_name))
        .expect("exrs failed to decode DWA fixture");

    let pixel_count = image.layer_data.size.area();
    let ground_truth = read_ground_truth(&dir().join(bin_name), channel_names.len(), pixel_count);

    for (channel_name, expected) in channel_names.iter().copied().zip(ground_truth.iter()) {
        let channel = image
            .layer_data
            .channel_data
            .list
            .iter()
            .find(|c| c.name == *channel_name)
            .unwrap_or_else(|| panic!("channel {} not found in decoded image", channel_name));

        let actual: Vec<f32> = match &channel.sample_data {
            FlatSamples::F16(samples) => samples.iter().map(|s| s.to_f32()).collect(),
            FlatSamples::F32(samples) => samples.clone(),
            FlatSamples::U32(samples) => samples.iter().map(|&s| s as f32).collect(),
        };

        assert_eq!(actual.len(), expected.len(), "channel {} length mismatch", channel_name);

        let mut mismatches = 0usize;
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            let diff = (a - e).abs();
            assert!(
                diff <= MAX_ALLOWED_DIFF,
                "channel {} sample [{}]: exrs decoded {}, real OpenEXR decoded {} (diff {})",
                channel_name, i, a, e, diff
            );
            if diff > 0.0 {
                mismatches += 1;
            }
        }

        let mismatch_fraction = mismatches as f64 / actual.len() as f64;
        assert!(
            mismatch_fraction <= MAX_ALLOWED_MISMATCH_FRACTION,
            "channel {}: {} of {} samples differ from real OpenEXR (fraction {:.4}), \
             which is too many to be the known last-bit rounding gap",
            channel_name, mismatches, actual.len(), mismatch_fraction
        );
    }
}

#[test]
fn dwa_three_standalone_lossy_dct_groups() {
    check_against_real_openexr("y_ry_by_dwaa.exr", "y_ry_by_dwaa.bin", &["Y", "RY", "BY"]);
}

#[test]
fn dwa_csc_group_then_standalone_group() {
    check_against_real_openexr("rgb_plus_y_dwaa.exr", "rgb_plus_y_dwaa.bin", &["R", "G", "B", "Y"]);
}
