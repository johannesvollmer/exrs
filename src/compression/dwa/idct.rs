// Inverse DCT for DWA, ported from OpenEXRCore's internal_dwa_simd.h,
// including its runtime CPU dispatch: `dct_inverse_8x8`/`dct_inverse_8x8_batch`
// pick the best available x86 tier at runtime (avx2 > sse2 > scalar), like
// OpenEXRs cpuid-based `initializeFuncs`
//
// Dispatch uses pulp's V3/V1 tokens, constructed only after a runtime CPU
// feature check. V3 needs AVX2+FMA, so
// AVX-only CPUs (Sandy/Ivy Bridge) fall back to sse2 here, where OpenEXR
// would use its own (non-avx2) avx kernel.
//
// The three kernels aren't bit-identical to each other (OpenEXRs own
// kernels disagree too: basis-constant precision and summation order
// differ)

use std::sync::OnceLock;

// AVX2 V3 tier: OpenEXR's "dctInverse8x8_avx_0". Each pass runs all 8
// rows/columns of the block in parallel, one 8-wide register per position.
//
// `dct_inverse_8x8_batch` runs the kernel through `V3::vectorize` rather
// than calling it as an ordinary function. `vectorize` is pulps own
// inherent, `#[target_feature]`-scoped, internally-unsafe trampoline
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx {
    use pulp::{f32x8, x86::V3};

    // OpenEXRs hardcoded AVX basis constants ("sAvxCoef").
    const A: f32 = 3.535536e-1;
    const B: f32 = 4.903927e-1;
    const C: f32 = 4.619398e-1;
    const D: f32 = 4.157349e-1;
    const E: f32 = 2.777855e-1;
    const F: f32 = 1.913422e-1;
    const G: f32 = 9.754573e-2;

    struct Coefficients {
        a: f32x8,
        na: f32x8,
        b: f32x8,
        nb: f32x8,
        c: f32x8,
        nc: f32x8,
        d: f32x8,
        // no "nd": the AVX never multiplies by -D
        e: f32x8,
        ne: f32x8,
        f: f32x8,
        nf: f32x8,
        g: f32x8,
        ng: f32x8,
    }

    impl Coefficients {
        // This, `row_pass`, and `column_pass` must inline into the
        // `vectorize` closure below for their ops to fuse into avx2
        // instructions; LLVM inlining heuristics aren't reliable
        // enough to guarantee that on their own
        #[inline(always)]
        fn new(v3: V3) -> Self {
            // Negated splats are exact (sign flip), so "x * na == -(x * a)".
            Self {
                a: v3.splat_f32x8(A),
                na: v3.splat_f32x8(-A),
                b: v3.splat_f32x8(B),
                nb: v3.splat_f32x8(-B),
                c: v3.splat_f32x8(C),
                nc: v3.splat_f32x8(-C),
                d: v3.splat_f32x8(D),
                e: v3.splat_f32x8(E),
                ne: v3.splat_f32x8(-E),
                f: v3.splat_f32x8(F),
                nf: v3.splat_f32x8(-F),
                g: v3.splat_f32x8(G),
                ng: v3.splat_f32x8(-G),
            }
        }
    }

    // OpenEXRs "IDCT_AVX_MMULT_ROWS" + "EO_TO_ROW_HALVES"
    #[inline(always)] // must fuse into the `vectorize` closure --> see `Coefficients::new`
    fn row_pass(v3: V3, coef: &Coefficients, input: [f32x8; 8]) -> [f32x8; 8] {
        let mul = |a, b| v3.mul_f32x8(a, b);
        let add = |a, b| v3.add_f32x8(a, b);
        let sub = |a, b| v3.sub_f32x8(a, b);

        let (in0, in2, in4, in6) = (input[0], input[2], input[4], input[6]);
        let (in1, in3, in5, in7) = (input[1], input[3], input[5], input[7]);

        let even0 =
            add(add(mul(in4, coef.a), mul(in6, coef.f)), add(mul(in0, coef.a), mul(in2, coef.c)));
        let even1 =
            add(add(mul(in4, coef.na), mul(in6, coef.nc)), add(mul(in0, coef.a), mul(in2, coef.f)));
        let even2 =
            add(add(mul(in4, coef.na), mul(in6, coef.c)), add(mul(in0, coef.a), mul(in2, coef.nf)));
        let even3 =
            add(add(mul(in4, coef.a), mul(in6, coef.nf)), add(mul(in0, coef.a), mul(in2, coef.nc)));

        let odd0 =
            add(add(mul(in5, coef.e), mul(in7, coef.g)), add(mul(in1, coef.b), mul(in3, coef.d)));
        let odd1 = add(
            add(mul(in5, coef.nb), mul(in7, coef.ne)),
            add(mul(in1, coef.d), mul(in3, coef.ng)),
        );
        let odd2 =
            add(add(mul(in5, coef.g), mul(in7, coef.d)), add(mul(in1, coef.e), mul(in3, coef.nb)));
        let odd3 =
            add(add(mul(in5, coef.d), mul(in7, coef.nb)), add(mul(in1, coef.g), mul(in3, coef.ne)));

        [
            add(even0, odd0),
            add(even1, odd1),
            add(even2, odd2),
            add(even3, odd3),
            sub(even3, odd3),
            sub(even2, odd2),
            sub(even1, odd1),
            sub(even0, odd0),
        ]
    }

    // The column transform from the back half of "dctInverse8x8_avx_0".
    #[inline(always)] // must fuse into the `vectorize` closure --> see `Coefficients::new`
    fn column_pass(v3: V3, coef: &Coefficients, input: [f32x8; 8]) -> [f32x8; 8] {
        let mul = |a, b| v3.mul_f32x8(a, b);
        let add = |a, b| v3.add_f32x8(a, b);
        let sub = |a, b| v3.sub_f32x8(a, b);

        let (in0, in1, in2, in3, in4, in5, in6, in7) =
            (input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7]);

        let beta0 =
            add(add(mul(coef.g, in7), mul(coef.e, in5)), add(mul(coef.d, in3), mul(coef.b, in1)));
        let beta1 =
            sub(sub(mul(coef.d, in1), add(mul(coef.b, in5), mul(coef.g, in3))), mul(coef.e, in7));
        let beta2 =
            add(mul(coef.d, in7), add(mul(coef.g, in5), sub(mul(coef.e, in1), mul(coef.b, in3))));
        let beta3 =
            sub(add(mul(coef.d, in5), mul(coef.g, in1)), add(mul(coef.b, in7), mul(coef.e, in3)));

        let theta0 = add(mul(coef.a, in4), mul(coef.a, in0));
        let theta3 = sub(mul(coef.a, in0), mul(coef.a, in4));

        let theta1 = add(mul(coef.f, in6), mul(coef.c, in2));
        let gamma0 = add(theta1, theta0);
        let gamma3 = sub(theta0, theta1);

        let theta2 = sub(mul(coef.f, in2), mul(coef.c, in6));
        let gamma1 = add(theta3, theta2);
        let gamma2 = sub(theta3, theta2);

        [
            add(gamma0, beta0),
            add(gamma1, beta1),
            add(gamma2, beta2),
            add(gamma3, beta3),
            sub(gamma3, beta3),
            sub(gamma2, beta2),
            sub(gamma1, beta1),
            sub(gamma0, beta0),
        ]
    }

    #[cfg(any(feature = "avx2-tests", feature = "simd-benches"))]
    pub fn dct_inverse_8x8(v3: V3, data: &mut [f32; 64]) {
        dct_inverse_8x8_batch(v3, std::iter::once(data));
    }

    // `V3::vectorize` runs a `FnOnce()` closure inside pulps own
    // `#[target_feature(enable = "avx2,fma")]` trampoline; passing the
    // kernel as a closure, rather than calling it as an ordinary function,
    // is what lets that closures body inline and fuse into avx2
    // instructions.
    //
    // `vectorize` also has fixed overhead per call (an indirect call
    // through its register-passing trampoline, plus a feature-cache check)
    pub fn dct_inverse_8x8_batch<'a>(v3: V3, blocks: impl Iterator<Item = &'a mut [f32; 64]>) {
        v3.vectorize(move || {
            let coef = Coefficients::new(v3);

            for data in blocks {
                // Row pass: lane i = row i, gathered with a strided read
                // (data is row-major).
                let columns: [f32x8; 8] = std::array::from_fn(|k| {
                    f32x8(
                        data[k],
                        data[8 + k],
                        data[16 + k],
                        data[24 + k],
                        data[32 + k],
                        data[40 + k],
                        data[48 + k],
                        data[56 + k],
                    )
                });

                let rows_out = row_pass(v3, &coef, columns);
                for (column, result) in rows_out.iter().enumerate() {
                    let r = [
                        result.0, result.1, result.2, result.3, result.4, result.5, result.6,
                        result.7,
                    ];
                    for (row, value) in r.iter().enumerate() {
                        data[row * 8 + column] = *value;
                    }
                }

                // Column pass: lane i = column i, each row already contiguous.
                let rows: [f32x8; 8] = std::array::from_fn(|row| {
                    let b = row * 8;
                    f32x8(
                        data[b],
                        data[b + 1],
                        data[b + 2],
                        data[b + 3],
                        data[b + 4],
                        data[b + 5],
                        data[b + 6],
                        data[b + 7],
                    )
                });

                let columns_out = column_pass(v3, &coef, rows);
                for (row, result) in columns_out.iter().enumerate() {
                    let b = row * 8;
                    data[b] = result.0;
                    data[b + 1] = result.1;
                    data[b + 2] = result.2;
                    data[b + 3] = result.3;
                    data[b + 4] = result.4;
                    data[b + 5] = result.5;
                    data[b + 6] = result.6;
                    data[b + 7] = result.7;
                }
            }
        });
    }
}

// SSE2 V1 tier: OpenEXRs "dctInverse8x8_sse2". Vectorizes 4
// output positions of one row at a time a different shape than "avx",
// so the two are not bit-identical.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse2 {
    use pulp::{f32x4, x86::V1};

    const A: f32 = 3.535536e-1;
    const B: f32 = 4.903927e-1;
    const C: f32 = 4.619398e-1;
    const D: f32 = 4.157349e-1;
    const E: f32 = 2.777855e-1;
    const F: f32 = 1.913422e-1;
    const G: f32 = 9.754573e-2;

    // Row-pass matrix columns (c0..c3 even positions, c4..c7 odd).
    struct RowCoefficients {
        c0: f32x4,
        c1: f32x4,
        c2: f32x4,
        c3: f32x4,
        c4: f32x4,
        c5: f32x4,
        c6: f32x4,
        c7: f32x4,
    }

    impl RowCoefficients {
        fn new() -> Self {
            Self {
                c0: f32x4(A, A, A, A),
                c1: f32x4(C, F, -F, -C),
                c2: f32x4(A, -A, -A, A),
                c3: f32x4(F, -C, C, -F),
                c4: f32x4(B, D, E, G),
                c5: f32x4(D, -G, -B, -E),
                c6: f32x4(E, -B, G, D),
                c7: f32x4(G, -E, D, -B),
            }
        }
    }

    struct ColumnCoefficients {
        a: f32x4,
        b: f32x4,
        c: f32x4,
        d: f32x4,
        e: f32x4,
        f: f32x4,
        g: f32x4,
    }

    impl ColumnCoefficients {
        fn new(v1: V1) -> Self {
            Self {
                a: v1.splat_f32x4(A),
                b: v1.splat_f32x4(B),
                c: v1.splat_f32x4(C),
                d: v1.splat_f32x4(D),
                e: v1.splat_f32x4(E),
                f: v1.splat_f32x4(F),
                g: v1.splat_f32x4(G),
            }
        }
    }

    // One row, summed strictly left-to-right from an explicit zero register,
    // matching "DCT_INVERSE_8x8_SS2_ROW_LOOP"s "_mm_add_ps" exactly.
    fn row_pass(v1: V1, coef: &RowCoefficients, row: &[f32]) -> (f32x4, f32x4) {
        let mul = |a, b| v1.mul_f32x4(a, b);
        let add = |a, b| v1.add_f32x4(a, b);
        let sub = |a, b| v1.sub_f32x4(a, b);
        let broadcast = |v: f32| v1.splat_f32x4(v);

        let x0 = mul(broadcast(row[0]), coef.c0);
        let x2 = mul(broadcast(row[2]), coef.c1);
        let x4 = mul(broadcast(row[4]), coef.c2);
        let x6 = mul(broadcast(row[6]), coef.c3);

        let x1 = mul(broadcast(row[1]), coef.c4);
        let x3 = mul(broadcast(row[3]), coef.c5);
        let x5 = mul(broadcast(row[5]), coef.c6);
        let x7 = mul(broadcast(row[7]), coef.c7);

        let zero = v1.splat_f32x4(0.0);
        let even = add(add(add(add(zero, x0), x2), x4), x6);
        let odd = add(add(add(add(zero, x1), x3), x5), x7);

        let lo = add(even, odd);
        let hi = sub(even, odd);
        (lo, f32x4(hi.3, hi.2, hi.1, hi.0))
    }

    // Same alpha/theta/gamma structure as the scalar kernel, but with
    // beta0..beta3 tree-paired, matching "dctInverse8x8_sse2" exactly.
    fn column_pass(v1: V1, coef: &ColumnCoefficients, input: [f32x4; 8]) -> [f32x4; 8] {
        let mul = |a, b| v1.mul_f32x4(a, b);
        let add = |a, b| v1.add_f32x4(a, b);
        let sub = |a, b| v1.sub_f32x4(a, b);

        let (in0, in1, in2, in3, in4, in5, in6, in7) =
            (input[0], input[1], input[2], input[3], input[4], input[5], input[6], input[7]);

        let beta0 =
            add(add(mul(in1, coef.b), mul(in3, coef.d)), add(mul(in5, coef.e), mul(in7, coef.g)));
        let beta1 =
            sub(sub(mul(in1, coef.d), mul(in3, coef.g)), add(mul(in5, coef.b), mul(in7, coef.e)));
        let beta2 =
            add(sub(mul(in1, coef.e), mul(in3, coef.b)), add(mul(in5, coef.g), mul(in7, coef.d)));
        let beta3 =
            add(sub(mul(in1, coef.g), mul(in3, coef.e)), sub(mul(in5, coef.d), mul(in7, coef.b)));

        let theta0 = mul(coef.a, add(in0, in4));
        let theta3 = mul(coef.a, sub(in0, in4));

        let alpha0 = mul(coef.c, in2);
        let alpha1 = mul(coef.f, in2);
        let alpha2 = mul(coef.c, in6);
        let alpha3 = mul(coef.f, in6);

        let theta1 = add(alpha0, alpha3);
        let theta2 = sub(alpha1, alpha2);

        let gamma0 = add(theta0, theta1);
        let gamma1 = add(theta3, theta2);
        let gamma2 = sub(theta3, theta2);
        let gamma3 = sub(theta0, theta1);

        [
            add(gamma0, beta0),
            add(gamma1, beta1),
            add(gamma2, beta2),
            add(gamma3, beta3),
            sub(gamma3, beta3),
            sub(gamma2, beta2),
            sub(gamma1, beta1),
            sub(gamma0, beta0),
        ]
    }

    pub fn dct_inverse_8x8(v1: V1, data: &mut [f32; 64]) {
        let row_coef = RowCoefficients::new();
        for row in 0..8 {
            let base = row * 8;
            let (lo, hi) = row_pass(v1, &row_coef, &data[base..base + 8]);
            data[base] = lo.0;
            data[base + 1] = lo.1;
            data[base + 2] = lo.2;
            data[base + 3] = lo.3;
            data[base + 4] = hi.0;
            data[base + 5] = hi.1;
            data[base + 6] = hi.2;
            data[base + 7] = hi.3;
        }

        let col_coef = ColumnCoefficients::new(v1);
        // Two batches of 4 columns each.
        for half in 0..2 {
            let offset = half * 4;
            let input: [f32x4; 8] = std::array::from_fn(|row| {
                let b = row * 8 + offset;
                f32x4(data[b], data[b + 1], data[b + 2], data[b + 3])
            });
            let out = column_pass(v1, &col_coef, input);
            for (row, result) in out.iter().enumerate() {
                let b = row * 8 + offset;
                data[b] = result.0;
                data[b + 1] = result.1;
                data[b + 2] = result.2;
                data[b + 3] = result.3;
            }
        }
    }
}

// Scalar fallback: OpenEXRs "dctInverse8x8_scalar", including its
// truncated PI constant and summation order.
fn dct_inverse_8x8_scalar(data: &mut [f32; 64]) {
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

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn forward_basis() -> &'static [[f32; 8]; 8] {
    static TABLE: OnceLock<[[f32; 8]; 8]> = OnceLock::new();

    TABLE.get_or_init(|| {
        const PI: f32 = 3.14159;
        const INV_SQRT_2: f32 = 0.70710677;

        let mut table = [[0.0f32; 8]; 8];
        for input in 0..8 {
            for output in 0..8 {
                let scale = if output == 0 {
                    0.5 * INV_SQRT_2
                } else {
                    0.5
                };
                table[input][output] =
                    scale * (((2 * input + 1) as f32 * output as f32 * PI) / 16.0).cos();
            }
        }
        table
    })
}

/// Scalar forward DCT for DWA 8x8 blocks. This intentionally uses the
/// straightforward separable DCT formula for the first encoder version; LLVM
/// can still optimize the fixed-size loops without adding explicit SIMD paths.
fn dct_forward_8x8_scalar(data: &mut [f32; 64]) {
    // The forward path mirrors the inverse path's fixed 8x8 basis, but keeps
    // the implementation scalar and easy to verify against the reference.
    const PI: f32 = 3.14159;
    const INV_SQRT_2: f32 = 0.70710677;

    let input = *data;
    for v in 0..8 {
        for u in 0..8 {
            let cu = if u == 0 {
                INV_SQRT_2
            } else {
                1.0
            };
            let cv = if v == 0 {
                INV_SQRT_2
            } else {
                1.0
            };
            let mut sum = 0.0f32;

            for y in 0..8 {
                let cy = (((2 * y + 1) as f32 * v as f32 * PI) / 16.0).cos();
                for x in 0..8 {
                    let cx = (((2 * x + 1) as f32 * u as f32 * PI) / 16.0).cos();
                    sum += input[y * 8 + x] * cx * cy;
                }
            }

            data[v * 8 + u] = 0.25 * cu * cv * sum;
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod forward {
    use pulp::{
        f32x4, f32x8,
        x86::{V1, V3},
    };

    use super::forward_basis;

    struct Coefficients8 {
        terms: [f32x8; 8],
    }

    impl Coefficients8 {
        #[inline(always)]
        fn new(_v3: V3) -> Self {
            let basis = forward_basis();
            Self {
                terms: std::array::from_fn(|input| {
                    let row = basis[input];
                    f32x8(row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7])
                }),
            }
        }
    }

    struct Coefficients4 {
        low: [f32x4; 8],
        high: [f32x4; 8],
    }

    impl Coefficients4 {
        #[inline(always)]
        fn new(_v1: V1) -> Self {
            let basis = forward_basis();
            Self {
                low: std::array::from_fn(|input| {
                    let row = basis[input];
                    f32x4(row[0], row[1], row[2], row[3])
                }),
                high: std::array::from_fn(|input| {
                    let row = basis[input];
                    f32x4(row[4], row[5], row[6], row[7])
                }),
            }
        }
    }

    #[inline(always)]
    fn forward_pass8(v3: V3, coef: &Coefficients8, input: [f32; 8]) -> f32x8 {
        let mul = |a, b| v3.mul_f32x8(a, b);
        let add = |a, b| v3.add_f32x8(a, b);
        let splat = |value: f32| v3.splat_f32x8(value);

        let mut out = v3.splat_f32x8(0.0);
        for index in 0..8 {
            out = add(out, mul(splat(input[index]), coef.terms[index]));
        }
        out
    }

    #[inline(always)]
    fn forward_pass4(v1: V1, coef: &[f32x4; 8], input: [f32; 8]) -> f32x4 {
        let mul = |a, b| v1.mul_f32x4(a, b);
        let add = |a, b| v1.add_f32x4(a, b);
        let splat = |value: f32| v1.splat_f32x4(value);

        let mut out = v1.splat_f32x4(0.0);
        for index in 0..8 {
            out = add(out, mul(splat(input[index]), coef[index]));
        }
        out
    }

    #[allow(dead_code)]
    pub(super) fn dct_forward_8x8(v3: V3, data: &mut [f32; 64]) {
        dct_forward_8x8_batch(v3, std::iter::once(data));
    }

    pub(super) fn dct_forward_8x8_batch<'a>(
        v3: V3,
        blocks: impl Iterator<Item = &'a mut [f32; 64]>,
    ) {
        v3.vectorize(move || {
            let coef = Coefficients8::new(v3);

            for data in blocks {
                for row in 0..8 {
                    let base = row * 8;
                    let input = [
                        data[base],
                        data[base + 1],
                        data[base + 2],
                        data[base + 3],
                        data[base + 4],
                        data[base + 5],
                        data[base + 6],
                        data[base + 7],
                    ];
                    let out = forward_pass8(v3, &coef, input);
                    data[base] = out.0;
                    data[base + 1] = out.1;
                    data[base + 2] = out.2;
                    data[base + 3] = out.3;
                    data[base + 4] = out.4;
                    data[base + 5] = out.5;
                    data[base + 6] = out.6;
                    data[base + 7] = out.7;
                }

                for column in 0..8 {
                    let input = [
                        data[column],
                        data[8 + column],
                        data[16 + column],
                        data[24 + column],
                        data[32 + column],
                        data[40 + column],
                        data[48 + column],
                        data[56 + column],
                    ];
                    let out = forward_pass8(v3, &coef, input);
                    data[column] = out.0;
                    data[8 + column] = out.1;
                    data[16 + column] = out.2;
                    data[24 + column] = out.3;
                    data[32 + column] = out.4;
                    data[40 + column] = out.5;
                    data[48 + column] = out.6;
                    data[56 + column] = out.7;
                }
            }
        });
    }

    pub(super) fn dct_forward_8x8_sse2(v1: V1, data: &mut [f32; 64]) {
        let coef = Coefficients4::new(v1);

        for row in 0..8 {
            let base = row * 8;
            let input = [
                data[base],
                data[base + 1],
                data[base + 2],
                data[base + 3],
                data[base + 4],
                data[base + 5],
                data[base + 6],
                data[base + 7],
            ];
            let low = forward_pass4(v1, &coef.low, input);
            let high = forward_pass4(v1, &coef.high, input);
            data[base] = low.0;
            data[base + 1] = low.1;
            data[base + 2] = low.2;
            data[base + 3] = low.3;
            data[base + 4] = high.0;
            data[base + 5] = high.1;
            data[base + 6] = high.2;
            data[base + 7] = high.3;
        }

        for column in 0..8 {
            let input = [
                data[column],
                data[8 + column],
                data[16 + column],
                data[24 + column],
                data[32 + column],
                data[40 + column],
                data[48 + column],
                data[56 + column],
            ];
            let low = forward_pass4(v1, &coef.low, input);
            let high = forward_pass4(v1, &coef.high, input);
            data[column] = low.0;
            data[8 + column] = low.1;
            data[16 + column] = low.2;
            data[24 + column] = low.3;
            data[32 + column] = high.0;
            data[40 + column] = high.1;
            data[48 + column] = high.2;
            data[56 + column] = high.3;
        }
    }
}

/// Forward DCT on an 8x8 block (in-place, row-major), dispatched at runtime
/// to the best available x86 SIMD tier (avx2 > sse2 > scalar), like a real
/// OpenEXR build.
#[cfg(any(feature = "avx2-tests", feature = "sse2-tests"))]
pub fn dct_forward_8x8(data: &mut [f32; 64]) {
    dct_forward_8x8_batch(std::iter::once(data));
}

/// Forward DCT on many 8x8 blocks, dispatched once for the whole batch rather
/// than once per block. Prefer this over looping calls to `dct_forward_8x8`.
pub fn dct_forward_8x8_batch<'a>(blocks: impl Iterator<Item = &'a mut [f32; 64]>) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        use pulp::x86::{V1, V3};

        if let Some(v3) = V3::try_new() {
            forward::dct_forward_8x8_batch(v3, blocks);
            return;
        }
        if let Some(v1) = V1::try_new() {
            for data in blocks {
                forward::dct_forward_8x8_sse2(v1, data);
            }
            return;
        }
    }

    for data in blocks {
        dct_forward_8x8_scalar(data);
    }
}

/// Inverse DCT on an 8x8 block (in-place, row-major), dispatched at
/// runtime to the best available x86 SIMD tier (avx2 > sse2 > scalar),
/// like a real OpenEXR build. See the file header comment.
///
/// Only exists for the dispatch-comparison tests; production code
/// (`decode_lossy_dct_group` in `mod.rs`) calls `dct_inverse_8x8_batch`
/// directly, since it always has a whole group of blocks at once.
#[cfg(any(feature = "avx2-tests", feature = "sse2-tests"))]
pub fn dct_inverse_8x8(data: &mut [f32; 64]) {
    dct_inverse_8x8_batch(std::iter::once(data));
}

/// Inverse DCT on many 8x8 blocks, dispatched once for the whole batch
/// rather than once per block. Prefer this over looping calls to
/// `dct_inverse_8x8`
pub fn dct_inverse_8x8_batch<'a>(blocks: impl Iterator<Item = &'a mut [f32; 64]>) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        use pulp::x86::{V1, V3};

        if let Some(v3) = V3::try_new() {
            avx::dct_inverse_8x8_batch(v3, blocks);
            return;
        }
        if let Some(v1) = V1::try_new() {
            for data in blocks {
                sse2::dct_inverse_8x8(v1, data);
            }
            return;
        }
    }

    for data in blocks {
        dct_inverse_8x8_scalar(data);
    }
}

/// Optimized path when only DC is non-zero.
pub fn dct_inverse_8x8_dc_only(data: &mut [f32; 64]) {
    let val = data[0] * 0.3535536f32 * 0.3535536f32;
    for v in data.iter_mut() {
        *v = val;
    }
}

// Deterministic blocks in the ballpark of half-precision DCT coefficients
// (xorshift64, no `rand` dependency. Shared by the
// correctness tests below and by the forced-tier benchmark in benches/idct.rs.
#[cfg(any(feature = "avx2-tests", feature = "sse2-tests", feature = "simd-benches"))]
fn pseudo_random_blocks(count: usize) -> Vec<[f32; 64]> {
    let mut state: u64 = 0x9e3779b97f4a7c15;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (((state >> 40) as i32 as f32) / (i32::MAX as f32)) * 1024.0
    };
    (0..count).map(|_| std::array::from_fn(|_| next())).collect()
}

// Exposes each SIMD tier as a direct call function, bypassing the
// runtime dispatch in `dct_inverse_8x8`. Used by benches/idct.rs to measure
// if avx2/sse2 is actually faster then scalar fallback on real hardware
#[cfg(feature = "simd-benches")]
#[allow(missing_docs)]
pub mod simd_bench_support {
    use super::*;

    pub fn bench_blocks(count: usize) -> Vec<[f32; 64]> {
        pseudo_random_blocks(count)
    }

    pub fn dct_forward_8x8_forced_scalar(data: &mut [f32; 64]) {
        dct_forward_8x8_scalar(data);
    }

    /// Runs the SSE2 forward-DCT kernel directly; returns `false` without
    /// touching `data` if this CPU lacks SSE2.
    pub fn dct_forward_8x8_forced_sse2(data: &mut [f32; 64]) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if let Some(v1) = pulp::x86::V1::try_new() {
            forward::dct_forward_8x8_sse2(v1, data);
            return true;
        }

        #[allow(unreachable_code)]
        false
    }

    /// Runs the AVX2 forward-DCT kernel directly; returns `false` without
    /// touching `data` if this CPU lacks AVX2+FMA.
    pub fn dct_forward_8x8_forced_avx2(data: &mut [f32; 64]) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if let Some(v3) = pulp::x86::V3::try_new() {
            forward::dct_forward_8x8(v3, data);
            return true;
        }

        #[allow(unreachable_code)]
        false
    }

    /// Runs the AVX2 forward-DCT kernel over a whole batch through one
    /// `vectorize` call. Returns `false` without touching `blocks` if this
    /// CPU lacks AVX2+FMA.
    pub fn dct_forward_8x8_forced_avx2_batch<'a>(
        blocks: impl Iterator<Item = &'a mut [f32; 64]>,
    ) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if let Some(v3) = pulp::x86::V3::try_new() {
            forward::dct_forward_8x8_batch(v3, blocks);
            return true;
        }

        #[allow(unreachable_code)]
        {
            let _ = blocks;
            false
        }
    }

    pub fn dct_inverse_8x8_forced_scalar(data: &mut [f32; 64]) {
        dct_inverse_8x8_scalar(data);
    }

    /// Runs the SSE2 kernel directly; returns `false` without touching `data`
    /// if this CPU lacks SSE2 (practically: no x86_64).
    pub fn dct_inverse_8x8_forced_sse2(data: &mut [f32; 64]) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if let Some(v1) = pulp::x86::V1::try_new() {
            sse2::dct_inverse_8x8(v1, data);
            return true;
        }

        #[allow(unreachable_code)]
        false
    }

    /// Runs the AVX2 kernel directly; returns `false` without touching `data`
    /// if this CPU lacks AVX2+FMA.
    pub fn dct_inverse_8x8_forced_avx2(data: &mut [f32; 64]) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if let Some(v3) = pulp::x86::V3::try_new() {
            avx::dct_inverse_8x8(v3, data);
            return true;
        }

        #[allow(unreachable_code)]
        false
    }

    /// Runs the AVX2 kernel over the whole batch through one `vectorize`
    /// call, the way `decode_lossy_dct_group` does, instead of once per
    /// block like `dct_inverse_8x8_forced_avx2`. Returns `false` without
    /// touching `blocks` if this CPU lacks AVX2+FMA
    pub fn dct_inverse_8x8_forced_avx2_batch<'a>(
        blocks: impl Iterator<Item = &'a mut [f32; 64]>,
    ) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if let Some(v3) = pulp::x86::V3::try_new() {
            avx::dct_inverse_8x8_batch(v3, blocks);
            return true;
        }

        #[allow(unreachable_code)]
        {
            let _ = blocks;
            false
        }
    }
}

#[cfg(any(feature = "avx2-tests", feature = "sse2-tests"))]
#[allow(dead_code, missing_docs)]
pub mod simd_test_support {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SimdTier {
        Avx2,
        Sse2,
        Scalar,
    }

    pub fn selected_simd_tier() -> SimdTier {
        if has_avx2_tier() {
            return SimdTier::Avx2;
        }
        if has_sse2_tier() {
            return SimdTier::Sse2;
        }
        SimdTier::Scalar
    }

    fn has_avx2_tier() -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            return pulp::x86::V3::try_new().is_some();
        }

        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            false
        }
    }

    fn has_sse2_tier() -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            return pulp::x86::V1::try_new().is_some();
        }

        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            false
        }
    }

    // The kernels are not bit-identical to each other (see file header), so
    // this only catches gross transcription bugs (wrong index, swapped sign,
    // transposed loop). Bit-exactness against real OpenEXR output is covered
    // end-to-end by tests/avx2.rs.
    fn assert_close_to_scalar_reference(kernel: impl Fn(&mut [f32; 64])) {
        for mut expected in pseudo_random_blocks(64) {
            let mut actual = expected;
            dct_inverse_8x8_scalar(&mut expected);
            kernel(&mut actual);

            for (e, a) in expected.iter().zip(actual.iter()) {
                let tolerance = 1e-2 * e.abs().max(1.0);
                assert!(
                    (e - a).abs() <= tolerance,
                    "expected {e}, got {a} (diff {})",
                    (e - a).abs()
                );
            }
        }
    }

    fn assert_forward_close_to_scalar_reference(kernel: impl Fn(&mut [f32; 64])) {
        for mut expected in pseudo_random_blocks(64) {
            let mut actual = expected;
            dct_forward_8x8_scalar(&mut expected);
            kernel(&mut actual);

            for (e, a) in expected.iter().zip(actual.iter()) {
                let tolerance = 1e-2 * e.abs().max(1.0);
                assert!(
                    (e - a).abs() <= tolerance,
                    "expected {e}, got {a} (diff {})",
                    (e - a).abs()
                );
            }
        }
    }

    #[cfg(feature = "avx2-tests")]
    pub fn assert_avx2_available() {
        assert!(
            has_avx2_tier(),
            "AVX2 SIMD test requested, but the AVX2/FMA tier is unavailable; selected tier: {:?}",
            selected_simd_tier()
        );
    }

    #[cfg(feature = "avx2-tests")]
    pub fn assert_avx2_close_to_scalar_reference() {
        assert_avx2_available();

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v3 = pulp::x86::V3::try_new().expect("AVX2 tier checked above");
            assert_close_to_scalar_reference(|data| avx::dct_inverse_8x8(v3, data));
        }
    }

    #[cfg(feature = "avx2-tests")]
    pub fn assert_avx2_forward_close_to_scalar_reference() {
        assert_avx2_available();

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v3 = pulp::x86::V3::try_new().expect("AVX2 tier checked above");
            assert_forward_close_to_scalar_reference(|data| forward::dct_forward_8x8(v3, data));
        }
    }

    #[cfg(feature = "avx2-tests")]
    pub fn assert_dispatch_picks_avx2() {
        assert_avx2_available();

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v3 = pulp::x86::V3::try_new().expect("AVX2 tier checked above");
            for mut expected in pseudo_random_blocks(16) {
                let mut actual = expected;
                avx::dct_inverse_8x8(v3, &mut expected);
                dct_inverse_8x8(&mut actual);
                assert_eq!(expected, actual);
            }
        }
    }

    #[cfg(feature = "avx2-tests")]
    pub fn assert_dispatch_picks_avx2_for_forward() {
        assert_avx2_available();

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v3 = pulp::x86::V3::try_new().expect("AVX2 tier checked above");
            for mut expected in pseudo_random_blocks(16) {
                let mut actual = expected;
                forward::dct_forward_8x8(v3, &mut expected);
                dct_forward_8x8(&mut actual);
                assert_eq!(expected, actual);
            }
        }
    }

    #[cfg(feature = "sse2-tests")]
    pub fn assert_sse2_available() {
        assert!(
            has_sse2_tier(),
            "SSE2 SIMD test requested, but the SSE2 tier is unavailable; selected tier: {:?}",
            selected_simd_tier()
        );
    }

    #[cfg(feature = "sse2-tests")]
    pub fn assert_sse2_close_to_scalar_reference() {
        assert_sse2_available();

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v1 = pulp::x86::V1::try_new().expect("SSE2 tier checked above");
            assert_close_to_scalar_reference(|data| sse2::dct_inverse_8x8(v1, data));
        }
    }

    #[cfg(feature = "sse2-tests")]
    pub fn assert_sse2_forward_close_to_scalar_reference() {
        assert_sse2_available();

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v1 = pulp::x86::V1::try_new().expect("SSE2 tier checked above");
            assert_forward_close_to_scalar_reference(|data| {
                forward::dct_forward_8x8_sse2(v1, data)
            });
        }
    }

    #[cfg(feature = "sse2-tests")]
    pub fn assert_dispatch_picks_sse2_without_avx2() {
        assert_sse2_available();
        assert!(
            !has_avx2_tier(),
            "SSE2 dispatch test must run with AVX2 hidden; selected tier: {:?}",
            selected_simd_tier()
        );

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v1 = pulp::x86::V1::try_new().expect("SSE2 tier checked above");
            for mut expected in pseudo_random_blocks(16) {
                let mut actual = expected;
                sse2::dct_inverse_8x8(v1, &mut expected);
                dct_inverse_8x8(&mut actual);
                assert_eq!(expected, actual);
            }
        }
    }

    #[cfg(feature = "sse2-tests")]
    pub fn assert_dispatch_picks_sse2_without_avx2_for_forward() {
        assert_sse2_available();
        assert!(
            !has_avx2_tier(),
            "SSE2 dispatch test must run with AVX2 hidden; selected tier: {:?}",
            selected_simd_tier()
        );

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let v1 = pulp::x86::V1::try_new().expect("SSE2 tier checked above");
            for mut expected in pseudo_random_blocks(16) {
                let mut actual = expected;
                forward::dct_forward_8x8_sse2(v1, &mut expected);
                dct_forward_8x8(&mut actual);
                assert_eq!(expected, actual);
            }
        }
    }
}
