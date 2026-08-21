// AVX tier: OpenEXR `dctInverse8x8_avx_0`. All 8 rows/columns in parallel, one
// f32x8 per position. Base `Avx` only (filename keeps tier convention with
// color_space_conversion/lossy_dct).

use miraculix::x86::ops::avx::avx::Avx;

use super::super::forward_basis;

// Transposes 8 contiguous row-vectors into 8 column-vectors entirely in
// registers (unpacklo/unpackhi + shuffle + permute2f128), replacing a scalar
// strided-gather construction (8 independent `data[k], data[8+k], ...`
// scalar reads per lane). Measured standalone (not in this crate) at ~32%
// faster to build and ~25% faster once fused with the row_pass math that
// consumes it -> LLVM compiles the scalar-gather construction into a chain of
// `vinsertps`-per-scalar instructions rather than a handful of wide shuffles.
#[inline(always)]
fn transpose8x8(avx: Avx, rows: [[f32; 8]; 8]) -> [[f32; 8]; 8] {
    let t0 = avx.unpacklo_f32x8(rows[0], rows[1]);
    let t1 = avx.unpackhi_f32x8(rows[0], rows[1]);
    let t2 = avx.unpacklo_f32x8(rows[2], rows[3]);
    let t3 = avx.unpackhi_f32x8(rows[2], rows[3]);
    let t4 = avx.unpacklo_f32x8(rows[4], rows[5]);
    let t5 = avx.unpackhi_f32x8(rows[4], rows[5]);
    let t6 = avx.unpacklo_f32x8(rows[6], rows[7]);
    let t7 = avx.unpackhi_f32x8(rows[6], rows[7]);

    let tt0 = avx.shuffle_f32x8::<0x44>(t0, t2);
    let tt1 = avx.shuffle_f32x8::<0xEE>(t0, t2);
    let tt2 = avx.shuffle_f32x8::<0x44>(t1, t3);
    let tt3 = avx.shuffle_f32x8::<0xEE>(t1, t3);
    let tt4 = avx.shuffle_f32x8::<0x44>(t4, t6);
    let tt5 = avx.shuffle_f32x8::<0xEE>(t4, t6);
    let tt6 = avx.shuffle_f32x8::<0x44>(t5, t7);
    let tt7 = avx.shuffle_f32x8::<0xEE>(t5, t7);

    [
        avx.permute2f128_f32x8::<0x20>(tt0, tt4),
        avx.permute2f128_f32x8::<0x20>(tt1, tt5),
        avx.permute2f128_f32x8::<0x20>(tt2, tt6),
        avx.permute2f128_f32x8::<0x20>(tt3, tt7),
        avx.permute2f128_f32x8::<0x31>(tt0, tt4),
        avx.permute2f128_f32x8::<0x31>(tt1, tt5),
        avx.permute2f128_f32x8::<0x31>(tt2, tt6),
        avx.permute2f128_f32x8::<0x31>(tt3, tt7),
    ]
}

// OpenEXRs hardcoded AVX basis constants ("sAvxCoef").
const A: f32 = 3.535536e-1;
const B: f32 = 4.903927e-1;
const C: f32 = 4.619398e-1;
const D: f32 = 4.157349e-1;
const E: f32 = 2.777855e-1;
const F: f32 = 1.913422e-1;
const G: f32 = 9.754573e-2;

// Public to the crate so the fused lossy-DCT decode path can build the
// constants once per block loop and reuse them across every block's
// in-register iDCT (see `inverse_one`).
pub(crate) struct Coefficients {
    a: [f32; 8],
    na: [f32; 8],
    b: [f32; 8],
    nb: [f32; 8],
    c: [f32; 8],
    nc: [f32; 8],
    d: [f32; 8],
    // no "nd": the AVX never multiplies by -D
    e: [f32; 8],
    ne: [f32; 8],
    f: [f32; 8],
    nf: [f32; 8],
    g: [f32; 8],
    ng: [f32; 8],
}

impl Coefficients {
    #[inline(always)]
    pub(crate) fn new(_avx: Avx) -> Self {
        // Negated splats are exact (sign flip), so "x * na == -(x * a)".
        Self {
            a: [A; 8],
            na: [-A; 8],
            b: [B; 8],
            nb: [-B; 8],
            c: [C; 8],
            nc: [-C; 8],
            d: [D; 8],
            e: [E; 8],
            ne: [-E; 8],
            f: [F; 8],
            nf: [-F; 8],
            g: [G; 8],
            ng: [-G; 8],
        }
    }
}

// OpenEXRs "IDCT_AVX_MMULT_ROWS" + "EO_TO_ROW_HALVES"
#[inline(always)]
fn row_pass(avx: Avx, coef: &Coefficients, input: [[f32; 8]; 8]) -> [[f32; 8]; 8] {
    let mul = |a, b| avx.mul_f32x8(a, b);
    let add = |a, b| avx.add_f32x8(a, b);
    let sub = |a, b| avx.sub_f32x8(a, b);

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
    let odd1 =
        add(add(mul(in5, coef.nb), mul(in7, coef.ne)), add(mul(in1, coef.d), mul(in3, coef.ng)));
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
#[inline(always)]
fn column_pass(avx: Avx, coef: &Coefficients, input: [[f32; 8]; 8]) -> [[f32; 8]; 8] {
    let mul = |a, b| avx.mul_f32x8(a, b);
    let add = |a, b| avx.add_f32x8(a, b);
    let sub = |a, b| avx.sub_f32x8(a, b);

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
pub fn dct_inverse_8x8(avx: Avx, data: &mut [f32; 64]) {
    dct_inverse_8x8_batch(avx, std::iter::once(data));
}

// One 8x8 inverse DCT.
//
// Full iDCT stays in registers: load 8 rows -> transpose to the column-major
// shape row_pass wants -> row_pass -> transpose back to rows for column_pass
// -> column_pass -> store. Scatter-storing the row-pass result and reloading
// it was pure intermediate L1 traffic for a 256-byte block that already
// fits in registers.
#[inline(always)]
pub(crate) fn inverse_one(avx: Avx, coef: &Coefficients, data: &mut [f32; 64]) {
    let rows: [[f32; 8]; 8] = std::array::from_fn(|row| {
        let b = row * 8;
        [
            data[b],
            data[b + 1],
            data[b + 2],
            data[b + 3],
            data[b + 4],
            data[b + 5],
            data[b + 6],
            data[b + 7],
        ]
    });
    let columns = transpose8x8(avx, rows);
    let row_pass_out = row_pass(avx, coef, columns);
    // row_pass_out[col].lane[row] = intermediate[row][col]; the 8x8
    // transpose is an involution, so one more pass yields
    // intermediate_rows[row].lane[col] for column_pass.
    let intermediate_rows = transpose8x8(avx, row_pass_out);
    let columns_out = column_pass(avx, coef, intermediate_rows);
    for (row, result) in columns_out.iter().enumerate() {
        let b = row * 8;
        data[b..b + 8].copy_from_slice(result);
    }
}

// Wrapped in `miraculix::avx_fn!` (not a plain function): `inverse_one`
// alone composes 2 `transpose8x8`s (8 unpack + 8 shuffle + 8 permute2f128
// each) plus `row_pass`/`column_pass`'s ~40 mul/add/sub: needs a shared
// `#[target_feature]` context or LLVM refuses to inline any of it into the
// caller, leaving real function calls (with a loadu/storeu round trip per
// call) where vector instructions belong. Confirmed via `llvm-objdump`
// during the port: without this, the loop compiled to hundreds of `callq`s
// into individual `miraculix::x86::ops::avx::avx::mulps`/`addps` and zero
// `ymm` instructions; wrapped, it inlines into the real transpose/butterfly.
// A closure-based `.vectorize()` trampoline was tried first and dropped --
// see `miraculix::x86::fn_macros`' module doc for why it isn't reliable.
miraculix::avx_fn! {
    pub fn dct_inverse_8x8_batch<'a>(avx: Avx, blocks: impl Iterator<Item = &'a mut [f32; 64]>) {
        let coef = Coefficients::new(avx);
        for data in blocks {
            inverse_one(avx, &coef, data);
        }
    }
}

struct ForwardCoefficients {
    terms: [[f32; 8]; 8],
}

impl ForwardCoefficients {
    #[inline(always)]
    fn new(_avx: Avx) -> Self {
        let basis = forward_basis();
        Self {
            terms: std::array::from_fn(|input| {
                let row = basis[input];
                [row[0], row[1], row[2], row[3], row[4], row[5], row[6], row[7]]
            }),
        }
    }
}

#[inline(always)]
fn forward_pass(avx: Avx, coef: &ForwardCoefficients, input: [f32; 8]) -> [f32; 8] {
    let mul = |a, b| avx.mul_f32x8(a, b);
    let add = |a, b| avx.add_f32x8(a, b);

    let mut out = [0.0f32; 8];
    for index in 0..8 {
        out = add(out, mul([input[index]; 8], coef.terms[index]));
    }
    out
}

// TODO just #[test]
#[cfg(any(feature = "avx2-tests", feature = "simd-benches"))]
pub fn dct_forward_8x8(avx: Avx, data: &mut [f32; 64]) {
    dct_forward_8x8_batch(avx, std::iter::once(data));
}

// Wrapped in `avx.vectorize`
// Wrapped in `miraculix::avx_fn!`
miraculix::avx_fn! {
    pub fn dct_forward_8x8_batch<'a>(avx: Avx, blocks: impl Iterator<Item = &'a mut [f32; 64]>) {
        let coef = ForwardCoefficients::new(avx);
        let basis = forward_basis();

        for data in blocks {
            // Row pass: each row's own 8 values are already contiguous, so
            // this needs no gather.
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
                let out = forward_pass(avx, &coef, input);
                data[base..base + 8].copy_from_slice(&out);
            }

            // Column pass: batched across all 8 columns via SIMD lanes
            // instead of gathering one column at a time with a stride-8
            // read. Each row is loaded contiguously once and fanned into 8
            // per-frequency accumulators (one lane per column), which are
            // then stored back contiguously per output row.
            let mut outputs = [[0.0f32; 8]; 8];
            for row in 0..8 {
                let base = row * 8;
                let row_vec = [
                    data[base],
                    data[base + 1],
                    data[base + 2],
                    data[base + 3],
                    data[base + 4],
                    data[base + 5],
                    data[base + 6],
                    data[base + 7],
                ];
                for v in 0..8 {
                    let coefficient = [basis[row][v]; 8];
                    outputs[v] = avx.add_f32x8(outputs[v], avx.mul_f32x8(coefficient, row_vec));
                }
            }

            for (v, out) in outputs.iter().enumerate() {
                let base = v * 8;
                data[base..base + 8].copy_from_slice(out);
            }
        }
    }
}
