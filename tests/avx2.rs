#![cfg(feature = "avx2-tests")]


use std::path::Path;
use pulp::x86::{V3};
use exr::{image::validate_results::ValidateResult, prelude::*};
use exr::compression::dwa::idct::*;
use exr::compression::dwa::idct::x86::*;

#[test]
pub fn avx2_inverse_matches_autovectorized() {
    testing::assert_blocks_match(
        "AVX2 inverse DCT",
        dct_inverse_8x8_autovectorized,
        |data| avx::dct_inverse_8x8(expect_avx2(), data),
    );
}

#[test]
pub fn avx2_forward_matches_autovectorized() {
    testing::assert_blocks_match(
        "AVX2 forward DCT",
        dct_forward_8x8_autovectorized,
        |data| avx::dct_forward_8x8(expect_avx2(), data),
    );
}


#[test]
fn dwa_three_standalone_lossy_dct_groups() {
    check_against_real_openexr("y_ry_by_dwaa.exr", "y_ry_by_dwaa_ground_truth.exr");
}

#[test]
fn dwa_csc_group_then_standalone_group() {
    check_against_real_openexr("rgb_plus_y_dwaa.exr", "rgb_plus_y_dwaa_ground_truth.exr");
}



fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/dwa_csc")
}

fn check_against_real_openexr(exr_name: &str, ground_truth_name: &str) {
    let _ = expect_avx2();

    let mut decoded = read_first_flat_layer_from_file(dir().join(exr_name))
        .expect("exrs failed to decode DWA fixture");

    let ground_truth = read_first_flat_layer_from_file(dir().join(ground_truth_name))
        .expect("uncompressed ground truth could not be loaded");

    // Match the ground truth's lossless encoding so "validate_result" compares
    // the samples bit-for-bit instead of with a lossy tolerance.
    decoded.layer_data.encoding = ground_truth.layer_data.encoding;

    ground_truth.assert_equals_result(&decoded);
}


fn expect_avx2() -> V3 {
    V3::try_new().expect("AVX2 SIMD mode requested, but the AVX2/FMA tier is unavailable")
}



