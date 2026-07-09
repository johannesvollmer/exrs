// AVX2 tier correctness tests. Opt-in via the `avx2-tests` feature and meant to
// run under a CPU (or SDE emulator) exposing AVX2/FMA; runtime dispatch is left
// intact, so `V3::try_new` decides availability.
#[cfg(all(test, feature = "avx2-tests"))]
mod avx2_tests {
    use std::path::Path;

    use pulp::x86::V3;

    use super::super::{
        dct_forward_8x8_autovectorized, dct_inverse_8x8_autovectorized, testing, x86::avx2,
    };
    use crate::{image::validate_results::ValidateResult, prelude::*};

    #[test]
    fn avx2_inverse_matches_autovectorized() {
        testing::assert_blocks_match("AVX2 inverse DCT", dct_inverse_8x8_autovectorized, |data| {
            avx2::dct_inverse_8x8(expect_avx2(), data)
        });
    }

    #[test]
    fn avx2_forward_matches_autovectorized() {
        testing::assert_blocks_match("AVX2 forward DCT", dct_forward_8x8_autovectorized, |data| {
            avx2::dct_forward_8x8(expect_avx2(), data)
        });
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
}

// SSE2 tier correctness tests. Opt-in via the `sse2-tests` feature. These force
// the SSE2 fallback path, so they must run with AVX2 hidden (e.g. under an SDE
// CPU without AVX2); `expect_sse2_without_avx2` asserts that.
#[cfg(all(test, feature = "sse2-tests"))]
mod sse2_tests {
    use pulp::x86::{V1, V3};

    use super::super::{
        dct_forward_8x8_autovectorized, dct_inverse_8x8_autovectorized, testing, x86::sse2,
    };

    #[test]
    fn assert_sse2_close_to_autovectorized_reference() {
        testing::assert_blocks_match("SSE2 inverse DCT", dct_inverse_8x8_autovectorized, |data| {
            sse2::dct_inverse_8x8(expect_sse2_without_avx2(), data)
        });
    }

    #[test]
    fn assert_sse2_forward_close_to_autovectorized_reference() {
        testing::assert_blocks_match("SSE2 forward DCT", dct_forward_8x8_autovectorized, |data| {
            sse2::dct_forward_8x8(expect_sse2_without_avx2(), data)
        });
    }

    fn expect_sse2() -> V1 {
        V1::try_new().expect("SSE2 SIMD mode requested, but the SSE2 tier is unavailable")
    }

    fn expect_sse2_without_avx2() -> V1 {
        assert!(V3::try_new().is_none(), "SSE2 dispatch fallback test must run with AVX2 hidden");
        expect_sse2()
    }
}
