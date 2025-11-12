//! DCT (Discrete Cosine Transform) for DWAA/DWAB compression.
//!
//! Uses 8x8 DCT-II for frequency decomposition.

use rustdct::{DctPlanner, Dct2};
use std::sync::{Arc, OnceLock};

/// Get a cached DCT-II transform for 8x8 blocks
fn get_dct8() -> &'static Arc<dyn Dct2<f32>> {
    static DCT8: OnceLock<Arc<dyn Dct2<f32>>> = OnceLock::new();
    DCT8.get_or_init(|| {
        let mut planner = DctPlanner::new();
        planner.plan_dct2(8)
    })
}

/// Perform inverse 8x8 DCT on a block of coefficients
///
/// The DCT coefficients are in "normal" order (not zigzag).
/// Uses a two-pass algorithm: first on rows, then on columns.
///
/// # Arguments
/// * `coeffs` - DCT coefficients in row-major order
///
/// # Returns
/// Spatial domain values (8x8 block)
pub fn inverse_dct_8x8(coeffs: &[f32; 64]) -> [f32; 64] {
    let dct = get_dct8();

    let mut result = [0.0f32; 64];
    let mut temp = [0.0f32; 64];

    // First pass: inverse DCT on rows
    for row in 0..8 {
        let row_start = row * 8;
        let row_coeffs = &coeffs[row_start..row_start + 8];
        let mut row_buffer: Vec<f32> = row_coeffs.to_vec();

        dct.process_dct2(&mut row_buffer);

        // Copy result to temp
        for col in 0..8 {
            temp[col * 8 + row] = row_buffer[col];
        }
    }

    // Second pass: inverse DCT on columns (which are now rows in temp due to transpose)
    for col in 0..8 {
        let col_start = col * 8;
        let col_coeffs = &temp[col_start..col_start + 8];
        let mut col_buffer: Vec<f32> = col_coeffs.to_vec();

        dct.process_dct2(&mut col_buffer);

        // Copy result to final output
        for row in 0..8 {
            result[row * 8 + col] = col_buffer[row];
        }
    }

    // Normalize by 1/(2*8) = 1/16 for DCT-II
    for value in &mut result {
        *value /= 16.0;
    }

    result
}

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
    // For now, use the standard implementation
    // TODO: Add optimizations for early termination based on last_non_zero
    // This would involve:
    // 1. Determining which rows/columns can be skipped
    // 2. Using a faster path for DC-only blocks (last_non_zero == 0)
    // 3. Implementing row-skipping logic similar to the OpenEXR reference

    if last_non_zero == 0 {
        // DC-only block - all values are the same
        let dc_value = coeffs[0] / 64.0; // Divide by 64 for proper normalization
        [dc_value; 64]
    } else {
        inverse_dct_8x8(coeffs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Forward DCT for testing (not used in actual decompression)
    fn forward_dct_8x8(spatial: &[f32; 64]) -> [f32; 64] {
        let dct = get_dct8();

        let mut temp = [0.0f32; 64];
        let mut result = [0.0f32; 64];

        // First pass: DCT on rows
        for row in 0..8 {
            let row_start = row * 8;
            let row_data = &spatial[row_start..row_start + 8];
            let mut row_buffer: Vec<f32> = row_data.to_vec();

            dct.process_dct2(&mut row_buffer);

            for col in 0..8 {
                temp[col * 8 + row] = row_buffer[col];
            }
        }

        // Second pass: DCT on columns
        for col in 0..8 {
            let col_start = col * 8;
            let col_data = &temp[col_start..col_start + 8];
            let mut col_buffer: Vec<f32> = col_data.to_vec();

            dct.process_dct2(&mut col_buffer);

            for row in 0..8 {
                result[row * 8 + col] = col_buffer[row];
            }
        }

        // Normalize
        for value in &mut result {
            *value /= 16.0;
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

        // Only DC coefficient should be non-zero
        assert!((freq[0] - value * 64.0).abs() < 1e-4);

        // Test optimized inverse
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

        // DC-only should produce constant value
        let expected = 0.5; // 8.0 / 16.0
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
        // Verify that the DCT is properly cached
        let dct1 = get_dct8();
        let dct2 = get_dct8();

        // Should be the same Arc instance
        assert!(Arc::ptr_eq(dct1, dct2));
    }
}
