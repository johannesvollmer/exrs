//! DCT (Discrete Cosine Transform) for DWAA/DWAB compression.
//!
//! Direct port of OpenEXR's dctInverse8x8 implementation from internal_dwa_simd.h.
//! This uses a custom butterfly algorithm with specific cosine-based scaling,
//! not a standard DCT-III.

use std::f32::consts::PI;

/// Perform inverse 8x8 DCT on a block of coefficients.
///
/// Direct port of OpenEXR's dctInverse8x8_scalar from internal_dwa_simd.h.
/// The DCT coefficients are in "normal" order (not zigzag).
///
/// # Arguments
/// * `coeffs` - DCT coefficients in row-major order
///
/// # Returns
/// Spatial domain values (8x8 block)
pub fn inverse_dct_8x8(coeffs: &[f32; 64]) -> [f32; 64] {
    inverse_dct_8x8_impl(coeffs, 0)
}

/// OpenEXR's dctInverse8x8 implementation with optional row skipping.
///
/// # Arguments
/// * `coeffs` - DCT coefficients in row-major order
/// * `zeroed_rows` - Number of bottom rows that are all zero (optimization)
///
/// # Returns
/// Spatial domain values (8x8 block)
fn inverse_dct_8x8_impl(coeffs: &[f32; 64], zeroed_rows: usize) -> [f32; 64] {
    // Constants from OpenEXR's dctInverse8x8_scalar
    let a = 0.5 * (PI / 4.0).cos();
    let b = 0.5 * (PI / 16.0).cos();
    let c = 0.5 * (PI / 8.0).cos();
    let d = 0.5 * (3.0 * PI / 16.0).cos();
    let e = 0.5 * (5.0 * PI / 16.0).cos();
    let f = 0.5 * (3.0 * PI / 8.0).cos();
    let g = 0.5 * (7.0 * PI / 16.0).cos();

    let mut data = *coeffs;

    // First pass - row wise
    for row in 0..(8 - zeroed_rows) {
        let row_start = row * 8;
        let row_ptr = &mut data[row_start..row_start + 8];

        let alpha = [
            c * row_ptr[2],
            f * row_ptr[2],
            c * row_ptr[6],
            f * row_ptr[6],
        ];

        let beta = [
            b * row_ptr[1] + d * row_ptr[3] + e * row_ptr[5] + g * row_ptr[7],
            d * row_ptr[1] - g * row_ptr[3] - b * row_ptr[5] - e * row_ptr[7],
            e * row_ptr[1] - b * row_ptr[3] + g * row_ptr[5] + d * row_ptr[7],
            g * row_ptr[1] - e * row_ptr[3] + d * row_ptr[5] - b * row_ptr[7],
        ];

        let theta = [
            a * (row_ptr[0] + row_ptr[4]),
            alpha[0] + alpha[3],
            alpha[1] - alpha[2],
            a * (row_ptr[0] - row_ptr[4]),
        ];

        let gamma = [
            theta[0] + theta[1],
            theta[3] + theta[2],
            theta[3] - theta[2],
            theta[0] - theta[1],
        ];

        row_ptr[0] = gamma[0] + beta[0];
        row_ptr[1] = gamma[1] + beta[1];
        row_ptr[2] = gamma[2] + beta[2];
        row_ptr[3] = gamma[3] + beta[3];
        row_ptr[4] = gamma[3] - beta[3];
        row_ptr[5] = gamma[2] - beta[2];
        row_ptr[6] = gamma[1] - beta[1];
        row_ptr[7] = gamma[0] - beta[0];
    }

    // Second pass - column wise
    for column in 0..8 {
        let alpha = [
            c * data[16 + column],
            f * data[16 + column],
            c * data[48 + column],
            f * data[48 + column],
        ];

        let beta = [
            b * data[8 + column] + d * data[24 + column] + e * data[40 + column] + g * data[56 + column],
            d * data[8 + column] - g * data[24 + column] - b * data[40 + column] - e * data[56 + column],
            e * data[8 + column] - b * data[24 + column] + g * data[40 + column] + d * data[56 + column],
            g * data[8 + column] - e * data[24 + column] + d * data[40 + column] - b * data[56 + column],
        ];

        let theta = [
            a * (data[column] + data[32 + column]),
            alpha[0] + alpha[3],
            alpha[1] - alpha[2],
            a * (data[column] - data[32 + column]),
        ];

        let gamma = [
            theta[0] + theta[1],
            theta[3] + theta[2],
            theta[3] - theta[2],
            theta[0] - theta[1],
        ];

        data[column] = gamma[0] + beta[0];
        data[8 + column] = gamma[1] + beta[1];
        data[16 + column] = gamma[2] + beta[2];
        data[24 + column] = gamma[3] + beta[3];
        data[32 + column] = gamma[3] - beta[3];
        data[40 + column] = gamma[2] - beta[2];
        data[48 + column] = gamma[1] - beta[1];
        data[56 + column] = gamma[0] - beta[0];
    }

    data
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

/// Optimized inverse DCT that can skip work based on last non-zero coefficient.
///
/// Direct port of OpenEXR's optimization strategy.
///
/// # Arguments
/// * `coeffs` - DCT coefficients in row-major order
/// * `last_non_zero` - Index of the last non-zero coefficient in zigzag order
///
/// # Returns
/// Spatial domain values (8x8 block)
pub fn inverse_dct_8x8_optimized(coeffs: &[f32; 64], last_non_zero: usize) -> [f32; 64] {
    if last_non_zero == 0 {
        // DC-only block - OpenEXR's dctInverse8x8DcOnly
        // data[0] * 3.535536e-01f * 3.535536e-01f
        // 3.535536e-01 = 1/sqrt(8) ≈ 0.35355339
        // 0.35355339² = 0.125 = 1/8
        let dc_value = coeffs[0] * 0.35355339 * 0.35355339;
        return [dc_value; 64];
    }

    // Determine how many rows we can skip based on the last non-zero coefficient
    let last_pos = ZIGZAG[last_non_zero];
    let last_row = last_pos / 8;
    let rows_to_process = (last_row + 1).min(8);

    // If we can skip 3+ rows, use the optimized path
    let zeroed_rows = 8 - rows_to_process;
    if zeroed_rows >= 3 {
        inverse_dct_8x8_impl(coeffs, zeroed_rows)
    } else {
        inverse_dct_8x8(coeffs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dc_only_block() {
        // Test OpenEXR's DC-only optimization
        // For a DC-only block with DC = 336.0, all spatial values should be DC * 0.35355339²
        let mut coeffs = [0.0f32; 64];
        coeffs[0] = 336.0;

        let result = inverse_dct_8x8_optimized(&coeffs, 0);

        // 336.0 * 0.35355339² = 336.0 * 0.125 = 42.0
        let expected = 42.0;
        for i in 0..64 {
            let error = (result[i] - expected).abs();
            assert!(
                error < 1e-4,
                "DC-only block failed at {}: expected {}, got {}",
                i,
                expected,
                result[i]
            );
        }
    }

    #[test]
    fn test_inverse_dct_8x8_basic() {
        // Test with a DC-only coefficient using the full IDCT path
        let mut coeffs = [0.0f32; 64];
        coeffs[0] = 336.0;

        let result = inverse_dct_8x8(&coeffs);

        // OpenEXR's IDCT with DC=336 should give ~42 for all spatial values
        // The exact value depends on the butterfly algorithm's normalization
        let expected = 42.0;
        for i in 0..64 {
            let error = (result[i] - expected).abs();
            assert!(
                error < 0.5,
                "Expected ~{}, got {} at index {}",
                expected,
                result[i],
                i
            );
        }
    }
}
