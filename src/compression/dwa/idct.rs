// Inverse DCT for DWA, ported from OpenEXRCore's scalar dctInverse8x8_scalar
// and vectorized 8-wide with the `wide` crate: each pass runs all 8 rows (or
// columns) of a block in parallel, one SIMD lane per row/column. `wide::f32x8`
// picks AVX, SSE2-as-two-halves, or a portable fallback via
// `#[cfg(target_feature = ...)]` at compile time, not runtime dispatch.
//
// Matches OpenEXR's scalar reference, not its SIMD-dispatched decoder (see
// `avx_identical` below); see tests/across_compression.rs and tests/dwa_csc.rs.
use wide::f32x8;

struct Coefficients {
    a: f32x8,
    b: f32x8,
    c: f32x8,
    d: f32x8,
    e: f32x8,
    f: f32x8,
    g: f32x8,
}

// One 1D inverse DCT pass, applied to 8 lines (rows or columns) at once:
// `input[k]` holds coefficient `k` of all 8 lines, `output[k]` holds sample
// `k` of all 8 lines.
fn dct_inverse_8x8_pass(coef: &Coefficients, input: [f32x8; 8]) -> [f32x8; 8] {
    let alpha0 = coef.c * input[2];
    let alpha1 = coef.f * input[2];
    let alpha2 = coef.c * input[6];
    let alpha3 = coef.f * input[6];

    let beta0 = coef.b * input[1] + coef.d * input[3] + coef.e * input[5] + coef.g * input[7];
    let beta1 = coef.d * input[1] - coef.g * input[3] - coef.b * input[5] - coef.e * input[7];
    let beta2 = coef.e * input[1] - coef.b * input[3] + coef.g * input[5] + coef.d * input[7];
    let beta3 = coef.g * input[1] - coef.e * input[3] + coef.d * input[5] - coef.b * input[7];

    let theta0 = coef.a * (input[0] + input[4]);
    let theta3 = coef.a * (input[0] - input[4]);
    let theta1 = alpha0 + alpha3;
    let theta2 = alpha1 - alpha2;

    let gamma0 = theta0 + theta1;
    let gamma1 = theta3 + theta2;
    let gamma2 = theta3 - theta2;
    let gamma3 = theta0 - theta1;

    [
        gamma0 + beta0,
        gamma1 + beta1,
        gamma2 + beta2,
        gamma3 + beta3,
        gamma3 - beta3,
        gamma2 - beta2,
        gamma1 - beta1,
        gamma0 - beta0,
    ]
}

/// Inverse DCT on 8x8 block (in-place). `data` is 64 floats in row-major.
pub fn dct_inverse_8x8(data: &mut [f32; 64]) {
    // Truncated PI literal, matching OpenEXR's constant.
    const PI: f32 = 3.14159;

    let coef = Coefficients {
        a: f32x8::splat(0.5 * (PI / 4.0).cos()),
        b: f32x8::splat(0.5 * (PI / 16.0).cos()),
        c: f32x8::splat(0.5 * (PI / 8.0).cos()),
        d: f32x8::splat(0.5 * ((3.0 * PI) / 16.0).cos()),
        e: f32x8::splat(0.5 * ((5.0 * PI) / 16.0).cos()),
        f: f32x8::splat(0.5 * ((3.0 * PI) / 8.0).cos()),
        g: f32x8::splat(0.5 * ((7.0 * PI) / 16.0).cos()),
    };

    // Row pass: lane i = row i. `data` is row-major, so this gathers "column
    // k across all rows" with a strided read.
    let columns: [f32x8; 8] =
        std::array::from_fn(|k| f32x8::new(std::array::from_fn(|row| data[row * 8 + k])));

    let rows_out = dct_inverse_8x8_pass(&coef, columns);
    for (column, result) in rows_out.iter().enumerate() {
        let values = result.to_array();
        for (row, value) in values.iter().enumerate() {
            data[row * 8 + column] = *value;
        }
    }

    // Column pass: lane i = column i. Each row is already contiguous.
    let rows: [f32x8; 8] =
        std::array::from_fn(|row| f32x8::new(std::array::from_fn(|column| data[row * 8 + column])));

    let columns_out = dct_inverse_8x8_pass(&coef, rows);
    for (row, result) in columns_out.iter().enumerate() {
        data[row * 8..row * 8 + 8].copy_from_slice(&result.to_array());
    }
}

/// Optimized path when only DC is non-zero.
pub fn dct_inverse_8x8_dc_only(data: &mut [f32; 64]) {
    let val = data[0] * 0.3535536f32 * 0.3535536f32;
    for v in data.iter_mut() {
        *v = val;
    }
}

// Unused by the decoder (see doc comment on `dct_inverse_8x8_avx_identical`).
// `#[allow(dead_code)]` on the module suppresses the crate-wide `deny(dead_code)`.
#[allow(dead_code)]
mod avx_identical {
    use super::f32x8;

    // OpenEXR's hardcoded AVX basis constants (internal_dwa_simd.h's sAvxCoef).
    const AVX_A: f32 = 3.535536e-1;
    const AVX_B: f32 = 4.903927e-1;
    const AVX_C: f32 = 4.619398e-1;
    const AVX_D: f32 = 4.157349e-1;
    const AVX_E: f32 = 2.777855e-1;
    const AVX_F: f32 = 1.913422e-1;
    const AVX_G: f32 = 9.754573e-2;

    struct AvxCoefficients {
        a: f32x8,
        na: f32x8,
        b: f32x8,
        nb: f32x8,
        c: f32x8,
        nc: f32x8,
        d: f32x8,
        nd: f32x8,
        e: f32x8,
        ne: f32x8,
        f: f32x8,
        nf: f32x8,
        g: f32x8,
        ng: f32x8,
    }

    impl AvxCoefficients {
        fn new() -> Self {
            // Precomputed negated splats (float negation is an exact sign
            // flip, so `x * na == -(x * a)` bit-for-bit).
            Self {
                a: f32x8::splat(AVX_A),
                na: f32x8::splat(-AVX_A),
                b: f32x8::splat(AVX_B),
                nb: f32x8::splat(-AVX_B),
                c: f32x8::splat(AVX_C),
                nc: f32x8::splat(-AVX_C),
                d: f32x8::splat(AVX_D),
                nd: f32x8::splat(-AVX_D),
                e: f32x8::splat(AVX_E),
                ne: f32x8::splat(-AVX_E),
                f: f32x8::splat(AVX_F),
                nf: f32x8::splat(-AVX_F),
                g: f32x8::splat(AVX_G),
                ng: f32x8::splat(-AVX_G),
            }
        }
    }

    // Row pass: OpenEXR's `IDCT_AVX_MMULT_ROWS` + `EO_TO_ROW_HALVES` - a dense
    // matrix-vector product against basis matrices M1 (even input positions
    // 0,2,4,6) and M2 (odd positions 1,3,5,7).
    fn dct_inverse_8x8_row_avx(coef: &AvxCoefficients, input: [f32x8; 8]) -> [f32x8; 8] {
        let (in0, in2, in4, in6) = (input[0], input[2], input[4], input[6]);
        let (in1, in3, in5, in7) = (input[1], input[3], input[5], input[7]);

        // M1 * [in0, in2, in4, in6]
        let even0 = (in4 * coef.a + in6 * coef.f) + (in0 * coef.a + in2 * coef.c);
        let even1 = (in4 * coef.na + in6 * coef.nc) + (in0 * coef.a + in2 * coef.f);
        let even2 = (in4 * coef.na + in6 * coef.c) + (in0 * coef.a + in2 * coef.nf);
        let even3 = (in4 * coef.a + in6 * coef.nf) + (in0 * coef.a + in2 * coef.nc);

        // M2 * [in1, in3, in5, in7]
        let odd0 = (in5 * coef.e + in7 * coef.g) + (in1 * coef.b + in3 * coef.d);
        let odd1 = (in5 * coef.nb + in7 * coef.ne) + (in1 * coef.d + in3 * coef.ng);
        let odd2 = (in5 * coef.g + in7 * coef.d) + (in1 * coef.e + in3 * coef.nb);
        let odd3 = (in5 * coef.d + in7 * coef.nb) + (in1 * coef.g + in3 * coef.ne);

        [
            even0 + odd0,
            even1 + odd1,
            even2 + odd2,
            even3 + odd3,
            even3 - odd3,
            even2 - odd2,
            even1 - odd1,
            even0 - odd0,
        ]
    }

    // Column pass: OpenEXR's AVX column transform (the back half of
    // `dctInverse8x8_avx_0`, after the row pass's transpose).
    fn dct_inverse_8x8_column_avx(coef: &AvxCoefficients, input: [f32x8; 8]) -> [f32x8; 8] {
        let (in0, in1, in2, in3, in4, in5, in6, in7) =
            (input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7]);

        let beta0 = (coef.g * in7 + coef.e * in5) + (coef.d * in3 + coef.b * in1);
        let beta1 = (coef.d * in1 - (coef.b * in5 + coef.g * in3)) - coef.e * in7;
        let beta2 = coef.d * in7 + (coef.g * in5 + (coef.e * in1 - coef.b * in3));
        let beta3 = (coef.d * in5 + coef.g * in1) - (coef.b * in7 + coef.e * in3);

        let theta0 = coef.a * in4 + coef.a * in0;
        let theta3 = coef.a * in0 - coef.a * in4;

        let theta1 = coef.f * in6 + coef.c * in2;
        let gamma0 = theta1 + theta0;
        let gamma3 = theta0 - theta1;

        let theta2 = coef.f * in2 - coef.c * in6;
        let gamma1 = theta3 + theta2;
        let gamma2 = theta3 - theta2;

        [
            gamma0 + beta0,
            gamma1 + beta1,
            gamma2 + beta2,
            gamma3 + beta3,
            gamma3 - beta3,
            gamma2 - beta2,
            gamma1 - beta1,
            gamma0 - beta0,
        ]
    }

    /// Inverse DCT on 8x8 block (in-place), bit-identical to a real OpenEXR
    /// build's SIMD-dispatched decode (avx > sse2 > scalar) rather than its
    /// scalar reference. Safe, portable Rust - no unsafe, no `target_feature`
    /// gating. Not used by the decoder by default; see `dct_inverse_8x8`.
    pub fn dct_inverse_8x8_avx_identical(data: &mut [f32; 64]) {
        let coef = AvxCoefficients::new();

        let columns: [f32x8; 8] =
            std::array::from_fn(|k| f32x8::new(std::array::from_fn(|row| data[row * 8 + k])));

        let rows_out = dct_inverse_8x8_row_avx(&coef, columns);
        for (column, result) in rows_out.iter().enumerate() {
            let values = result.to_array();
            for (row, value) in values.iter().enumerate() {
                data[row * 8 + column] = *value;
            }
        }

        let rows: [f32x8; 8] = std::array::from_fn(|row| {
            f32x8::new(std::array::from_fn(|column| data[row * 8 + column]))
        });

        let columns_out = dct_inverse_8x8_column_avx(&coef, rows);
        for (row, result) in columns_out.iter().enumerate() {
            data[row * 8..row * 8 + 8].copy_from_slice(&result.to_array());
        }
    }
} // mod avx_identical

#[allow(unused_imports)]
pub use avx_identical::dct_inverse_8x8_avx_identical;

// TODO: sse2_identical, same technique as avx_identical above (dctInverse8x8_sse2,
// internal_dwa_simd.h; see this repo's git history at 137ea81 for the original
// unsafe transcription). Dispatch order is avx > sse2 > scalar, so this is
// OpenEXR's non-AVX x86(_64) fallback - the last of the three hardware paths.
//
// Row pass constants (four __m128 lanes each, broadcast-multiplied against
// one scalar row value at a time, then summed strictly left-to-right, NOT
// tree-paired like avx_identical's row pass):
//   c0 = [a, a, a, a]                    (against row position 0)
//   c1 = [c, f, -f, -c]                  (against row position 2)
//   c2 = [a, -a, -a, a]                  (against row position 4)
//   c3 = [f, -c, c, -f]                  (against row position 6)
//   c4 = [b, d, e, g]                    (against row position 1)
//   c5 = [d, -g, -b, -e]                 (against row position 3)
//   c6 = [e, -b, g, d]                   (against row position 5)
//   c7 = [g, -e, d, -b]                  (against row position 7)
//   even_sum = ((pos0*c0 + pos2*c1) + pos4*c2) + pos6*c3
//   odd_sum  = ((pos1*c4 + pos3*c5) + pos5*c6) + pos7*c7
//   out[0..4] = even_sum + odd_sum, out[4..8] = reverse(even_sum - odd_sum)
//
// Column pass reuses dct_inverse_8x8_pass's alpha0..3/theta0..3/gamma0..3
// names and structure (including theta0 = a*(in0+in4), a single multiply
// after adding - unlike avx_identical's two separate multiplies) - only
// beta0..beta3 differ, each grouped as a tree of two pairs instead of
// scalar's left-to-right chain, e.g. beta0 = (in1*b + in3*d) + (in5*e + in7*g).
//
// Verify the same way avx_identical was verified: temporarily point
// dwa/mod.rs's dct_inverse_8x8(...) call at the new function, assert_eq!
// (not tolerance) against tests/images/valid/custom/dwa_csc/*_ground_truth.exr,
// then revert the call site.
//
// Also evaluate the `pulp` crate (safe SIMD with runtime feature detection,
// unlike `wide`'s compile-time-only dispatch) as an alternative to hand-
// deriving sse2_identical/avx_identical, and/or as a way to pick between
// them at runtime.

// Original scalar port that `dct_inverse_8x8_pass` replaced, kept for
// reference.
//
// fn dct_inverse_8x8_scalar(data: &mut [f32; 64]) {
//     // Truncated PI literal, matching OpenEXR's constant.
//     const PI: f32 = 3.14159;
//
//     let a = 0.5 * (PI / 4.0).cos();
//     let b = 0.5 * (PI / 16.0).cos();
//     let c = 0.5 * (PI / 8.0).cos();
//     let d = 0.5 * ((3.0 * PI) / 16.0).cos();
//     let e = 0.5 * ((5.0 * PI) / 16.0).cos();
//     let f = 0.5 * ((3.0 * PI) / 8.0).cos();
//     let g = 0.5 * ((7.0 * PI) / 16.0).cos();
//
//     let mut alpha = [0f32; 4];
//     let mut beta = [0f32; 4];
//     let mut theta = [0f32; 4];
//     let mut gamma = [0f32; 4];
//
//     // First pass - row wise
//     for row in 0..8 {
//         let base = row * 8;
//         let row_ptr = &mut data[base..base + 8];
//
//         alpha[0] = c * row_ptr[2];
//         alpha[1] = f * row_ptr[2];
//         alpha[2] = c * row_ptr[6];
//         alpha[3] = f * row_ptr[6];
//
//         beta[0] = b * row_ptr[1] + d * row_ptr[3] + e * row_ptr[5] + g * row_ptr[7];
//         beta[1] = d * row_ptr[1] - g * row_ptr[3] - b * row_ptr[5] - e * row_ptr[7];
//         beta[2] = e * row_ptr[1] - b * row_ptr[3] + g * row_ptr[5] + d * row_ptr[7];
//         beta[3] = g * row_ptr[1] - e * row_ptr[3] + d * row_ptr[5] - b * row_ptr[7];
//
//         theta[0] = a * (row_ptr[0] + row_ptr[4]);
//         theta[3] = a * (row_ptr[0] - row_ptr[4]);
//
//         theta[1] = alpha[0] + alpha[3];
//         theta[2] = alpha[1] - alpha[2];
//
//         gamma[0] = theta[0] + theta[1];
//         gamma[1] = theta[3] + theta[2];
//         gamma[2] = theta[3] - theta[2];
//         gamma[3] = theta[0] - theta[1];
//
//         row_ptr[0] = gamma[0] + beta[0];
//         row_ptr[1] = gamma[1] + beta[1];
//         row_ptr[2] = gamma[2] + beta[2];
//         row_ptr[3] = gamma[3] + beta[3];
//
//         row_ptr[4] = gamma[3] - beta[3];
//         row_ptr[5] = gamma[2] - beta[2];
//         row_ptr[6] = gamma[1] - beta[1];
//         row_ptr[7] = gamma[0] - beta[0];
//     }
//
//     // Second pass - column wise
//     for column in 0..8 {
//         alpha[0] = c * data[16 + column];
//         alpha[1] = f * data[16 + column];
//         alpha[2] = c * data[48 + column];
//         alpha[3] = f * data[48 + column];
//
//         beta[0] = b * data[8 + column]
//             + d * data[24 + column]
//             + e * data[40 + column]
//             + g * data[56 + column];
//
//         beta[1] = d * data[8 + column]
//             - g * data[24 + column]
//             - b * data[40 + column]
//             - e * data[56 + column];
//
//         beta[2] = e * data[8 + column] - b * data[24 + column]
//             + g * data[40 + column]
//             + d * data[56 + column];
//
//         beta[3] = g * data[8 + column] - e * data[24 + column] + d * data[40 + column]
//             - b * data[56 + column];
//
//         theta[0] = a * (data[column] + data[32 + column]);
//         theta[3] = a * (data[column] - data[32 + column]);
//
//         theta[1] = alpha[0] + alpha[3];
//         theta[2] = alpha[1] - alpha[2];
//
//         gamma[0] = theta[0] + theta[1];
//         gamma[1] = theta[3] + theta[2];
//         gamma[2] = theta[3] - theta[2];
//         gamma[3] = theta[0] - theta[1];
//
//         data[column] = gamma[0] + beta[0];
//         data[8 + column] = gamma[1] + beta[1];
//         data[16 + column] = gamma[2] + beta[2];
//         data[24 + column] = gamma[3] + beta[3];
//
//         data[32 + column] = gamma[3] - beta[3];
//         data[40 + column] = gamma[2] - beta[2];
//         data[48 + column] = gamma[1] - beta[1];
//         data[56 + column] = gamma[0] - beta[0];
//     }
// }
