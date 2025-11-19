//! DCT (Discrete Cosine Transform) for DWAA/DWAB compression.
//!
//! Direct port of OpenEXR's dctInverse8x8 implementation from
//! internal_dwa_simd.h. This uses a custom butterfly algorithm with specific
//! cosine-based scaling, not a standard DCT-III.

use super::constants;
use half::f16;

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

/// Forward 8x8 DCT (matches OpenEXR's scalar implementation).
pub fn forward_dct_8x8(data: &mut [f32; 64]) {
    const C1: f32 = 0.980_785_25;
    const C2: f32 = 0.923_879_5;
    const C3: f32 = 0.831_469_6;
    const C4: f32 = 0.707_106_77;
    const C5: f32 = 0.555_570_24;
    const C6: f32 = 0.382_683_43;
    const C7: f32 = 0.195_090_32;

    const HALF: f32 = 0.5;

    let mut row_tmp = [0.0f32; 64];

    for row in 0..8 {
        let src = &data[row * 8..row * 8 + 8];
        let dst = &mut row_tmp[row * 8..row * 8 + 8];

        let a0 = src[0] + src[7];
        let a1 = src[1] + src[2];
        let a2 = src[1] - src[2];
        let a3 = src[3] + src[4];
        let a4 = src[3] - src[4];
        let a5 = src[5] + src[6];
        let a6 = src[5] - src[6];
        let a7 = src[0] - src[7];

        let mut k0 = C4 * (a0 + a3);
        let mut k1 = C4 * (a1 + a5);

        dst[0] = HALF * (k0 + k1);
        dst[4] = HALF * (k0 - k1);

        let rot_x = a2 - a6;
        let rot_y = a0 - a3;

        dst[2] = HALF * (C6 * rot_x + C2 * rot_y);
        dst[6] = HALF * (C6 * rot_y - C2 * rot_x);

        k0 = C4 * (a1 - a5);
        k1 = -C4 * (a2 + a6);

        let rot_x = a7 - k0;
        let rot_y = a4 + k1;

        dst[3] = HALF * (C3 * rot_x - C5 * rot_y);
        dst[5] = HALF * (C5 * rot_x + C3 * rot_y);

        let rot_x = a7 + k0;
        let rot_y = k1 - a4;

        dst[1] = HALF * (C1 * rot_x - C7 * rot_y);
        dst[7] = HALF * (C7 * rot_x + C1 * rot_y);
    }

    for column in 0..8 {
        let idx = column;

        let a0 = row_tmp[idx] + row_tmp[56 + idx];
        let a7 = row_tmp[idx] - row_tmp[56 + idx];
        let a1 = row_tmp[8 + idx] + row_tmp[16 + idx];
        let a2 = row_tmp[8 + idx] - row_tmp[16 + idx];
        let a3 = row_tmp[24 + idx] + row_tmp[32 + idx];
        let a4 = row_tmp[24 + idx] - row_tmp[32 + idx];
        let a5 = row_tmp[40 + idx] + row_tmp[48 + idx];
        let a6 = row_tmp[40 + idx] - row_tmp[48 + idx];

        let mut k0 = C4 * (a0 + a3);
        let mut k1 = C4 * (a1 + a5);

        data[idx] = HALF * (k0 + k1);
        data[32 + idx] = HALF * (k0 - k1);

        let rot_x = a2 - a6;
        let rot_y = a0 - a3;

        data[16 + idx] = HALF * (C6 * rot_x + C2 * rot_y);
        data[48 + idx] = HALF * (C6 * rot_y - C2 * rot_x);

        k0 = C4 * (a1 - a5);
        k1 = -C4 * (a2 + a6);

        let rot_x = a7 - k0;
        let rot_y = a4 + k1;

        data[24 + idx] = HALF * (C3 * rot_x - C5 * rot_y);
        data[40 + idx] = HALF * (C5 * rot_x + C3 * rot_y);

        let rot_x = a7 + k0;
        let rot_y = k1 - a4;

        data[8 + idx] = HALF * (C1 * rot_x - C7 * rot_y);
        data[56 + idx] = HALF * (C7 * rot_x + C1 * rot_y);
    }
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

/// Convert row-major coefficients into zigzag half bits with quantization.
pub fn quantize_to_half_zigzag(
    coeffs: &[f32; 64],
    quant_table: &[f32; 64],
    half_quant_table: &[u16; 64],
    dst: &mut [u16; 64],
) {
    coeffs
        .iter()
        .zip(quant_table.iter())
        .zip(half_quant_table.iter())
        .enumerate()
        .for_each(|(i, ((&coeff, &tol), &half_tol))| {
            let zig_idx = constants::ZIGZAG_ORDER[i];
            dst[zig_idx] = quantize_coeff(coeff, half_tol, tol);
        });
}

fn quantize_coeff(value: f32, half_tol_bits: u16, tolerance: f32) -> u16 {
    let src_bits = float_to_half_bits(value);
    let src_float = value;
    algo_quantize(src_bits, half_tol_bits, tolerance, src_float)
}

fn algo_quantize(src: u16, tol_bits: u16, tolerance: f32, src_float: f32) -> u16 {
    // Port of OpenEXR's algoQuantize. See internal_dwa_encoder.h.
    fn count_bits(v: u32) -> u32 {
        v.count_ones()
    }

    fn leading_zeros(v: u32) -> u32 {
        v.leading_zeros()
    }

    fn float_from_half_bits(bits: u16) -> f32 {
        f16::from_bits(bits).to_f32()
    }

    fn quantize_with_mask(
        abs_src: u32,
        npow2: u32,
        mask: u32,
        err_tol: f32,
        src_float: f32,
        sign: u32,
        prefer_smaller: bool,
    ) -> u16 {
        let mut best = abs_src;
        let mut best_bits = count_bits(best);
        let mut best_delta = err_tol;

        let candidates = if prefer_smaller {
            [
                abs_src & mask,
                (abs_src & mask).saturating_sub(npow2),
                (abs_src & mask).saturating_add(npow2),
                abs_src & !mask,
            ]
        } else {
            [
                abs_src & mask,
                (abs_src & mask) | npow2,
                (abs_src + npow2) & mask,
                (abs_src + (npow2 << 1)) & mask,
            ]
        };

        for &cand in &candidates {
            let bits = count_bits(cand);
            let cand_float = float_from_half_bits((sign | cand) as u16);
            let delta = (cand_float - src_float).abs();
            if delta < err_tol && (bits < best_bits || (bits == best_bits && delta < best_delta)) {
                best = cand;
                best_bits = bits;
                best_delta = delta;
            }
        }

        (sign | best) as u16
    }

    fn quantize_generic(
        abs_src: u32,
        tol_sig: u32,
        err_tol: f32,
        src_float: f32,
        sign: u32,
    ) -> u16 {
        let tsigshift = 32 - leading_zeros(tol_sig);
        let npow2 = 1 << tsigshift;
        let lowermask = npow2 - 1;
        let mask = !lowermask;
        let src_masked_val = abs_src & lowermask;

        if src_masked_val > tol_sig {
            quantize_with_mask(abs_src, npow2, mask, err_tol, src_float, sign, true)
        } else if src_masked_val < tol_sig {
            quantize_with_mask(abs_src, npow2, mask, err_tol, src_float, sign, false)
        } else {
            quantize_with_mask(abs_src, npow2, mask, err_tol, src_float, sign, true)
        }
    }

    let sign = (src as u32) & 0x8000;
    let mut abs_src = (src as u32) & 0x7fff;

    let src_float = src_float.abs();

    let src_exp = src & 0x7c00;
    let tol_exp = tol_bits & 0x7c00;

    if src_exp == 0x7c00 {
        return src;
    }

    if src_float < tolerance {
        return sign as u16;
    }

    let exp_diff = (src_exp as u32).saturating_sub(tol_exp as u32) >> 10;
    let tol_sig = (((tol_bits as u32 & 0x03ff) | (1 << 10)) >> exp_diff).max(1);

    if tol_exp == 0 {
        let mask = !(tol_sig - 1);
        abs_src &= mask;
        return (sign | abs_src) as u16;
    }

    quantize_generic(abs_src, tol_sig, tolerance, src_float, sign)
}

pub(crate) fn float_to_half_bits(value: f32) -> u16 {
    let ui = value.to_bits();
    let sign = ((ui >> 16) & 0x8000) as u16;
    let mut ret = sign;
    let mut abs = ui & 0x7fff_ffff;

    if abs >= 0x3880_0000 {
        if abs >= 0x7f80_0000 {
            ret |= 0x7c00;
            if abs == 0x7f80_0000 {
                return ret;
            }
            let m = (abs & 0x007f_ffff) >> 13;
            return ret | (m as u16) | if m == 0 { 1 } else { 0 };
        }

        if abs > 0x477f_efff {
            return ret | 0x7c00;
        }

        abs -= 0x3800_0000;
        abs = (abs + 0x0000_0fff + ((abs >> 13) & 1)) >> 13;
        return ret | (abs as u16);
    }

    if abs < 0x3300_0001 {
        return ret;
    }

    let e = abs >> 23;
    let shift = 0x7e - e;
    let m = 0x0080_0000 | (abs & 0x007f_ffff);
    let r = (m as u64) << (32 - shift);
    ret |= (m >> shift) as u16;
    if r > 0x8000_0000 || (r == 0x8000_0000 && (ret & 0x1) != 0) {
        ret = ret.wrapping_add(1);
    }
    ret
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
