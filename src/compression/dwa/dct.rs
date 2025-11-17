//! DCT (Discrete Cosine Transform) for DWAA/DWAB compression.
//!
//! Uses 8x8 DCT-III for inverse transform (decompression).

use rustdct::{DctPlanner, Dct2, Dct3};
use smallvec::SmallVec;
use std::sync::{Arc, OnceLock};

/// Cached DCT-II transform for 8x8 blocks (forward, for testing)
fn dct8_forward() -> &'static Arc<dyn Dct2<f32>> {
    static DCT8: OnceLock<Arc<dyn Dct2<f32>>> = OnceLock::new();
    DCT8.get_or_init(|| {
        let mut planner = DctPlanner::new();
        planner.plan_dct2(8)
    })
}

/// Cached DCT-III transform for 8x8 blocks (inverse, for decompression)
fn dct8_inverse() -> &'static Arc<dyn Dct3<f32>> {
    static IDCT8: OnceLock<Arc<dyn Dct3<f32>>> = OnceLock::new();
    IDCT8.get_or_init(|| {
        let mut planner = DctPlanner::new();
        planner.plan_dct3(8)
    })
}

/// Perform inverse 8x8 DCT on a block of coefficients
///
/// The DCT coefficients are in "normal" order (not zigzag).
/// Uses DCT-III (inverse DCT) with a two-pass algorithm: first on rows, then on columns.
///
/// # Arguments
/// * `coeffs` - DCT coefficients in row-major order
///
/// # Returns
/// Spatial domain values (8x8 block)
pub fn inverse_dct_8x8(coeffs: &[f32; 64]) -> [f32; 64] {
    let idct = dct8_inverse();

    let mut temp = [0.0f32; 64];

    // First pass: inverse DCT (DCT-III) on rows
    for row in 0..8 {
        let row_start = row * 8;
        let mut row_buffer: SmallVec<[f32; 8]> = coeffs[row_start..row_start + 8]
            .iter()
            .copied()
            .collect();

        idct.process_dct3(&mut row_buffer);

        // Copy result to temp (transposed)
        for col in 0..8 {
            temp[col * 8 + row] = row_buffer[col];
        }
    }

    // Second pass: inverse DCT (DCT-III) on columns (which are now rows in temp due to transpose)
    let mut result = [0.0f32; 64];
    for col in 0..8 {
        let col_start = col * 8;
        let mut col_buffer: SmallVec<[f32; 8]> = temp[col_start..col_start + 8]
            .iter()
            .copied()
            .collect();

        idct.process_dct3(&mut col_buffer);

        // Copy result to final output with normalization (row-major order)
        // rustdct's 2D DCT-III needs /2 normalization to match OpenEXR's DC/8 behavior
        for row in 0..8 {
            result[row * 8 + col] = col_buffer[row] / 2.0;
        }
    }

    result
}

/// Zigzag scan order for 8x8 DCT blocks
const ZIGZAG: [usize; 64] = [
     0,  1,  8, 16,  9,  2,  3, 10,
    17, 24, 32, 25, 18, 11,  4,  5,
    12, 19, 26, 33, 40, 48, 41, 34,
    27, 20, 13,  6,  7, 14, 21, 28,
    35, 42, 49, 56, 57, 50, 43, 36,
    29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46,
    53, 60, 61, 54, 47, 55, 62, 63,
];

/// Optimized inverse DCT that can skip work based on last non-zero coefficient
///
/// If the last non-zero coefficient is at index `last_non_zero`, we can skip
/// processing high-frequency components that are all zero.
///
/// # Arguments
/// * `coeffs` - DCT coefficients in row-major order
/// * `last_non_zero` - Index of the last non-zero coefficient in zigzag order
///
/// # Returns
/// Spatial domain values (8x8 block)
pub fn inverse_dct_8x8_optimized(coeffs: &[f32; 64], last_non_zero: usize) -> [f32; 64] {
    if last_non_zero == 0 {
        // DC-only block - all values are the same
        // OpenEXR outputs DC / 8 for DC-only blocks
        let dc_value = coeffs[0] / 8.0;
        return [dc_value; 64];
    }

    // Determine how many rows we can skip based on the last non-zero coefficient
    // Find the row index (in normal order) of the last non-zero zigzag position
    let last_pos = ZIGZAG[last_non_zero];
    let last_row = last_pos / 8;

    // If all coefficients after a certain row are zero, we can skip those rows
    // in the first pass of the IDCT
    let rows_to_process = (last_row + 1).min(8);

    // If we can skip 3+ rows, use the optimized path
    if rows_to_process <= 5 {
        inverse_dct_8x8_partial(coeffs, 8 - rows_to_process)
    } else {
        inverse_dct_8x8(coeffs)
    }
}

/// Inverse DCT with partial row processing (like OpenEXR's zeroedRows parameter)
fn inverse_dct_8x8_partial(coeffs: &[f32; 64], zeroed_rows: usize) -> [f32; 64] {
    let idct = dct8_inverse();
    let rows_to_process = 8 - zeroed_rows;

    let mut temp = [0.0f32; 64];

    // First pass: inverse DCT (DCT-III) on non-zero rows only
    for row in 0..rows_to_process {
        let row_start = row * 8;
        let mut row_buffer: SmallVec<[f32; 8]> = coeffs[row_start..row_start + 8]
            .iter()
            .copied()
            .collect();

        idct.process_dct3(&mut row_buffer);

        // Copy result to temp (transposed)
        for col in 0..8 {
            temp[col * 8 + row] = row_buffer[col];
        }
    }

    // Zero out the rows we skipped (already initialized to 0.0)

    // Second pass: inverse DCT (DCT-III) on all columns
    let mut result = [0.0f32; 64];
    for col in 0..8 {
        let col_start = col * 8;
        let mut col_buffer: SmallVec<[f32; 8]> = temp[col_start..col_start + 8]
            .iter()
            .copied()
            .collect();

        idct.process_dct3(&mut col_buffer);

        // Copy result to final output with normalization (row-major order)
        // rustdct's 2D DCT-III needs /2 normalization to match OpenEXR's DC/8 behavior
        for row in 0..8 {
            result[row * 8 + col] = col_buffer[row] / 2.0;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // Forward DCT for testing (not used in actual decompression)
    fn forward_dct_8x8(spatial: &[f32; 64]) -> [f32; 64] {
        let dct = dct8_forward();

        let mut temp = [0.0f32; 64];

        // First pass: DCT-II on rows
        for row in 0..8 {
            let row_start = row * 8;
            let mut row_buffer: SmallVec<[f32; 8]> = spatial[row_start..row_start + 8]
                .iter()
                .copied()
                .collect();

            dct.process_dct2(&mut row_buffer);

            for col in 0..8 {
                temp[col * 8 + row] = row_buffer[col];
            }
        }

        // Second pass: DCT-II on columns
        let mut result = [0.0f32; 64];
        for col in 0..8 {
            let col_start = col * 8;
            let mut col_buffer: SmallVec<[f32; 8]> = temp[col_start..col_start + 8]
                .iter()
                .copied()
                .collect();

            dct.process_dct2(&mut col_buffer);

            // Copy result to final output with normalization (row-major order)
            for row in 0..8 {
                result[row * 8 + col] = col_buffer[row] / 8.0;
            }
        }

        result
    }

    #[test]
    fn test_dct_roundtrip() {
        // Create a test pattern
        let mut spatial = [0.0f32; 64];
        for i in 0..64 {
            spatial[i] = (i as f32).sin();
        }

        // Forward then inverse
        let freq = forward_dct_8x8(&spatial);
        let recovered = inverse_dct_8x8(&freq);

        // Check roundtrip accuracy
        for i in 0..64 {
            let error = (spatial[i] - recovered[i]).abs();
            assert!(
                error < 1e-4,
                "DCT roundtrip failed at index {}: {} -> {}",
                i,
                spatial[i],
                recovered[i]
            );
        }
    }

    #[test]
    fn test_dc_only_block() {
        // Create a constant block (DC-only)
        let value = 42.0f32;
        let spatial = [value; 64];

        let freq = forward_dct_8x8(&spatial);

        // DC coefficient should be value * 8 (forward DCT with /8 normalization)
        assert!((freq[0] - value * 8.0).abs() < 1e-4, "DC coeff should be {}, got {}", value * 8.0, freq[0]);

        // Test optimized inverse: DC / 8 should give back original value
        let recovered = inverse_dct_8x8_optimized(&freq, 0);

        for i in 0..64 {
            let error = (value - recovered[i]).abs();
            assert!(
                error < 1e-4,
                "DC-only block failed at {}: expected {}, got {}",
                i,
                value,
                recovered[i]
            );
        }
    }

    #[test]
    fn test_inverse_dct_8x8() {
        // Test with known coefficients
        let mut coeffs = [0.0f32; 64];
        coeffs[0] = 8.0; // DC coefficient

        let result = inverse_dct_8x8(&coeffs);

        // DC-only should produce constant value: DC / 8 = 8.0 / 8.0 = 1.0
        let expected = 1.0;
        for i in 0..64 {
            assert!(
                (result[i] - expected).abs() < 1e-5,
                "Expected {}, got {}",
                expected,
                result[i]
            );
        }
    }

    #[test]
    fn test_dct_cached() {
        // Verify that the DCT transforms are properly cached
        let dct1 = dct8_forward();
        let dct2 = dct8_forward();
        assert!(Arc::ptr_eq(dct1, dct2));

        let idct1 = dct8_inverse();
        let idct2 = dct8_inverse();
        assert!(Arc::ptr_eq(idct1, idct2));
    }

    #[test]
    fn test_rustdct_normalization() {
        // Check what normalization rustdct uses for 1D
        let dct = dct8_forward();
        let idct = dct8_inverse();

        // Test 1D with constant input
        let mut data: SmallVec<[f32; 8]> = vec![1.0f32; 8].into_iter().collect();
        println!("1D Input: {:?}", &data[..]);

        dct.process_dct2(&mut data);
        println!("1D After DCT-II: {:?}", &data[..]);

        idct.process_dct3(&mut data);
        println!("1D After DCT-III: {:?}", &data[..]);
        println!("1D Round-trip scale factor: {}", data[0]);
        assert!((data[0] - 4.0).abs() < 1e-5, "Expected 4.0, got {}", data[0]);

        // Test 2D with constant input
        let spatial = [1.0f32; 64];
        let freq = forward_dct_8x8(&spatial);
        println!("2D DC coefficient: {}", freq[0]);

        let recovered_no_norm = [1.0f32; 64];
        // Manually apply 2D DCT without normalization
        let mut temp = [0.0f32; 64];
        for row in 0..8 {
            let mut row_buf: SmallVec<[f32; 8]> = recovered_no_norm[row*8..(row+1)*8].iter().copied().collect();
            dct.process_dct2(&mut row_buf);
            for col in 0..8 {
                temp[col * 8 + row] = row_buf[col];
            }
        }
        let mut result = [0.0f32; 64];
        for col in 0..8 {
            let mut col_buf: SmallVec<[f32; 8]> = temp[col*8..(col+1)*8].iter().copied().collect();
            dct.process_dct2(&mut col_buf);
            for row in 0..8 {
                result[row * 8 + col] = col_buf[row];
            }
        }
        println!("2D DCT-II scale (DC): {}", result[0]);

        // Now test inverse
        let mut coeffs = [0.0f32; 64];
        coeffs[0] = 8.0;
        let mut temp2 = [0.0f32; 64];
        for row in 0..8 {
            let mut row_buf: SmallVec<[f32; 8]> = coeffs[row*8..(row+1)*8].iter().copied().collect();
            idct.process_dct3(&mut row_buf);
            for col in 0..8 {
                temp2[col * 8 + row] = row_buf[col];
            }
        }
        let mut result2 = [0.0f32; 64];
        for col in 0..8 {
            let mut col_buf: SmallVec<[f32; 8]> = temp2[col*8..(col+1)*8].iter().copied().collect();
            idct.process_dct3(&mut col_buf);
            for row in 0..8 {
                result2[row * 8 + col] = col_buf[row];
            }
        }
        println!("2D DCT-III on [8,0,...]: {} (should be 1.0 after /8)", result2[0]);
        println!("So 2D IDCT normalization should be: {}", result2[0] / 8.0);
    }
}
