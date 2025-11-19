//! DCT (Discrete Cosine Transform) for DWAA/DWAB compression.
//!
//! Direct port of OpenEXR's dctInverse8x8 implementation from
//! internal_dwa_simd.h. This uses a custom butterfly algorithm with specific
//! cosine-based scaling, not a standard DCT-III.

use half::f16;

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
            COS_B * data[8 + column]
                + COS_D * data[24 + column]
                + COS_E * data[40 + column]
                + COS_G * data[56 + column],
            COS_D * data[8 + column]
                - COS_G * data[24 + column]
                - COS_B * data[40 + column]
                - COS_E * data[56 + column],
            COS_E * data[8 + column] - COS_B * data[24 + column]
                + COS_G * data[40 + column]
                + COS_D * data[56 + column],
            COS_G * data[8 + column] - COS_E * data[24 + column] + COS_D * data[40 + column]
                - COS_B * data[56 + column],
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

/// Convert zigzag-ordered half bits into row-major f32 coefficients.
pub fn from_half_zigzag(src: &[u16; 64], dst: &mut [f32; 64]) {
    let to_f32 = |bits: u16| f16::from_bits(bits).to_f32();
    dst[0] = to_f32(src[0]);
    dst[1] = to_f32(src[1]);
    dst[2] = to_f32(src[5]);
    dst[3] = to_f32(src[6]);
    dst[4] = to_f32(src[14]);
    dst[5] = to_f32(src[15]);
    dst[6] = to_f32(src[27]);
    dst[7] = to_f32(src[28]);
    dst[8] = to_f32(src[2]);
    dst[9] = to_f32(src[4]);
    dst[10] = to_f32(src[7]);
    dst[11] = to_f32(src[13]);
    dst[12] = to_f32(src[16]);
    dst[13] = to_f32(src[26]);
    dst[14] = to_f32(src[29]);
    dst[15] = to_f32(src[42]);
    dst[16] = to_f32(src[3]);
    dst[17] = to_f32(src[8]);
    dst[18] = to_f32(src[12]);
    dst[19] = to_f32(src[17]);
    dst[20] = to_f32(src[25]);
    dst[21] = to_f32(src[30]);
    dst[22] = to_f32(src[41]);
    dst[23] = to_f32(src[43]);
    dst[24] = to_f32(src[9]);
    dst[25] = to_f32(src[11]);
    dst[26] = to_f32(src[18]);
    dst[27] = to_f32(src[24]);
    dst[28] = to_f32(src[31]);
    dst[29] = to_f32(src[40]);
    dst[30] = to_f32(src[44]);
    dst[31] = to_f32(src[53]);
    dst[32] = to_f32(src[10]);
    dst[33] = to_f32(src[19]);
    dst[34] = to_f32(src[23]);
    dst[35] = to_f32(src[32]);
    dst[36] = to_f32(src[39]);
    dst[37] = to_f32(src[45]);
    dst[38] = to_f32(src[52]);
    dst[39] = to_f32(src[54]);
    dst[40] = to_f32(src[20]);
    dst[41] = to_f32(src[22]);
    dst[42] = to_f32(src[33]);
    dst[43] = to_f32(src[38]);
    dst[44] = to_f32(src[46]);
    dst[45] = to_f32(src[51]);
    dst[46] = to_f32(src[55]);
    dst[47] = to_f32(src[60]);
    dst[48] = to_f32(src[21]);
    dst[49] = to_f32(src[34]);
    dst[50] = to_f32(src[37]);
    dst[51] = to_f32(src[47]);
    dst[52] = to_f32(src[50]);
    dst[53] = to_f32(src[56]);
    dst[54] = to_f32(src[59]);
    dst[55] = to_f32(src[61]);
    dst[56] = to_f32(src[35]);
    dst[57] = to_f32(src[36]);
    dst[58] = to_f32(src[48]);
    dst[59] = to_f32(src[49]);
    dst[60] = to_f32(src[57]);
    dst[61] = to_f32(src[58]);
    dst[62] = to_f32(src[62]);
    dst[63] = to_f32(src[63]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dc_only_block() {
        // Test OpenEXR's DC-only optimization
        // For a DC-only block with DC = 336.0, all spatial values should be DC *
        // 0.35355339²
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
