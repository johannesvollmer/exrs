// Scalar 8x8 inverse DCT for DWA, ported from OpenEXRCore.

/// Inverse DCT on 8x8 block (in-place), scalar implementation.
/// `data` is 64 floats in row-major.
pub fn dct_inverse_8x8(data: &mut [f32; 64]) {
    dct_inverse_8x8_scalar(data, 0);
}

fn dct_inverse_8x8_scalar(data: &mut [f32; 64], zeroed_rows: usize) {
    // Matches OpenEXR's dctInverse8x8_scalar, which uses this truncated
    // literal instead of a full-precision PI, for bit-identical output.
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
    for row in 0..8 - zeroed_rows {
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
