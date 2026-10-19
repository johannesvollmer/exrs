// Discrete cosine transform (forward and inverse) for DWA, ported from
// OpenEXRCore's internal_dwa_simd.h, including its runtime CPU dispatch:
// `dct_inverse_8x8_batch`/`dct_forward_8x8_batch` pick the best available x86
// tier at runtime (avx2 > sse2 > scalar), like OpenEXRs cpuid-based
// `initializeFuncs`
//
// Dispatch uses pulp's V3/V1 tokens, constructed only after a runtime CPU
// feature check. V3 needs AVX2+FMA, so
// AVX-only CPUs (Sandy/Ivy Bridge) fall back to sse2 here, where OpenEXR
// would use its own (non-avx2) avx kernel.
//
// The three kernels aren't bit-identical to each other (OpenEXRs own
// kernels disagree too: basis-constant precision and summation order
// differ)

// public only for benchmarking (the runtime dispatch below and the tier tests
// reach the individual kernels through this)
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[doc(hidden)]
pub mod x86;

// public only for benchmarking (benches/dct.rs reaches
// `test::pseudo_random_blocks`)
#[cfg(any(test, feature = "simd-benches"))]
#[doc(hidden)]
pub mod test;

// Autovectorized fallback: OpenEXRs "dctInverse8x8_scalar", including its
// truncated PI constant and summation order. Written as straightforward
// fixed-size loops that LLVM can autovectorize without explicit SIMD.
// public only for benchmarking (the in-crate dispatch and tests reach it
// directly)
#[doc(hidden)]
pub fn dct_inverse_8x8_autovectorized(data: &mut [f32; 64]) {
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

    // First pass; row wise
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

    // Second pass; column wise
    for column in 0..8 {
        alpha[0] = c * data[16 + column];
        alpha[1] = f * data[16 + column];
        alpha[2] = c * data[48 + column];
        alpha[3] = f * data[48 + column];

        beta[0] = b * data[8 + column]
            + d * data[24 + column]
            + e * data[40 + column]
            + g * data[56 + column];

        beta[1] = d * data[8 + column]
            - g * data[24 + column]
            - b * data[40 + column]
            - e * data[56 + column];

        beta[2] = e * data[8 + column] - b * data[24 + column]
            + g * data[40 + column]
            + d * data[56 + column];

        beta[3] = g * data[8 + column] - e * data[24 + column] + d * data[40 + column]
            - b * data[56 + column];

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

// Normalized DCT-II basis, indexed by [input][output], shared by all forward
// kernels. Precomputed in f32 as:
//   scale * (((2 * input + 1) as f32 * output as f32 * PI) / 16.0).cos()
// where PI = 3.14159 and scale = 0.5 * (output == 0 ? 0.70710677 : 1.0).
// Preserve the original truncated PI and f32 rounding: using symmetry or a
// more precise PI here would change the existing SIMD encoder's coefficients.
#[rustfmt::skip]
const FORWARD_BASIS: [[f32; 8]; 8] = [
    [0.35355338, 0.49039266, 0.46193984, 0.41573495, 0.35355362, 0.27778545, 0.19134216, 0.097545706],
    [0.35355338, 0.41573495, 0.19134216, -0.09754446, -0.35355273, -0.49039242, -0.46194032, -0.2777866],
    [0.35355338, 0.27778545, -0.19134098, -0.49039242, -0.35355452, 0.097543105, 0.46193886, 0.4157363],
    [0.35355338, 0.097545706, -0.46193933, -0.2777866, 0.35355183, 0.4157363, -0.19133842, -0.49039343],
    [0.35355338, -0.09754446, -0.46194032, 0.27778342, 0.3535554, -0.41573277, -0.19134545, 0.4903917],
    [0.35355338, -0.27778438, -0.19134332, 0.49039316, -0.3535509, -0.09754932, 0.46194187, -0.41573134],
    [0.35355338, -0.41573423, 0.19133982, 0.097548075, -0.3535563, 0.49039367, -0.4619375, 0.27777922],
    [0.35355338, -0.49039242, 0.46193886, -0.41573277, 0.35355002, -0.27778026, 0.1913349, -0.09753735],
];

/// Autovectorized forward DCT for DWA 8x8 blocks. Apply the fixed 1D basis
/// along rows and then columns, as in the explicit SIMD kernels.
// public only for benchmarking (the in-crate dispatch and tests reach it directly)
#[doc(hidden)]
pub fn dct_forward_8x8_autovectorized(data: &mut [f32; 64]) {
    let mut rows = [0.0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            for u in 0..8 {
                rows[y * 8 + u] += data[y * 8 + x] * FORWARD_BASIS[x][u];
            }
        }
    }

    data.fill(0.0);
    for y in 0..8 {
        for v in 0..8 {
            for u in 0..8 {
                data[v * 8 + u] += rows[y * 8 + u] * FORWARD_BASIS[y][v];
            }
        }
    }
}

/// Forward DCT on many 8x8 blocks, dispatched once for the whole batch rather
/// than once per block. Prefer this over looping calls to `dct_forward_8x8`.
pub(crate) fn dct_forward_8x8_batch<'a>(mut blocks: impl Iterator<Item = &'a mut [f32; 64]>) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if x86::try_dct_forward_8x8_batch(&mut blocks) {
        return;
    }

    for data in blocks {
        dct_forward_8x8_autovectorized(data);
    }
}

/// Inverse DCT on many 8x8 blocks, dispatched once for the whole batch
/// rather than once per block. Prefer this over looping calls to
/// `dct_inverse_8x8`
pub(crate) fn dct_inverse_8x8_batch<'a>(mut blocks: impl Iterator<Item = &'a mut [f32; 64]>) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if x86::try_dct_inverse_8x8_batch(&mut blocks) {
        return;
    }

    for data in blocks {
        dct_inverse_8x8_autovectorized(data);
    }
}

/// Optimized path when only DC is non-zero.
pub(crate) fn dct_inverse_8x8_dc_only(data: &mut [f32; 64]) {
    let val = data[0] * 0.3535536f32 * 0.3535536f32;
    for v in data.iter_mut() {
        *v = val;
    }
}
