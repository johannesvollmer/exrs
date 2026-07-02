// Inverse DCT for DWA, ported from OpenEXRCore's scalar dctInverse8x8_scalar,
// vectorized 8-wide with the `wide` crate (safe; its internal unsafe code)

// Rather than vectorizing across the 8 coefficients *within* one row/column
// (which is how OpenEXR's own SSE2/AVX port works, and requires shuffling
// values between lanes), this vectorizes across the 8 independent rows (row pass)
// and the 8 independent columns (column pass): lane if every `f32x8` holds
// row/column i's value. Every `+`/`-`/`*` below is
// then a per-lane op with no cross-lane interaction, so each lane computes
// exactly the same sequence of scalar operations, in the same order, that a
// plain scalar loop over that row/column would - making this bit-identical
// to a scalar port while still running 8 rows/columns at once.
//
// `wide::f32x8` picks its backing implementation (AVX, SSE2-as-two-halves,
// or a portable fallback) via `#[cfg(target_feature = ...)]`, resolved at
// compile time - not `is_x86_feature_detected!` runtime dispatch - so there's
// no per-block branch/overhead, which matters since DWA decodes many small
// 8x8 blocks.
//
// Caveat: this matches OpenEXR's *scalar* reference, not its own
// SIMD-dispatched decoder. OpenEXR's SSE2/AVX path (internal_dwa_simd.h)
// hardcodes its basis constants as 6-digit decimal literals instead of this
// scalar paths runtime `cosf(...)` result - 4 of the 7 differ by exactly 1
// ULP - and sums in a different (tree, not left-to-right chain) order. Since
// real-world builds dispatch to SIMD by default, a fresh real-OpenEXR decode
// differs from this port by ~1-2 ULP in half precision on some samples - not
// a bug, just the scalar/SIMD ambiguity upstream itself has. See
// tests/across_compression.rs and tests/dwa_csc.rs for how the test suite's
// lossy-compression tolerance accounts for that gap.
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
// `k` of all 8 lines. Mirrors OpenEXR's scalar per-line loop body exactly.
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
    // literal PI (not full precision) for bit-identical output.
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

    // Row pass: lane i = row i. `data` is row-major, so gathering "column k
    // across all rows" is a strided read.
    let columns: [f32x8; 8] =
        std::array::from_fn(|k| f32x8::new(std::array::from_fn(|row| data[row * 8 + k])));

    let rows_out = dct_inverse_8x8_pass(&coef, columns);
    for (column, result) in rows_out.iter().enumerate() {
        let values = result.to_array();
        for (row, value) in values.iter().enumerate() {
            data[row * 8 + column] = *value;
        }
    }

    // Column pass: lane i = column i. Each row is already contiguous, so
    // this needs no gather, just 8 plain loads/stores.
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

// Original scalar port that `dct_inverse_8x8_pass` is replaced, kept here
// for reference.
//
// fn dct_inverse_8x8_scalar(data: &mut [f32; 64]) {
//     // Matches OpenEXR's dctInverse8x8_scalar, which uses this truncated PI
//     // literal (not full precision) for bit-identical output.
//     //
//     // Caveat: OpenEXR itself isn't internally bit-identical here. Its
//     // SSE2/AVX path (internal_dwa_simd.h) hardcodes these same 7 basis
//     // constants as 6-digit decimal literals instead of this scalar path's
//     // runtime `cosf(...)` result - 4 of the 7 differ by exactly 1 ULP. Since
//     // real-world builds dispatch to SIMD by default, files from the "real"
//     // library almost always reflect those SIMD constants, not the scalar
//     // ones this reference (and this port) computes.
//     //
//     // So a fresh real-OpenEXR decode differs from this (scalar-matching)
//     // port by ~1-2 ULP in half precision on some samples - not a bug, just
//     // the scalar/SIMD ambiguity upstream itself has.
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
