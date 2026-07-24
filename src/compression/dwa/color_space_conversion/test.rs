// Test and bench support for the DWA CSC kernels, plus the SIMD tier
// correctness tests. Mirrors discrete_cosine_transform/test.rs.

use super::{csc709_forward_8x8_batch, csc709_inverse_8x8_batch};

#[allow(unused)]
fn assert_blocks_match(
    autovectorized: fn(&mut [[f32; 64]; 3]),
    kernel: impl Fn(&mut [[f32; 64]; 3]),
) {
    use crate::image::validate_results::ValidateResult;

    for mut expected in pseudo_random_triplets(4096) {
        let mut actual = expected;
        autovectorized(&mut expected);
        kernel(&mut actual);

        for (expected, actual) in expected.iter().zip(actual.iter()) {
            expected.to_vec().assert_approx_equals_result(&actual.to_vec());
        }
    }
}

// Deterministic block triplets in the ballpark of nonlinear-space DCT input
// (xorshift64, no `rand` dependency). Shared by the correctness tests below
// and by any forced-tier benchmark.
#[allow(unused)]
pub fn pseudo_random_triplets(count: usize) -> Vec<[[f32; 64]; 3]> {
    let mut state: u64 = 0x2545f4914f6cdd1d;

    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (((state >> 40) as i32 as f32) / (i32::MAX as f32)) * 4.0
    };

    (0..count).map(|_| std::array::from_fn(|_| std::array::from_fn(|_| next()))).collect()
}

#[allow(unused)]
fn csc709_forward_8x8(data: &mut [[f32; 64]; 3]) {
    csc709_forward_8x8_batch(std::iter::once(data));
}

#[allow(unused)]
fn csc709_inverse_8x8(data: &mut [[f32; 64]; 3]) {
    csc709_inverse_8x8_batch(std::iter::once(data));
}

// AVX2 tier correctness tests.
#[cfg(all(test, feature = "avx2-tests"))]
mod avx2_tests {
    use pulp::x86::V3;

    use super::{
        super::{csc709_forward_8x8_autovectorized, csc709_inverse_8x8_autovectorized, x86::avx2},
        assert_blocks_match,
    };

    #[test]
    fn avx2_forward_matches_autovectorized() {
        assert_blocks_match(csc709_forward_8x8_autovectorized, |data| {
            avx2::csc709_forward_8x8(expect_avx2(), data)
        });
    }

    #[test]
    fn avx2_inverse_matches_autovectorized() {
        assert_blocks_match(csc709_inverse_8x8_autovectorized, |data| {
            avx2::csc709_inverse_8x8(expect_avx2(), data)
        });
    }

    fn expect_avx2() -> V3 {
        V3::try_new().expect("AVX2 SIMD mode requested, but the AVX2/FMA tier is unavailable")
    }
}

// SSE2 tier correctness tests.
#[cfg(all(test, feature = "sse2-tests"))]
mod sse2_tests {
    use pulp::x86::{V1, V3};

    use super::{
        super::{csc709_forward_8x8_autovectorized, csc709_inverse_8x8_autovectorized, x86::sse2},
        assert_blocks_match,
    };

    #[test]
    fn assert_sse2_forward_close_to_autovectorized_reference() {
        assert_blocks_match(csc709_forward_8x8_autovectorized, |data| {
            sse2::csc709_forward_8x8(expect_sse2_without_avx2(), data)
        });
    }

    #[test]
    fn assert_sse2_inverse_close_to_autovectorized_reference() {
        assert_blocks_match(csc709_inverse_8x8_autovectorized, |data| {
            sse2::csc709_inverse_8x8(expect_sse2_without_avx2(), data)
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

// Always-on (not SIMD-tier-gated)
#[cfg(test)]
mod roundtrip_tests {
    use super::{
        super::{
            csc709_forward_8x8_autovectorized, csc709_forward_8x8_batch,
            csc709_inverse_8x8_autovectorized, csc709_inverse_8x8_batch,
        },
        pseudo_random_triplets,
    };
    use crate::image::validate_results::ValidateResult;

    #[test]
    fn autovectorized_forward_inverse_is_identity() {
        for original in pseudo_random_triplets(32) {
            let mut block = original;
            csc709_forward_8x8_autovectorized(&mut block);
            csc709_inverse_8x8_autovectorized(&mut block);

            for (original, roundtripped) in original.iter().zip(block.iter()) {
                original.to_vec().assert_approx_equals_result(&roundtripped.to_vec());
            }
        }
    }

    /// Same identity through the batch dispatch entry points (SIMD or scalar,
    /// depending on the host CPU).
    #[test]
    fn batch_forward_inverse_is_identity() {
        let originals = pseudo_random_triplets(32);
        let mut blocks = originals.clone();

        csc709_forward_8x8_batch(blocks.iter_mut());
        csc709_inverse_8x8_batch(blocks.iter_mut());

        for (original, roundtripped) in originals.iter().zip(&blocks) {
            for (original, roundtripped) in original.iter().zip(roundtripped.iter()) {
                original.to_vec().assert_approx_equals_result(&roundtripped.to_vec());
            }
        }
    }
}
