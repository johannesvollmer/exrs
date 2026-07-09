#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
compile_error!("AVX2 SIMD tests require an x86 or x86_64 target");

use std::path::Path;

use exr::{
    compression::simd_test_support::{
        assert_avx2_available, assert_avx2_close_to_scalar_reference,
        assert_avx2_forward_close_to_scalar_reference, assert_dispatch_picks_avx2,
        assert_dispatch_picks_avx2_for_forward, selected_simd_tier,
    },
    image::validate_results::ValidateResult,
    prelude::*,
};

fn require_avx2() {
    eprintln!("selected SIMD tier: {:?}", selected_simd_tier());
    assert_avx2_available();
}

#[test]
fn avx2_idct_matches_scalar_reference() {
    require_avx2();
    assert_avx2_close_to_scalar_reference();
}

#[test]
fn avx2_fdct_matches_scalar_reference() {
    require_avx2();
    assert_avx2_forward_close_to_scalar_reference();
}

#[test]
fn dispatch_picks_avx2_when_available() {
    require_avx2();
    assert_dispatch_picks_avx2();
}

#[test]
fn dispatch_picks_avx2_for_forward_dct() {
    require_avx2();
    assert_dispatch_picks_avx2_for_forward();
}

fn dir() -> &'static Path {
    Path::new("tests/images/valid/custom/dwa_csc")
}

fn check_against_real_openexr(exr_name: &str, ground_truth_name: &str) {
    require_avx2();

    let mut decoded = read_first_flat_layer_from_file(dir().join(exr_name))
        .expect("exrs failed to decode DWA fixture");

    let ground_truth = read_first_flat_layer_from_file(dir().join(ground_truth_name))
        .expect("uncompressed ground truth could not be loaded");

    // Match the ground truth's lossless encoding so "validate_result" compares
    // the samples bit-for-bit instead of with a lossy tolerance.
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
