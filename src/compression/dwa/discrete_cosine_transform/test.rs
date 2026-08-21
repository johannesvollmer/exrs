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

    for mut expected in pseudo_random_blocks(4096) {
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

    use miraculix::x86::{detect_features, ops::avx::avx::Avx};

    use super::{
        super::{dct_forward_8x8_autovectorized, dct_inverse_8x8_autovectorized, x86::avx},
        assert_blocks_match,
    };
    use crate::{image::validate_results::ValidateResult, prelude::*};

    #[test]
    fn avx2_inverse_matches_autovectorized() {
        assert_blocks_match(dct_inverse_8x8_autovectorized, |data| {
            avx::dct_inverse_8x8(expect_avx2(), data)
        });
    }

    #[test]
    fn avx2_forward_matches_autovectorized() {
        assert_blocks_match(dct_forward_8x8_autovectorized, |data| {
            avx::dct_forward_8x8(expect_avx2(), data)
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

    fn expect_avx2() -> Avx {
        Avx::from_features(detect_features())
            .expect("AVX SIMD mode requested, but the AVX tier is unavailable")
    }
}

// AVX-512 tier correctness tests (Stage 1 prototype; not yet wired into
// runtime dispatch). Opt-in via the `avx512-tests` feature; `V4::try_new`
// requires avx512f/bw/cd/dq/vl all present.
#[cfg(all(test, feature = "avx512-tests"))]
mod avx512_tests {
    use miraculix::x86::{
        detect_features,
        ops::{
            avx::avx::Avx,
            avx512::{avx512dq::Avx512Dq, avx512f::Avx512f},
        },
    };

    use super::{
        super::{dct_inverse_8x8_autovectorized, x86::avx512dq as avx512f},
        pseudo_random_blocks,
    };
    use crate::image::validate_results::ValidateResult;

    /// Compares the AVX-512 2-blocks-per-register kernel against the scalar
    /// reference applied to each block independently -> the AVX-512 kernel
    /// must produce the same result as if the two blocks had been decoded
    /// one at a time.
    fn assert_pairs_match(kernel: impl Fn(&mut [f32; 64], &mut [f32; 64])) {
        for pair in pseudo_random_blocks(4096).chunks_exact(2) {
            let mut expected_a = pair[0];
            let mut expected_b = pair[1];
            let mut actual_a = pair[0];
            let mut actual_b = pair[1];

            dct_inverse_8x8_autovectorized(&mut expected_a);
            dct_inverse_8x8_autovectorized(&mut expected_b);
            kernel(&mut actual_a, &mut actual_b);

            expected_a.to_vec().assert_approx_equals_result(&actual_a.to_vec());
            expected_b.to_vec().assert_approx_equals_result(&actual_b.to_vec());
        }
    }

    #[test]
    fn avx512_inverse_matches_autovectorized() {
        let (avx512f, avx, avx512dq) = expect_avx512();
        assert_pairs_match(|a, b| avx512f::dct_inverse_8x8_pair(avx512f, avx, avx512dq, a, b));
    }

    /// The hand-interleaved 2-pair ("quad") kernel must produce exactly the
    /// same result as two independent `inverse_pair` calls, interleaving
    /// the two chains' stages is a scheduling change only, not a math change.
    #[test]
    fn avx512_quad_matches_pair() {
        let (avx512f, avx, avx512dq) = expect_avx512();
        let blocks = pseudo_random_blocks(4096);
        for group in blocks.chunks_exact(4) {
            let mut expected = [group[0], group[1], group[2], group[3]];
            let mut actual = expected;

            let [ref mut ea0, ref mut eb0, ref mut ea1, ref mut eb1] = expected;
            avx512f::dct_inverse_8x8_pair(avx512f, avx, avx512dq, ea0, eb0);
            avx512f::dct_inverse_8x8_pair(avx512f, avx, avx512dq, ea1, eb1);

            let [ref mut a0, ref mut b0, ref mut a1, ref mut b1] = actual;
            avx512f::dct_inverse_8x8_quad(avx512f, avx, avx512dq, a0, b0, a1, b1);

            for (e, a) in expected.iter().zip(actual.iter()) {
                e.to_vec().assert_approx_equals_result(&a.to_vec());
            }
        }
    }

    /// Component-level dual-port shape (R∥G via `inverse_quad`, then B) must
    /// match three sequential `inverse_pair`s on the same RGB spatial pair.
    #[test]
    fn avx512_rgb_component_quad_matches_seq() {
        let (avx512f, avx, avx512dq) = expect_avx512();
        let blocks = pseudo_random_blocks(4096 * 6);
        let mut pairs: Vec<avx512f::RgbPairBlocks> =
            blocks.chunks_exact(6).map(|c| ([c[0], c[1], c[2]], [c[3], c[4], c[5]])).collect();
        let mut expected = pairs.clone();

        avx512f::dct_inverse_rgb_pair_components_seq(avx512f, avx, avx512dq, &mut expected);
        avx512f::dct_inverse_rgb_pair_components_quad(avx512f, avx, avx512dq, &mut pairs);

        for (e, a) in expected.iter().zip(pairs.iter()) {
            for c in 0..3 {
                e.0[c].to_vec().assert_approx_equals_result(&a.0[c].to_vec());
                e.1[c].to_vec().assert_approx_equals_result(&a.1[c].to_vec());
            }
        }
    }

    fn expect_avx512() -> (Avx512f, Avx, Avx512Dq) {
        let features = detect_features();
        (
            Avx512f::from_features(features)
                .expect("AVX-512 SIMD mode requested, but the AVX-512F tier is unavailable"),
            Avx::from_features(features)
                .expect("AVX-512 SIMD mode requested, but the AVX tier is unavailable"),
            Avx512Dq::from_features(features)
                .expect("AVX-512 SIMD mode requested, but the AVX-512DQ tier is unavailable"),
        )
    }
}

// SSE2 tier correctness tests. Opt-in via the `sse2-tests` feature. These force
// the SSE2 fallback path, so they must run with AVX2 hidden (e.g. under an SDE
// CPU without AVX2); `expect_sse2_without_avx2` asserts that.
#[cfg(all(test, feature = "sse2-tests"))]
mod sse2_tests {
    use miraculix::x86::{
        detect_features,
        ops::{avx::avx::Avx, sse::sse::Sse},
    };

    use super::{
        super::{dct_forward_8x8_autovectorized, dct_inverse_8x8_autovectorized, x86::sse},
        assert_blocks_match,
    };

    #[test]
    fn assert_sse2_close_to_autovectorized_reference() {
        assert_blocks_match(dct_inverse_8x8_autovectorized, |data| {
            sse::dct_inverse_8x8(expect_sse_without_avx(), data)
        });
    }

    #[test]
    fn assert_sse2_forward_close_to_autovectorized_reference() {
        assert_blocks_match(dct_forward_8x8_autovectorized, |data| {
            sse::dct_forward_8x8(expect_sse_without_avx(), data)
        });
    }

    fn expect_sse() -> Sse {
        Sse::from_features(detect_features())
            .expect("SSE SIMD mode requested, but the SSE tier is unavailable")
    }

    fn expect_sse_without_avx() -> Sse {
        assert!(
            Avx::from_features(detect_features()).is_none(),
            "SSE dispatch fallback test must run with AVX hidden"
        );
        expect_sse()
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
        // A simple hardcoded ramp block plus deterministic pseudo-random blocks.
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
