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
// Python/OpenEXR runtime dependency - just a frozen reference decode. This
// reuses exrs's own `ValidateResult` machinery (see `expect_eq_other` in
// tests/across_compression.rs) instead of a bespoke binary format and
// hand-rolled tolerance check.

use std::path::Path;

use exr::{image::validate_results::ValidateResult, prelude::*};

fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/dwa_csc")
}

// LOSSY_DCT is lossy: exrs's IDCT (src/compression/dwa/idct.rs) matches
// OpenEXR's *scalar* reference exactly, but doesn't reproduce a real OpenEXR
// decode bit-for-bit. Not an exrs bug - OpenEXR's own scalar and SIMD
// (SSE2/AVX) IDCT disagree with each other (basis-constant precision and
// summation order both differ; see the comments on `dct_inverse_8x8` in
// idct.rs), and real builds dispatch to SIMD by default, so this port
// differs from a typical real decode by 1-2 ULP in half precision on some
// samples. `ValidateResult`'s lossy-compression tolerance (see
// `f32::validate_result` in src/image/mod.rs) is here to catch *structural*
// bugs (like the DC cursor bug above), not to chase the inherent
// scalar/SIMD ambiguity.
fn check_against_real_openexr(exr_name: &str, ground_truth_name: &str) {
    let decoded = read_first_flat_layer_from_file(dir().join(exr_name))
        .expect("exrs failed to decode DWA fixture");

    let mut ground_truth = read_first_flat_layer_from_file(dir().join(ground_truth_name))
        .expect("uncompressed ground truth could not be loaded");

    // The ground truth was saved uncompressed; match `decoded`'s compression
    // so `validate_result` applies the lossy tolerance instead of failing on
    // an encoding mismatch (same pattern as `expect_eq_other` in
    // tests/across_compression.rs).
    ground_truth.layer_data.encoding.compression = decoded.layer_data.encoding.compression;

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
