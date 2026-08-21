// SSE2-only write-row for hosts without AVX2/F16C (strip-tile write is scalar
// below AVX2). No F16C: software f32->f16 bits. `to_linear` reuses
// `avx2::linearize_lanes`.

use std::convert::TryInto;

use miraculix::x86::ops::sse::{sse::Sse, sse2::Sse2};

use super::avx2::linearize_lanes;

/// `sign | mantissa_result`, where `mantissa_result` is either the
/// finite-path result or the inf/NaN fixup
#[inline(always)]
fn blend_i32x4(sse2: Sse2, mask: [i32; 4], if_true: [i32; 4], if_false: [i32; 4]) -> [i32; 4] {
    sse2.or_i32x4(sse2.and_i32x4(mask, if_true), sse2.andnot_i32x4(mask, if_false))
}

/// Narrows 4 `u32` lanes (each holding a value in `0..=0xffff`) to the low 4
/// `u16` lanes of an 8-lane result (high 4 lanes are a duplicate, from
/// `pack_i32x4_to_i16x8`'s 2-input shape). SSE2 has no unsigned 32->16
/// saturating pack (that's SSE4.1's `packus_epi32`); this uses the standard
/// bias-then-xor trick: subtracting 0x8000 brings `0..=0xffff` into signed
/// 16-bit range so the signed pack doesn't saturate, and adding/subtracting
/// 0x8000 mod 65536 is exactly an XOR of bit 15.
#[inline(always)]
fn narrow_u32x4_to_u16x4_biased(sse2: Sse2, v: [i32; 4]) -> [u16; 8] {
    let biased = sse2.sub_i32x4(v, [0x8000; 4]);
    let packed = sse2.pack_i32x4_to_i16x8(biased, biased);
    let flipped = sse2.xor_i16x8(packed, [-0x8000i16; 8]);
    flipped.map(|x| x as u16)
}

/// Lane-wise, branchless, round-to-nearest-even f32->f16 bit conversion for
/// 4 lanes (an 8-wide row needs two calls, one per half, SSE2's `__m128`
/// is 4-wide, unlike AVX2's 8-wide `__m256`). Returns 8 lanes with lanes 0-3
/// the real result and 4-7 a duplicate: see `narrow_u32x4_to_u16x4_biased`.
#[inline(always)]
fn f32_bits_to_f16_bits_x4(sse: Sse, sse2: Sse2, x_in: [i32; 4]) -> [u16; 8] {
    const F16_OVERFLOW_EXP_THRESHOLD: i32 = (127 + 16) << 23;
    const F32_INFINITY_BITS: i32 = 0xFFi32 << 23;
    const F16_SUBNORMAL_THRESHOLD: i32 = 113i32 << 23;
    const DENORM_MAGIC_BITS: i32 = ((127 - 15) + (23 - 10) + 1) << 23;
    const BIAS_ADJUST: i32 = ((15i32 - 127) << 23).wrapping_add(0xfff);

    let sign_mask = [i32::MIN; 4];
    let x_sgn = sse2.and_i32x4(x_in, sign_mask);
    let x = sse2.xor_i32x4(x_in, x_sgn); // abs-value bits; top bit now 0

    let is_overflow = sse2.cmpgt_i32x4(x, [F16_OVERFLOW_EXP_THRESHOLD - 1; 4]);
    let is_nan = sse2.cmpgt_i32x4(x, [F32_INFINITY_BITS; 4]);
    let inf_or_nan_result = blend_i32x4(sse2, is_nan, [0x7e00; 4], [0x7c00; 4]);

    // Denormal/zero path: add a magic float so IEEE round-to-nearest-even
    // addition does the mantissa rounding for us, then subtract the magic
    // bits back off.
    let denorm_magic_bits = [DENORM_MAGIC_BITS; 4];
    let x_f: [f32; 4] = x.map(|bits| f32::from_bits(bits as u32));
    let magic_f: [f32; 4] = denorm_magic_bits.map(|bits| f32::from_bits(bits as u32));
    let denorm_sum_bits: [i32; 4] = sse.add_f32x4(x_f, magic_f).map(|f| f.to_bits() as i32);
    let denorm_result = sse2.sub_i32x4(denorm_sum_bits, denorm_magic_bits);

    // Normal path: rebias the exponent and round-to-nearest-even via the
    // "add 0xfff plus the truncated bit" trick.
    let mant_odd = sse2.and_i32x4(sse2.shr_i32x4::<13>(x), [1; 4]);
    let x_adjusted = sse2.add_i32x4(sse2.add_i32x4(x, [BIAS_ADJUST; 4]), mant_odd);
    let normal_result = sse2.shr_i32x4::<13>(x_adjusted);

    let is_denorm = sse2.cmplt_i32x4(x, [F16_SUBNORMAL_THRESHOLD; 4]);
    let finite_result = blend_i32x4(sse2, is_denorm, denorm_result, normal_result);
    let mantissa_result = blend_i32x4(sse2, is_overflow, inf_or_nan_result, finite_result);

    let signed = sse2.or_i32x4(mantissa_result, sse2.shr_i32x4::<16>(x_sgn));
    narrow_u32x4_to_u16x4_biased(sse2, signed)
}

/// Lane-wise f16->f32 bit widening for 4 lanes, each already zero-extended
/// into the low 16 bits of a 32-bit lane (see call sites).
#[inline(always)]
fn f16_bits_to_f32_bits_x4(sse: Sse, sse2: Sse2, h_in: [i32; 4]) -> [i32; 4] {
    const MAGIC_EXP_BITS: i32 = (254 - 15) << 23;
    const INFNAN_THRESHOLD_BITS: i32 = (127 + 16) << 23;

    let sign = sse2.shl_i32x4::<16>(sse2.and_i32x4(h_in, [0x8000; 4]));
    let value_bits = sse2.shl_i32x4::<13>(sse2.and_i32x4(h_in, [0x7fff; 4]));

    let value_f: [f32; 4] = value_bits.map(|bits| f32::from_bits(bits as u32));
    let magic_f: [f32; 4] = [MAGIC_EXP_BITS; 4].map(|bits| f32::from_bits(bits as u32));
    let scaled_bits: [i32; 4] = sse.mul_f32x4(value_f, magic_f).map(|f| f.to_bits() as i32);

    let is_infnan = sse2.cmpgt_i32x4(scaled_bits, [INFNAN_THRESHOLD_BITS - 1; 4]);
    let infnan_exp_fixup = sse2.and_i32x4(is_infnan, [0xFFi32 << 23; 4]);

    sse2.or_i32x4(sse2.or_i32x4(scaled_bits, infnan_exp_fixup), sign)
}

/// SSE2 analog of `x86::avx2::write_row_f16`. `row` must be exactly 8 `f32`
/// DCT-output samples; `out_row` exactly 16 bytes (8 half-float samples).
#[inline(always)]
pub(super) fn write_row_f16(
    sse: Sse,
    sse2: Sse2,
    row: &[f32],
    to_linear: Option<&[u16; 65536]>,
    out_row: &mut [u8],
) -> bool {
    let Ok(&row): Result<&[f32; 8], _> = row.try_into() else {
        return false;
    };
    if out_row.len() != 16 {
        return false;
    }

    let lo_bits: [i32; 4] = std::array::from_fn(|i| row[i].to_bits() as i32);
    let hi_bits: [i32; 4] = std::array::from_fn(|i| row[4 + i].to_bits() as i32);

    let lo_half = f32_bits_to_f16_bits_x4(sse, sse2, lo_bits);
    let hi_half = f32_bits_to_f16_bits_x4(sse, sse2, hi_bits);
    // `punpcklqdq`: the low 64 bits (real lanes 0-3) of each half,
    // concatenated: a plain array read, no shuffle instruction needed.
    let nonlinear: [u16; 8] = std::array::from_fn(|i| {
        if i < 4 {
            lo_half[i]
        } else {
            hi_half[i - 4]
        }
    });

    let linear = match to_linear {
        Some(table) => linearize_lanes(nonlinear, table),
        None => nonlinear,
    };

    for (chunk, &half) in out_row.chunks_exact_mut(2).zip(linear.iter()) {
        chunk.copy_from_slice(&half.to_le_bytes());
    }
    true
}

/// Same as `write_row_f16`, but widens the linearized halves back to `f32`
/// for F32-sample-type channels, matching `write_row_f16`'s AVX2.
#[inline(always)]
pub(super) fn write_row_f32(
    sse: Sse,
    sse2: Sse2,
    row: &[f32],
    to_linear: Option<&[u16; 65536]>,
    out_row: &mut [u8],
) -> bool {
    let Ok(&row): Result<&[f32; 8], _> = row.try_into() else {
        return false;
    };
    if out_row.len() != 32 {
        return false;
    }

    let lo_bits: [i32; 4] = std::array::from_fn(|i| row[i].to_bits() as i32);
    let hi_bits: [i32; 4] = std::array::from_fn(|i| row[4 + i].to_bits() as i32);

    let lo_half = f32_bits_to_f16_bits_x4(sse, sse2, lo_bits);
    let hi_half = f32_bits_to_f16_bits_x4(sse, sse2, hi_bits);
    let nonlinear: [u16; 8] = std::array::from_fn(|i| {
        if i < 4 {
            lo_half[i]
        } else {
            hi_half[i - 4]
        }
    });

    let linear = match to_linear {
        Some(table) => linearize_lanes(nonlinear, table),
        None => nonlinear,
    };

    // `punpcklwd`/`punpckhwd` against zero: zero-extend each u16 lane to a
    // u32 lane: a plain widening cast, no shuffle instruction needed.
    let linear_lo32: [i32; 4] = std::array::from_fn(|i| linear[i] as i32);
    let linear_hi32: [i32; 4] = std::array::from_fn(|i| linear[4 + i] as i32);

    let widened_lo = f16_bits_to_f32_bits_x4(sse, sse2, linear_lo32);
    let widened_hi = f16_bits_to_f32_bits_x4(sse, sse2, linear_hi32);

    for (chunk, &bits) in out_row[..16].chunks_exact_mut(4).zip(widened_lo.iter()) {
        chunk.copy_from_slice(&(bits as u32).to_le_bytes());
    }
    for (chunk, &bits) in out_row[16..].chunks_exact_mut(4).zip(widened_hi.iter()) {
        chunk.copy_from_slice(&(bits as u32).to_le_bytes());
    }
    true
}

#[cfg(test)]
mod test {
    use half::f16;
    use miraculix::x86::{
        detect_features,
        ops::sse::{sse::Sse, sse2::Sse2},
    };

    use super::{super::super::transfer_curve::to_linear_table, write_row_f16, write_row_f32};

    fn expect_sse2() -> (Sse, Sse2) {
        let features = detect_features();
        (
            Sse::from_features(features).expect("SSE SIMD mode requested, but unavailable"),
            Sse2::from_features(features).expect("SSE2 SIMD mode requested, but unavailable"),
        )
    }

    fn scalar_linear_bits(value: f32, to_linear: Option<&[u16; 65536]>) -> u16 {
        let nonlinear = f16::from_f32(value);
        match to_linear {
            Some(table) => table[nonlinear.to_bits() as usize],
            None => nonlinear.to_bits(),
        }
    }

    /// Same sweep shape as `avx2::test::sweep_rows`, full f32 bit space plus
    /// a handful of named specials.
    fn sweep_rows() -> impl Iterator<Item = [f32; 8]> {
        let special = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            65504.0,
            65520.0,
            -65504.0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::from_bits(0x7fc00001),
            f32::from_bits(0x00000001),
            f32::from_bits(0x0000ffff),
            6.104_f32.exp2() * f32::EPSILON,
        ];
        let special_rows: Vec<[f32; 8]> = special
            .chunks(8)
            .map(|chunk| {
                let mut row = [0.0f32; 8];
                row[..chunk.len()].copy_from_slice(chunk);
                row
            })
            .collect();

        let swept_rows = (0u32..=0xFFFF).map(|base| {
            std::array::from_fn(|lane| f32::from_bits(base.wrapping_add(lane as u32 * 0x1000_0001)))
        });

        special_rows.into_iter().chain(swept_rows)
    }

    fn assert_bits_match(a: u16, b: u16, context: &str) {
        let a_nan = f16::from_bits(a).is_nan();
        let b_nan = f16::from_bits(b).is_nan();
        if a_nan || b_nan {
            assert_eq!(
                a_nan, b_nan,
                "{}: one side is NaN, the other isn't (a=0x{:04x}, b=0x{:04x})",
                context, a, b
            );
        } else {
            assert_eq!(a, b, "{}: bit mismatch (a=0x{:04x}, b=0x{:04x})", context, a, b);
        }
    }

    #[test]
    fn write_row_f16_matches_scalar() {
        let (sse, sse2) = expect_sse2();
        for to_linear in [None, Some(to_linear_table())] {
            for row in sweep_rows() {
                let mut simd = [0u8; 16];
                assert!(write_row_f16(sse, sse2, &row, to_linear, &mut simd));

                for (lane, &value) in row.iter().enumerate() {
                    let expected = scalar_linear_bits(value, to_linear);
                    let actual = u16::from_le_bytes([simd[lane * 2], simd[lane * 2 + 1]]);
                    assert_bits_match(
                        actual,
                        expected,
                        &format!(
                            "f16 lane {lane}, value {value:e}, to_linear={}",
                            to_linear.is_some()
                        ),
                    );
                }
            }
        }
    }

    #[test]
    fn write_row_f32_matches_scalar() {
        let (sse, sse2) = expect_sse2();
        for to_linear in [None, Some(to_linear_table())] {
            for row in sweep_rows() {
                let mut simd = [0u8; 32];
                assert!(write_row_f32(sse, sse2, &row, to_linear, &mut simd));

                for (lane, &value) in row.iter().enumerate() {
                    let expected = f16::from_bits(scalar_linear_bits(value, to_linear)).to_f32();
                    let actual = f32::from_le_bytes([
                        simd[lane * 4],
                        simd[lane * 4 + 1],
                        simd[lane * 4 + 2],
                        simd[lane * 4 + 3],
                    ]);
                    if expected.is_nan() {
                        assert!(
                            actual.is_nan(),
                            "f32 lane {}, value {:e}: expected NaN, got {:e}",
                            lane,
                            value,
                            actual,
                        );
                    } else {
                        assert_eq!(
                            actual.to_bits(),
                            expected.to_bits(),
                            "f32 lane {}, value {:e}, to_linear={}",
                            lane,
                            value,
                            to_linear.is_some()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn write_row_falls_back_for_short_rows() {
        let (sse, sse2) = expect_sse2();
        let row = [0.0f32; 7];
        let mut out16 = [0u8; 14];
        let mut out32 = [0u8; 28];
        assert!(!write_row_f16(sse, sse2, &row, None, &mut out16));
        assert!(!write_row_f32(sse, sse2, &row, None, &mut out32));
    }
}
