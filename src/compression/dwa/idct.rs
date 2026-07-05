// Inverse DCT for DWA, ported from OpenEXRCore's internal_dwa_simd.h
// including its runtime CPU dispatch: `dct_inverse_8x8` picks the best
// available x86 tier at runtime (avx2 > sse2 > scalar), like OpenEXR
// cpuid-based `initializeFuncs`, and uses scalar on non-x86 or fallback
//
// Dispatch uses pulp V3 and V1 tokens only construct after a cpuid
// check and their methods carry the matching #[target_feature], giving
// real runtime dispatch in 100% safe rust.
// pulps V3 needs AVX2+FMA, so AVX-only CPUs (Sandy/Ivy Bridge) fallback on sse2
// where OpenEXR would use its avx kernel.
//
// The three kernels are not bit-identical to each other (OpenEXRs own
// kernels disagree too: basis-constant precision and summation order
// differ), but each is bit-identical to OpenEXRs counterpart.

// AVX2 V3 tier: OpenEXRs "dctInverse8x8_avx_0". Each pass runs all 8
// rows/columns of the block in parallel, one 8-wide register per position.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx {
    use pulp::{ f32x8, x86::V3 };

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
    fn row_pass(v3: V3, coef: &Coefficients, input: [f32x8; 8]) -> [f32x8; 8] {
        let mul = |a, b| v3.mul_f32x8(a, b);
        let add = |a, b| v3.add_f32x8(a, b);
        let sub = |a, b| v3.sub_f32x8(a, b);

        let (in0, in2, in4, in6) = (input[0], input[2], input[4], input[6]);
        let (in1, in3, in5, in7) = (input[1], input[3], input[5], input[7]);

        let even0 = add(
            add(mul(in4, coef.a), mul(in6, coef.f)),
            add(mul(in0, coef.a), mul(in2, coef.c))
        );
        let even1 = add(
            add(mul(in4, coef.na), mul(in6, coef.nc)),
            add(mul(in0, coef.a), mul(in2, coef.f))
        );
        let even2 = add(
            add(mul(in4, coef.na), mul(in6, coef.c)),
            add(mul(in0, coef.a), mul(in2, coef.nf))
        );
        let even3 = add(
            add(mul(in4, coef.a), mul(in6, coef.nf)),
            add(mul(in0, coef.a), mul(in2, coef.nc))
        );

        let odd0 = add(
            add(mul(in5, coef.e), mul(in7, coef.g)),
            add(mul(in1, coef.b), mul(in3, coef.d))
        );
        let odd1 = add(
            add(mul(in5, coef.nb), mul(in7, coef.ne)),
            add(mul(in1, coef.d), mul(in3, coef.ng))
        );
        let odd2 = add(
            add(mul(in5, coef.g), mul(in7, coef.d)),
            add(mul(in1, coef.e), mul(in3, coef.nb))
        );
        let odd3 = add(
            add(mul(in5, coef.d), mul(in7, coef.nb)),
            add(mul(in1, coef.g), mul(in3, coef.ne))
        );

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
    fn column_pass(v3: V3, coef: &Coefficients, input: [f32x8; 8]) -> [f32x8; 8] {
        let mul = |a, b| v3.mul_f32x8(a, b);
        let add = |a, b| v3.add_f32x8(a, b);
        let sub = |a, b| v3.sub_f32x8(a, b);

        let (in0, in1, in2, in3, in4, in5, in6, in7) = (
            input[0],
            input[1],
            input[2],
            input[3],
            input[4],
            input[5],
            input[6],
            input[7],
        );

        let beta0 = add(
            add(mul(coef.g, in7), mul(coef.e, in5)),
            add(mul(coef.d, in3), mul(coef.b, in1))
        );
        let beta1 = sub(
            sub(mul(coef.d, in1), add(mul(coef.b, in5), mul(coef.g, in3))),
            mul(coef.e, in7)
        );
        let beta2 = add(
            mul(coef.d, in7),
            add(mul(coef.g, in5), sub(mul(coef.e, in1), mul(coef.b, in3)))
        );
        let beta3 = sub(
            add(mul(coef.d, in5), mul(coef.g, in1)),
            add(mul(coef.b, in7), mul(coef.e, in3))
        );

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

    pub fn dct_inverse_8x8(v3: V3, data: &mut [f32; 64]) {
        let coef = Coefficients::new(v3);

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
                data[56 + k]
            )
        });

        let rows_out = row_pass(v3, &coef, columns);
        for (column, result) in rows_out.iter().enumerate() {
            let r = [
                result.0,
                result.1,
                result.2,
                result.3,
                result.4,
                result.5,
                result.6,
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
                data[b + 7]
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
}

// SSE2 V1 tier: OpenEXRs "dctInverse8x8_sse2". Vectorizes 4
// output positions of one row at a time a different shape than "avx",
// so the two are not bit-identical.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod sse2 {
    use pulp::{ f32x4, x86::V1 };

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

        let (in0, in1, in2, in3, in4, in5, in6, in7) = (
            input[0],
            input[1],
            input[2],
            input[3],
            input[4],
            input[5],
            input[6],
            input[7],
        );

        let beta0 = add(
            add(mul(in1, coef.b), mul(in3, coef.d)),
            add(mul(in5, coef.e), mul(in7, coef.g))
        );
        let beta1 = sub(
            sub(mul(in1, coef.d), mul(in3, coef.g)),
            add(mul(in5, coef.b), mul(in7, coef.e))
        );
        let beta2 = add(
            sub(mul(in1, coef.e), mul(in3, coef.b)),
            add(mul(in5, coef.g), mul(in7, coef.d))
        );
        let beta3 = add(
            sub(mul(in1, coef.g), mul(in3, coef.e)),
            sub(mul(in5, coef.d), mul(in7, coef.b))
        );

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

/// Inverse DCT on an 8x8 block (in-place, row-major), dispatched at
/// runtime to the best available x86 SIMD tier (avx2 > sse2 > scalar),
/// like a real OpenEXR build. See the file header comment.
pub fn dct_inverse_8x8(data: &mut [f32; 64]) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        use pulp::x86::{ V1, V3 };

        if let Some(v3) = V3::try_new() {
            avx::dct_inverse_8x8(v3, data);
            return;
        }
        if let Some(v1) = V1::try_new() {
            sse2::dct_inverse_8x8(v1, data);
            return;
        }
    }

    dct_inverse_8x8_scalar(data);
}

/// Optimized path when only DC is non-zero.
pub fn dct_inverse_8x8_dc_only(data: &mut [f32; 64]) {
    let val = data[0] * 0.3535536f32 * 0.3535536f32;
    for v in data.iter_mut() {
        *v = val;
    }
}

// All tests exercise the SIMD kernels; only on x86.
#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;

    // Deterministic blocks in the ballpark of half-precision DCT
    // coefficients (xorshift64, no `rand` dependency in the lib target).
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

    // The kernels are not bit-identical to each other (see file header), so
    // this only catches gross transcription bugs (wrong index, swapped sign,
    // transposed loop). Bit-exactness against real OpenEXR output is covered
    // end-to-end by tests/dwa_csc.rs.
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

    #[test]
    fn avx_is_close_to_scalar_reference() {
        let Some(v3) = pulp::x86::V3::try_new() else {
            return; // CPU can't run this kernel, nothing to test
        };
        assert_close_to_scalar_reference(|data| avx::dct_inverse_8x8(v3, data));
    }

    #[test]
    fn sse2_is_close_to_scalar_reference() {
        let Some(v1) = pulp::x86::V1::try_new() else {
            return; // CPU can't run this kernel, nothing to test
        };
        assert_close_to_scalar_reference(|data| sse2::dct_inverse_8x8(v1, data));
    }

    // The dispatch must pick the avx tier on an AVX2-capable machine,
    // not silently fall further down the hierarchy.
    #[test]
    fn dispatch_picks_avx_when_available() {
        let Some(v3) = pulp::x86::V3::try_new() else {
            return; // CPU can't run this kernel, nothing to test
        };

        for mut expected in pseudo_random_blocks(16) {
            let mut actual = expected;
            avx::dct_inverse_8x8(v3, &mut expected);
            dct_inverse_8x8(&mut actual);
            assert_eq!(expected, actual);
        }
    }
}
