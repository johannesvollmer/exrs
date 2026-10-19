// Test and bench support for the DWA DCT kernels, plus the opt-in SIMD tier
// correctness tests.
//
// The helpers stay public because benches/dct.rs is a separate crate and
// reaches them through the public API (hence the module is `simd-benches`-gated
// as well as `test`-gated); the tier tests below pick them up via `test`.

use super::{dct_forward_8x8_batch, dct_inverse_8x8_batch};

// Only the in-crate tier tests use this, so it need not be `pub`.
#[allow(unused)]
fn assert_blocks_match(autovectorized: fn(&mut [f32; 64]), kernel: impl Fn(&mut [f32; 64])) {
    // Compare via the crate's one definitive approximate-float helper rather
    // than a bespoke tolerance loop.
    use crate::image::validate_results::ValidateResult;

    for mut expected in pseudo_random_blocks(64) {
        let mut actual = expected;
        autovectorized(&mut expected);
        kernel(&mut actual);

        expected.to_vec().assert_approx_equals_result(&actual.to_vec());
    }
}

// Deterministic blocks in the ballpark of half-precision DCT coefficients
// (xorshift64, no `rand` dependency. Shared by the correctness tests below
// and by the forced-tier benchmark in benches/dct.rs.
// Stays `pub` (unlike `assert_blocks_match`) only because the benchmark needs
// it.
#[allow(unused)]
pub fn pseudo_random_blocks(count: usize) -> Vec<[f32; 64]> {
    let mut state: u64 = 0x9e3779b97f4a7c15;

    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (((state >> 40) as i32 as f32) / (i32::MAX as f32)) * 1024.0
    };

    (0..count).map(|_| std::array::from_fn(|_| next())).collect()
}

#[allow(unused)]
fn dct_forward_8x8(data: &mut [f32; 64]) {
    dct_forward_8x8_batch(std::iter::once(data));
}

#[allow(unused)]
fn dct_inverse_8x8(data: &mut [f32; 64]) {
    dct_inverse_8x8_batch(std::iter::once(data));
}

// AVX2 tier correctness tests. Opt-in via the `avx2-tests` feature and meant to
// run under a CPU (or SDE emulator) exposing AVX2/FMA; runtime dispatch is left
// intact, so `V3::try_new` decides availability.
#[cfg(all(test, feature = "avx2-tests"))]
mod avx2_tests {
    use std::path::Path;

    use pulp::x86::V3;

    use super::{
        super::{dct_forward_8x8_autovectorized, dct_inverse_8x8_autovectorized, x86::avx2},
        assert_blocks_match,
    };
    use crate::{image::validate_results::ValidateResult, prelude::*};

    #[test]
    fn avx2_inverse_matches_autovectorized() {
        assert_blocks_match(dct_inverse_8x8_autovectorized, |data| {
            avx2::dct_inverse_8x8(expect_avx2(), data)
        });
    }

    #[test]
    fn avx2_forward_matches_autovectorized() {
        assert_blocks_match(dct_forward_8x8_autovectorized, |data| {
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

        // Match the ground truth's lossless encoding so "validate_result"
        // compares the samples bit-for-bit instead of with a lossy
        // tolerance.
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

    use super::{
        super::{dct_forward_8x8_autovectorized, dct_inverse_8x8_autovectorized, x86::sse2},
        assert_blocks_match,
    };

    #[test]
    fn assert_sse2_close_to_autovectorized_reference() {
        assert_blocks_match(dct_inverse_8x8_autovectorized, |data| {
            sse2::dct_inverse_8x8(expect_sse2_without_avx2(), data)
        });
    }

    #[test]
    fn assert_sse2_forward_close_to_autovectorized_reference() {
        assert_blocks_match(dct_forward_8x8_autovectorized, |data| {
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

// Always-on (not SIMD-tier-gated) roundtrip tests: forward DCT followed by
// inverse DCT must recover the original block. Runs whatever tier the runtime
// dispatch selects on the host CPU.
#[cfg(test)]
mod roundtrip_tests {
    use super::{
        super::{
            dct_forward_8x8_autovectorized, dct_forward_8x8_batch, dct_inverse_8x8_autovectorized,
            dct_inverse_8x8_batch,
        },
        pseudo_random_blocks,
    };
    use crate::image::validate_results::ValidateResult;

    fn sample_blocks() -> Vec<[f32; 64]> {
        // A simple hardcoded ramp block plus deterministic pseudo-random
        // blocks.
        let ramp = std::array::from_fn(|i| i as f32 - 32.0);
        let mut blocks = vec![ramp];
        blocks.extend(pseudo_random_blocks(32));
        blocks
    }

    /// The scalar reference forward and inverse kernels compose to an
    /// approximate identity (the forward is a normalized DCT-II, the inverse is
    /// OpenEXR's scalar IDCT with a truncated PI — which is what the codec
    /// depends on).
    #[test]
    fn autovectorized_forward_inverse_is_identity() {
        for original in sample_blocks() {
            let mut block = original;
            dct_forward_8x8_autovectorized(&mut block);
            dct_inverse_8x8_autovectorized(&mut block);
            original.to_vec().assert_approx_equals_result(&block.to_vec());
        }
    }

    /// Same identity through the batch dispatch entry points (SIMD or scalar,
    /// depending on the host CPU).
    #[test]
    fn batch_forward_inverse_is_identity() {
        let originals = sample_blocks();
        let mut blocks = originals.clone();

        dct_forward_8x8_batch(blocks.iter_mut());
        dct_inverse_8x8_batch(blocks.iter_mut());

        for (original, roundtripped) in originals.iter().zip(&blocks) {
            original.to_vec().assert_approx_equals_result(&roundtripped.to_vec());
        }
    }
}

// Keep the original direct 2D formula as an independent reference for the
// separable kernels. Codec-level lossy tolerances are too loose for this check.
#[cfg(test)]
mod forward_tests {
    use super::{super::dct_forward_8x8_autovectorized, pseudo_random_blocks};

    #[test]
    fn forward_matches_direct_formula() {
        let mut blocks = vec![
            [0.0; 64],
            [1.0; 64],
            [-65504.0; 64],
            std::array::from_fn(|i| i as f32 - 32.0),
            std::array::from_fn(|i| {
                if (i / 8 + i % 8) % 2 == 0 {
                    1.0
                } else {
                    -1.0
                }
            }),
        ];
        // Every impulse position exercises a different pair of basis rows.
        for i in 0..64 {
            let mut block = [0.0; 64];
            block[i] = 1.0;
            blocks.push(block);
        }
        blocks.extend(
            pseudo_random_blocks(64)
                .into_iter()
                .map(|block| block.map(|value| (value - 4.0) * 8192.0)),
        );

        for original in blocks {
            let mut expected = original;
            forward_reference(&mut expected);
            // Summation order changes, especially near cancellation. Bound the
            // absolute error relative to input magnitude, not each coefficient.
            let tolerance = original.iter().fold(1.0f32, |max, x| max.max(x.abs())) * 1e-5;
            let check = |actual: [f32; 64]| {
                for (i, (&expected, &actual)) in expected.iter().zip(&actual).enumerate() {
                    assert!(
                        (expected - actual).abs() <= tolerance,
                        "coefficient {}: expected {}, got {}, tolerance {}",
                        i,
                        expected,
                        actual,
                        tolerance
                    );
                }
            };

            let mut actual = original;
            dct_forward_8x8_autovectorized(&mut actual);
            check(actual);

            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                use super::super::x86::{avx2, sse2};
                if let Some(v1) = pulp::x86::V1::try_new() {
                    let mut actual = original;
                    sse2::dct_forward_8x8(v1, &mut actual);
                    check(actual);
                }
                if let Some(v3) = pulp::x86::V3::try_new() {
                    let mut actual = original;
                    avx2::dct_forward_8x8_batch(v3, std::iter::once(&mut actual));
                    check(actual);
                }
            }
        }
    }

    fn forward_reference(data: &mut [f32; 64]) {
        const PI: f32 = 3.14159;
        const INV_SQRT_2: f32 = 0.70710677;

        let input = *data;
        for v in 0..8 {
            for u in 0..8 {
                let cu = if u == 0 {
                    INV_SQRT_2
                } else {
                    1.0
                };
                let cv = if v == 0 {
                    INV_SQRT_2
                } else {
                    1.0
                };
                let mut sum = 0.0f32;

                for y in 0..8 {
                    let cy = (((2 * y + 1) as f32 * v as f32 * PI) / 16.0).cos();
                    for x in 0..8 {
                        let cx = (((2 * x + 1) as f32 * u as f32 * PI) / 16.0).cos();
                        sum += input[y * 8 + x] * cx * cy;
                    }
                }

                data[v * 8 + u] = 0.25 * cu * cv * sum;
            }
        }
    }
}
