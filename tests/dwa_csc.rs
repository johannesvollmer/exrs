// Cross-checks exrs's DWA decoder against the real OpenEXR C++ library for
// images with more than one LOSSY_DCT channel group per chunk:
//
// - y_ry_by_dwaa.exr:    three standalone channels (Y, RY, BY) - never
//   CSC-grouped (cscIdx == -1 in internal_dwa_classifier.h), so this
//   exercises three standalone-group decodes in a row.
// - rgb_plus_y_dwaa.exr: an R/G/B CSC triplet followed by a standalone Y
//   channel, exercising the transition from a CSC group to a standalone one.
//
// Both scenarios previously hit a real bug: the DC buffer is planar across
// *all* groups in the chunk, but every group was read starting from offset 0
// instead of a running cursor, so groups after the first read another
// group's DC values. Fixed via the `dc_cursor` in `decode_lossy_dct_group`
// (src/compression/dwa/mod.rs).
//
// Each fixture is paired with a `*_ground_truth.exr`: an uncompressed EXR
// holding the real OpenEXR library's own decode of the DWAA file (see
// generate.py in tests/images/valid/custom/dwa_csc/), so this test has no
// Python/OpenEXR runtime dependency - just a frozen reference decode.

use std::path::Path;

use exr::{ image::validate_results::ValidateResult, prelude::* };

fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/dwa_csc")
}

// The ground truth was produced by an OpenEXR build that picked its avx
// IDCT kernel. This code dispatches to the same kernel hierarchy at runtime
// (src/compression/dwa/idct.rs), so on an AVX2-capable machine the decode
// must be bit-identical. On lower tiers exrs correctly picks a different
// kernel (sse2/scalar) whose output legitimately differs from the avx-made
// ground truth by a few ULP, so there is nothing meaningful to compare and
// the test is skipped.
fn ground_truth_simd_tier_available() -> bool {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // same token the decoder's dispatch uses for its avx tier
        pulp::x86::V3::try_new().is_some()
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        false
    }
}

fn check_against_real_openexr(exr_name: &str, ground_truth_name: &str) {
    if !ground_truth_simd_tier_available() {
        eprintln!("skipping: this CPU lacks the SIMD tier the ground truth was generated with");
        return;
    }

    let mut decoded = read_first_flat_layer_from_file(dir().join(exr_name)).expect(
        "exrs failed to decode DWA fixture"
    );

    let ground_truth = read_first_flat_layer_from_file(dir().join(ground_truth_name)).expect(
        "uncompressed ground truth could not be loaded"
    );

    // Match the ground truth's lossless encoding so "validate_result"
    // compares the samples bit-for-bit instead of with a lossy tolerance.
    decoded.layer_data.encoding = ground_truth.layer_data.encoding;

    ground_truth.assert_equals_result(&decoded);
}

#[test]
fn dwa_three_standalone_lossy_dct_groups() {
    check_against_real_openexr("y_ry_by_dwaa.exr", "y_ry_by_dwaa_ground_truth.exr");
}

#[test]
fn dwa_csc_group_then_standalone_group() {
    check_against_real_openexr("rgb_plus_y_dwaa.exr", "rgb_plus_y_dwaa_ground_truth.exr");
}
