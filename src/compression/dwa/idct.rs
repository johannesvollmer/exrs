// Scalar 8x8 inverse DCT for DWA, ported from OpenEXRCore.

/// Inverse DCT on 8x8 block (in-place). `data` is 64 floats in row-major.
///
/// By default matches OpenEXR's *scalar* dctInverse8x8_scalar exactly. With
/// the `dwa_simd_identical` feature (x86_64 only), matches OpenEXR's
/// SIMD-dispatched (SSE2/AVX) path instead - see `super::idct_simd` and the
/// comment below on why these two upstream paths aren't the same.
pub fn dct_inverse_8x8(data: &mut [f32; 64]) {
    #[cfg(all(feature = "dwa_simd_identical", target_arch = "x86_64"))]
    {
        super::idct_simd::dct_inverse_8x8_simd(data);
        return;
    }

    #[allow(unreachable_code)]
    dct_inverse_8x8_scalar(data);
}

fn dct_inverse_8x8_scalar(data: &mut [f32; 64]) {
    // Matches OpenEXR's dctInverse8x8_scalar, which uses this truncated PI
    // literal (not full precision) for bit-identical output.
    //
    // Caveat: OpenEXR itself isn't internally bit-identical here. Its
    // SSE2/AVX path (internal_dwa_simd.h) hardcodes these same 7 basis
    // constants as 6-digit decimal literals instead of this scalar path's
    // runtime `cosf(...)` result - 4 of the 7 differ by exactly 1 ULP. Since
    // real-world builds dispatch to SIMD by default, files from the "real"
    // library almost always reflect those SIMD constants, not the scalar
    // ones this reference (and this port) computes.
    //
    // So a fresh real-OpenEXR decode differs from this (scalar-matching)
    // port by ~1-2 ULP in half precision on some samples - not a bug, just
    // the scalar/SIMD ambiguity upstream itself has. `dwa_simd_identical`
    // closes that gap; see tests/across_compression.rs and tests/dwa_csc.rs
    // for how the test suite accounts for it.
    const PI: f32 = 3.14159;

    let a = 0.5 * (PI / 4.0).cos();
    let b = 0.5 * (PI / 16.0).cos();
    let c = 0.5 * (PI / 8.0).cos();
    let d = 0.5 * ((3.0 * PI) / 16.0).cos();
    let e = 0.5 * ((5.0 * PI) / 16.0).cos();
    let f = 0.5 * ((3.0 * PI) / 8.0).cos();
    let g = 0.5 * ((7.0 * PI) / 16.0).cos();

    let mut alpha = [0f32; 4];
    let mut beta = [0f32; 4];
    let mut theta = [0f32; 4];
    let mut gamma = [0f32; 4];

    // First pass - row wise
    for row in 0..8 {
        let base = row * 8;
        let row_ptr = &mut data[base..base + 8];

        alpha[0] = c * row_ptr[2];
        alpha[1] = f * row_ptr[2];
        alpha[2] = c * row_ptr[6];
        alpha[3] = f * row_ptr[6];

        beta[0] = b * row_ptr[1] + d * row_ptr[3] + e * row_ptr[5] + g * row_ptr[7];
        beta[1] = d * row_ptr[1] - g * row_ptr[3] - b * row_ptr[5] - e * row_ptr[7];
        beta[2] = e * row_ptr[1] - b * row_ptr[3] + g * row_ptr[5] + d * row_ptr[7];
        beta[3] = g * row_ptr[1] - e * row_ptr[3] + d * row_ptr[5] - b * row_ptr[7];

        theta[0] = a * (row_ptr[0] + row_ptr[4]);
        theta[3] = a * (row_ptr[0] - row_ptr[4]);

        theta[1] = alpha[0] + alpha[3];
        theta[2] = alpha[1] - alpha[2];

        gamma[0] = theta[0] + theta[1];
        gamma[1] = theta[3] + theta[2];
        gamma[2] = theta[3] - theta[2];
        gamma[3] = theta[0] - theta[1];

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
        alpha[0] = c * data[16 + column];
        alpha[1] = f * data[16 + column];
        alpha[2] = c * data[48 + column];
        alpha[3] = f * data[48 + column];

        beta[0] =
            b * data[8 + column] +
            d * data[24 + column] +
            e * data[40 + column] +
            g * data[56 + column];

        beta[1] =
            d * data[8 + column] -
            g * data[24 + column] -
            b * data[40 + column] -
            e * data[56 + column];

        beta[2] =
            e * data[8 + column] -
            b * data[24 + column] +
            g * data[40 + column] +
            d * data[56 + column];

        beta[3] =
            g * data[8 + column] -
            e * data[24 + column] +
            d * data[40 + column] -
            b * data[56 + column];

        theta[0] = a * (data[column] + data[32 + column]);
        theta[3] = a * (data[column] - data[32 + column]);

        theta[1] = alpha[0] + alpha[3];
        theta[2] = alpha[1] - alpha[2];

        gamma[0] = theta[0] + theta[1];
        gamma[1] = theta[3] + theta[2];
        gamma[2] = theta[3] - theta[2];
        gamma[3] = theta[0] - theta[1];

        data[column] = gamma[0] + beta[0];
        data[8 + column] = gamma[1] + beta[1];
        data[16 + column] = gamma[2] + beta[2];
        data[24 + column] = gamma[3] + beta[3];

        data[32 + column] = gamma[3] - beta[3];
        data[40 + column] = gamma[2] - beta[2];
        data[48 + column] = gamma[1] - beta[1];
        data[56 + column] = gamma[0] - beta[0];
    }
}

/// Optimized path when only DC is non-zero.
pub fn dct_inverse_8x8_dc_only(data: &mut [f32; 64]) {
    let val = data[0] * 0.3535536f32 * 0.3535536f32;
    for v in data.iter_mut() {
        *v = val;
    }
}
