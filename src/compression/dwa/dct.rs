//! DCT (Discrete Cosine Transform) for DWAA/DWAB compression.
//!
//! Direct port of OpenEXR's dctInverse8x8 implementation from internal_dwa_simd.h.
//! This uses a custom butterfly algorithm with specific cosine-based scaling,
//! not a standard DCT-III.

const PI_APPROX: f32 = 3.14159;
const COS_A: f32 = f32::from_bits(0x3eb504fb); // 0.3535536230
const COS_B: f32 = f32::from_bits(0x3efb14bf); // 0.4903926551
const COS_C: f32 = f32::from_bits(0x3eec8361); // 0.4619398415
const COS_D: f32 = f32::from_bits(0x3ed4db36); // 0.4157349467
const COS_E: f32 = f32::from_bits(0x3e8e39e5); // 0.2777854502
const COS_F: f32 = f32::from_bits(0x3e43ef33); // 0.1913421601
const COS_G: f32 = f32::from_bits(0x3dc7c60b); // 0.0975457057
const DC_SCALE: f32 = f32::from_bits(0x3eb504fa); // 0.3535536

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
    let mut data = *coeffs;

    // First pass - row wise
    for row in 0..(8 - zeroed_rows) {
        let row_start = row * 8;
        let row_ptr = &mut data[row_start..row_start + 8];

        let alpha = [
            COS_C * row_ptr[2],
            COS_F * row_ptr[2],
            COS_C * row_ptr[6],
            COS_F * row_ptr[6],
        ];

        let beta = [
            COS_B * row_ptr[1] + COS_D * row_ptr[3] + COS_E * row_ptr[5] + COS_G * row_ptr[7],
            COS_D * row_ptr[1] - COS_G * row_ptr[3] - COS_B * row_ptr[5] - COS_E * row_ptr[7],
            COS_E * row_ptr[1] - COS_B * row_ptr[3] + COS_G * row_ptr[5] + COS_D * row_ptr[7],
            COS_G * row_ptr[1] - COS_E * row_ptr[3] + COS_D * row_ptr[5] - COS_B * row_ptr[7],
        ];

        let theta = [
            COS_A * (row_ptr[0] + row_ptr[4]),
            alpha[0] + alpha[3],
            alpha[1] - alpha[2],
            COS_A * (row_ptr[0] - row_ptr[4]),
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
            COS_C * data[16 + column],
            COS_F * data[16 + column],
            COS_C * data[48 + column],
            COS_F * data[48 + column],
        ];

        let beta = [
            COS_B * data[8 + column] + COS_D * data[24 + column] + COS_E * data[40 + column] + COS_G * data[56 + column],
            COS_D * data[8 + column] - COS_G * data[24 + column] - COS_B * data[40 + column] - COS_E * data[56 + column],
            COS_E * data[8 + column] - COS_B * data[24 + column] + COS_G * data[40 + column] + COS_D * data[56 + column],
            COS_G * data[8 + column] - COS_E * data[24 + column] + COS_D * data[40 + column] - COS_B * data[56 + column],
        ];

        let theta = [
            COS_A * (data[column] + data[32 + column]),
            alpha[0] + alpha[3],
            alpha[1] - alpha[2],
            COS_A * (data[column] - data[32 + column]),
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
        let dc_value = coeffs[0] * DC_SCALE * DC_SCALE;
        return [dc_value; 64];
    }

    // Determine how many rows we can skip based on the last non-zero coefficient
    let zeroed_rows = if last_non_zero < 2 {
        7
    } else if last_non_zero < 3 {
        6
    } else if last_non_zero < 9 {
        5
    } else if last_non_zero < 10 {
        4
    } else if last_non_zero < 20 {
        3
    } else if last_non_zero < 21 {
        2
    } else if last_non_zero < 35 {
        1
    } else {
        0
    };

    inverse_dct_8x8_impl(coeffs, zeroed_rows)
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
