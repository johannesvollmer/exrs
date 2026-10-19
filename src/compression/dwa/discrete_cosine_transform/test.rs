// DWA DCT correctness tests and deterministic inputs shared with
// benches/dct.rs. Public only so the benchmark can use the input generator.

// Deterministic blocks in the ballpark of half-precision DCT coefficients
// (xorshift64, no `rand` dependency).
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

// OpenEXR fixtures exercise multiple lossy groups sharing the AC/DC streams.
#[cfg(test)]
mod openexr_tests {
    use std::path::Path;

    use crate::{image::validate_results::ValidateResult, prelude::*};

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
        let decoded = read_first_flat_layer_from_file(dir().join(exr_name))
            .expect("exrs failed to decode DWA fixture");

        let mut ground_truth = read_first_flat_layer_from_file(dir().join(ground_truth_name))
            .expect("uncompressed ground truth could not be loaded");

        // Compare the same metadata, allowing the pixel rounding differences
        // between OpenEXR's SIMD inverse and our autovectorized scalar inverse.
        // Keep DWA encoding so validation uses its normal lossy pixel
        // tolerance.
        ground_truth.layer_data.encoding = decoded.layer_data.encoding;

        ground_truth.assert_equals_result(&decoded);
    }
}

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

    /// Same identity through the batch entry points.
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

    /// Runtime-selected transforms preserve the baseline arithmetic, including
    /// when an iterator skips blocks that do not need a transform.
    #[test]
    fn batch_matches_baseline() {
        for count in [0, 1, 33] {
            let originals = sample_blocks();
            let originals = &originals[..count];
            for inverse in [false, true] {
                let mut expected = originals.to_vec();
                for block in expected.iter_mut().step_by(2) {
                    if inverse {
                        dct_inverse_8x8_autovectorized(block);
                    } else {
                        dct_forward_8x8_autovectorized(block);
                    }
                }

                let mut actual = originals.to_vec();
                if inverse {
                    dct_inverse_8x8_batch(actual.iter_mut().step_by(2));
                } else {
                    dct_forward_8x8_batch(actual.iter_mut().step_by(2));
                }
                assert_eq!(actual, expected);
            }
        }
    }
}

// Keep the original direct 2D formula as an independent reference for the
// separable transform. Codec-level lossy tolerances are too loose for this
// check.
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
            let mut actual = original;
            dct_forward_8x8_autovectorized(&mut actual);
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
