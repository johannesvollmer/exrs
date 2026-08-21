// AVX-512: 2 blocks per 512-bit register (A in lanes 0-7, B in 8-15).
// Lane-preserving butterfly = width-doubled `avx2.rs` (same coeffs). Transpose
// recombine uses AVX `permute2f128` per ymm half (`shuffle_f32x16` cannot
// express a-b-a-b; pure zmm shuffle rewrites to slow `vpermt2pd`).

use miraculix::x86::ops::{
    avx::avx::Avx,
    avx512::{avx512dq::Avx512Dq, avx512f::Avx512f},
};

/// Rebuild one half of the 8x8x2 transpose from a (tt_x, tt_{x+4}) pair by
/// applying AVX's `permute2f128_f32x8` independently to each block's 256-bit
/// half.
///
/// `IMM` is 0x20 (lo: low 128 of a, low 128 of b) or 0x31 (hi: high 128 of a,
/// high 128 of b). Result layout: for 0x20 -> a0,b0,a2,b2; for 0x31 ->
/// a1,b1,a3,b3.
#[inline(always)]
fn recombine<const IMM: i32>(
    avx: Avx,
    avx512dq: Avx512Dq,
    a: [f32; 16],
    b: [f32; 16],
) -> [f32; 16] {
    let a_lo: [f32; 8] = a[0..8].try_into().unwrap();
    let b_lo: [f32; 8] = b[0..8].try_into().unwrap();
    let a_hi = avx512dq.extract_f32x8_from_x16::<1>(a);
    let b_hi = avx512dq.extract_f32x8_from_x16::<1>(b);
    let out_lo = avx.permute2f128_f32x8::<IMM>(a_lo, b_lo);
    let out_hi = avx.permute2f128_f32x8::<IMM>(a_hi, b_hi);
    let mut wide_lo = [0.0f32; 16];
    wide_lo[0..8].copy_from_slice(&out_lo);
    avx512dq.insert_f32x8_into_x16::<1>(wide_lo, out_hi)
}

#[inline(always)]
fn transpose8x8x2(
    avx: Avx,
    avx512f: Avx512f,
    avx512dq: Avx512Dq,
    rows: [[f32; 16]; 8],
) -> [[f32; 16]; 8] {
    let t0 = avx512f.unpacklo_f32x16(rows[0], rows[1]);
    let t1 = avx512f.unpackhi_f32x16(rows[0], rows[1]);
    let t2 = avx512f.unpacklo_f32x16(rows[2], rows[3]);
    let t3 = avx512f.unpackhi_f32x16(rows[2], rows[3]);
    let t4 = avx512f.unpacklo_f32x16(rows[4], rows[5]);
    let t5 = avx512f.unpackhi_f32x16(rows[4], rows[5]);
    let t6 = avx512f.unpacklo_f32x16(rows[6], rows[7]);
    let t7 = avx512f.unpackhi_f32x16(rows[6], rows[7]);

    let tt0 = avx512f.shuffle_f32x16::<0x44>(t0, t2);
    let tt1 = avx512f.shuffle_f32x16::<0xEE>(t0, t2);
    let tt2 = avx512f.shuffle_f32x16::<0x44>(t1, t3);
    let tt3 = avx512f.shuffle_f32x16::<0xEE>(t1, t3);
    let tt4 = avx512f.shuffle_f32x16::<0x44>(t4, t6);
    let tt5 = avx512f.shuffle_f32x16::<0xEE>(t4, t6);
    let tt6 = avx512f.shuffle_f32x16::<0x44>(t5, t7);
    let tt7 = avx512f.shuffle_f32x16::<0xEE>(t5, t7);

    [
        recombine::<0x20>(avx, avx512dq, tt0, tt4),
        recombine::<0x20>(avx, avx512dq, tt1, tt5),
        recombine::<0x20>(avx, avx512dq, tt2, tt6),
        recombine::<0x20>(avx, avx512dq, tt3, tt7),
        recombine::<0x31>(avx, avx512dq, tt0, tt4),
        recombine::<0x31>(avx, avx512dq, tt1, tt5),
        recombine::<0x31>(avx, avx512dq, tt2, tt6),
        recombine::<0x31>(avx, avx512dq, tt3, tt7),
    ]
}

// Mirrors `avx2::Coefficients`, just splat to 16 lanes so the same constant
// feeds both blocks at once.
const A: f32 = 3.535536e-1;
const B: f32 = 4.903927e-1;
const C: f32 = 4.619398e-1;
const D: f32 = 4.157349e-1;
const E: f32 = 2.777855e-1;
const F: f32 = 1.913422e-1;
const G: f32 = 9.754573e-2;

pub(crate) struct Coefficients {
    a: [f32; 16],
    na: [f32; 16],
    b: [f32; 16],
    nb: [f32; 16],
    c: [f32; 16],
    nc: [f32; 16],
    d: [f32; 16],
    e: [f32; 16],
    ne: [f32; 16],
    f: [f32; 16],
    nf: [f32; 16],
    g: [f32; 16],
    ng: [f32; 16],
}

impl Coefficients {
    #[inline(always)]
    pub(crate) fn new(_avx512f: Avx512f) -> Self {
        Self {
            a: [A; 16],
            na: [-A; 16],
            b: [B; 16],
            nb: [-B; 16],
            c: [C; 16],
            nc: [-C; 16],
            d: [D; 16],
            e: [E; 16],
            ne: [-E; 16],
            f: [F; 16],
            nf: [-F; 16],
            g: [G; 16],
            ng: [-G; 16],
        }
    }
}

// Mechanical width-doubling of `avx2::row_pass` (same butterfly, f32x16
// instead of f32x8): this step is purely elementwise, so it never needs to
// know about the block-A/block-B split at all.
//
// Fusing via a mul-add FMA op was tried and reverted:
// bit-exactness broke (4096/4096 test blocks produced at least one differing
// f32 vs the plain mul/add version, single rounding vs double), and it was
// even ~2-4% slower in isolation despite ~9% fewer vector instructions.
// Root cause confirmed via `llvm-mca` (znver4/znver5 sched models): the
// fused version isn't an LLVM codegen defect -> AMD's own port-mapping data
// shows plain mul/add spreads across all 4 FP pipes (FP0-FP3, resource
// pressure 65-96% each) while `vfmadd231ps` collapses almost entirely onto
// one pipe (97% on FP1 alone), and register-dependency pressure rises
// 17%->60%. This kernel already had enough independent mul/add work to
// saturate 4 ports.
#[inline(always)]
fn row_pass(avx512f: Avx512f, coef: &Coefficients, input: [[f32; 16]; 8]) -> [[f32; 16]; 8] {
    let mul = |a, b| avx512f.mul_f32x16(a, b);
    let add = |a, b| avx512f.add_f32x16(a, b);
    let sub = |a, b| avx512f.sub_f32x16(a, b);

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

// Mechanical width-doubling of `avx2::column_pass`.
#[inline(always)]
fn column_pass(avx512f: Avx512f, coef: &Coefficients, input: [[f32; 16]; 8]) -> [[f32; 16]; 8] {
    let mul = |a, b| avx512f.mul_f32x16(a, b);
    let add = |a, b| avx512f.add_f32x16(a, b);
    let sub = |a, b| avx512f.sub_f32x16(a, b);

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

/// Pack one row of block A (lanes 0-7) and block B (lanes 8-15).
#[inline(always)]
fn load_pair(avx512dq: Avx512Dq, a: &[f32; 64], b: &[f32; 64], row: usize) -> [f32; 16] {
    let ba = row * 8;
    let va: [f32; 8] = a[ba..ba + 8].try_into().unwrap();
    let vb: [f32; 8] = b[ba..ba + 8].try_into().unwrap();
    let mut wide = [0.0f32; 16];
    wide[0..8].copy_from_slice(&va);
    avx512dq.insert_f32x8_into_x16::<1>(wide, vb)
}

#[inline(always)]
fn store_pair(
    avx512dq: Avx512Dq,
    a: &mut [f32; 64],
    b: &mut [f32; 64],
    row: usize,
    value: [f32; 16],
) {
    let ba = row * 8;
    let fa: [f32; 8] = value[0..8].try_into().unwrap();
    let fb = avx512dq.extract_f32x8_from_x16::<1>(value);
    a[ba..ba + 8].copy_from_slice(&fa);
    b[ba..ba + 8].copy_from_slice(&fb);
}

/// One 8x8 inverse DCT for each of two blocks at once. Same in-register
/// shape as `avx2::inverse_one`: load 8 rows -> transpose -> row_pass ->
/// transpose back -> column_pass -> store, just processing block A and
/// block B side by side in every step.
#[inline(always)]
pub(crate) fn inverse_pair(
    avx512f: Avx512f,
    avx: Avx,
    avx512dq: Avx512Dq,
    coef: &Coefficients,
    a: &mut [f32; 64],
    b: &mut [f32; 64],
) {
    let rows: [[f32; 16]; 8] = std::array::from_fn(|row| load_pair(avx512dq, a, b, row));
    let columns = transpose8x8x2(avx, avx512f, avx512dq, rows);
    let row_pass_out = row_pass(avx512f, coef, columns);
    let intermediate_rows = transpose8x8x2(avx, avx512f, avx512dq, row_pass_out);
    let columns_out = column_pass(avx512f, coef, intermediate_rows);
    for (row, result) in columns_out.iter().enumerate() {
        store_pair(avx512dq, a, b, row, *result);
    }
}

miraculix::avx512_fn! {
    #[cfg(any(feature = "avx512-tests", feature = "simd-benches"))]
    pub fn dct_inverse_8x8_pair(
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
        a: &mut [f32; 64],
        b: &mut [f32; 64],
    ) {
        let coef = Coefficients::new(avx512f);
        inverse_pair(avx512f, avx, avx512dq, &coef, a, b);
    }
}

// Batched inverse DCT: processes blocks 2 at a time through the AVX-512
// kernel; a trailing odd block (if `blocks` has an odd length) falls back to
// the scalar autovectorized kernel -> this is a Stage-1 prototype for
// benchmarking/correctness only, not yet wired into the real dispatch chain.
//
// Wrapped in `miraculix::avx512_fn!` (not a plain function): `inverse_pair`
// alone composes 2 `transpose8x8x2`s (8 unpack + 8 shuffle + 8 `recombine`
// each, `recombine` itself being 2 extracts + 2 permutes + 1 insert) plus
// `row_pass`/`column_pass`'s ~40 mul/add/sub: needs a shared
// `#[target_feature]` context or LLVM refuses to inline any of it, leaving
// real function calls where vector instructions belong. Confirmed via
// `llvm-objdump` during the port: unwrapped, this compiled to hundreds of
// `callq`s into individual `miraculix::x86::ops::avx512::avx512f::
// mulps`/`addps` with huge stack-probe preludes and zero `zmm`
// instructions. A closure-based `.vectorize_with_avx_dq()` trampoline was
// tried first: it fixed the smaller AVX2 case but *not* this one (LLVM's
// inliner declined to fold the closure into the trampoline for a body this
// size): see `miraculix::x86::fn_macros`' module doc.
miraculix::avx512_fn! {
    pub fn dct_inverse_8x8_batch<'a>(
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
        blocks: impl Iterator<Item = &'a mut [f32; 64]>,
    ) {
        let coef = Coefficients::new(avx512f);
        let mut iter = blocks;
        loop {
            let Some(first) = iter.next() else { break };
            match iter.next() {
                Some(second) => inverse_pair(avx512f, avx, avx512dq, &coef, first, second),
                None => {
                    super::super::dct_inverse_8x8_autovectorized(first);
                    break;
                }
            }
        }
    }
}

/// Two independent `inverse_pair` chains (4 spatial blocks, 2 zmm registers),
/// stage-interleaved by hand: both chains' transpose runs before either
/// chain's row_pass, etc. Zen5 has two full-width 512-bit execution ports; a
/// single `inverse_pair` chain is a strict transpose -> row_pass -> transpose
/// -> column_pass dependency chain with nothing else in flight to fill the
/// second port while each stage's latency drains. Placing a second,
/// data-independent chain's same-stage instructions immediately adjacent in
/// the source gives the scheduler two ready, port-fillable instruction
/// streams instead of one, a source-level microbenchmark-only prototype,
/// not wired into any dispatch path. See `dct_inverse_bench_avx512_batch_quad`
/// in `benches/dct.rs` for the A/B against back-to-back `inverse_pair` calls
/// (which is what today's `dct_inverse_8x8_batch` loop already produces, one
/// pair per iteration, whatever ILP LLVM's own loop unrolling finds on its
/// own).
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn inverse_quad(
    avx512f: Avx512f,
    avx: Avx,
    avx512dq: Avx512Dq,
    coef: &Coefficients,
    a0: &mut [f32; 64],
    b0: &mut [f32; 64],
    a1: &mut [f32; 64],
    b1: &mut [f32; 64],
) {
    let rows0: [[f32; 16]; 8] = std::array::from_fn(|row| load_pair(avx512dq, a0, b0, row));
    let rows1: [[f32; 16]; 8] = std::array::from_fn(|row| load_pair(avx512dq, a1, b1, row));

    let columns0 = transpose8x8x2(avx, avx512f, avx512dq, rows0);
    let columns1 = transpose8x8x2(avx, avx512f, avx512dq, rows1);

    let row_pass_out0 = row_pass(avx512f, coef, columns0);
    let row_pass_out1 = row_pass(avx512f, coef, columns1);

    let intermediate_rows0 = transpose8x8x2(avx, avx512f, avx512dq, row_pass_out0);
    let intermediate_rows1 = transpose8x8x2(avx, avx512f, avx512dq, row_pass_out1);

    let columns_out0 = column_pass(avx512f, coef, intermediate_rows0);
    let columns_out1 = column_pass(avx512f, coef, intermediate_rows1);

    for (row, result) in columns_out0.iter().enumerate() {
        store_pair(avx512dq, a0, b0, row, *result);
    }
    for (row, result) in columns_out1.iter().enumerate() {
        store_pair(avx512dq, a1, b1, row, *result);
    }
}

miraculix::avx512_fn! {
    #[cfg(any(feature = "avx512-tests", feature = "simd-benches"))]
    #[allow(clippy::too_many_arguments)]
    pub fn dct_inverse_8x8_quad(
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
        a0: &mut [f32; 64],
        b0: &mut [f32; 64],
        a1: &mut [f32; 64],
        b1: &mut [f32; 64],
    ) {
        let coef = Coefficients::new(avx512f);
        inverse_quad(avx512f, avx, avx512dq, &coef, a0, b0, a1, b1);
    }
}

// Same batching contract as `dct_inverse_8x8_batch`, but processes 4
// blocks (2 pairs) per iteration through the hand-interleaved
// `inverse_quad` kernel; a short trailing remainder (1-3 blocks) falls
// back to `inverse_pair`/scalar. Wrapped in `miraculix::avx512_fn!`, same
// reasoning as `dct_inverse_8x8_batch`.
miraculix::avx512_fn! {
    pub fn dct_inverse_8x8_batch_quad<'a>(
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
        blocks: impl Iterator<Item = &'a mut [f32; 64]>,
    ) {
        let coef = Coefficients::new(avx512f);
        let mut iter = blocks;
        loop {
            let Some(a0) = iter.next() else { break };
            let Some(b0) = iter.next() else {
                super::super::dct_inverse_8x8_autovectorized(a0);
                break;
            };
            let Some(a1) = iter.next() else {
                inverse_pair(avx512f, avx, avx512dq, &coef, a0, b0);
                break;
            };
            let Some(b1) = iter.next() else {
                inverse_pair(avx512f, avx, avx512dq, &coef, a0, b0);
                super::super::dct_inverse_8x8_autovectorized(a1);
                break;
            };
            inverse_quad(avx512f, avx, avx512dq, &coef, a0, b0, a1, b1);
        }
    }
}

/// One RGB spatial pair as held by the fused AVX-512 decode step: two
/// blocks x three components. Microbench / test fixture type only.
pub type RgbPairBlocks = ([[f32; 64]; 3], [[f32; 64]; 3]);

// Baseline shape of today's `decode_pair_dct_csc` DCT loop: three sequential
// `inverse_pair` calls (R, then G, then B) on one spatial pair. No zigzag /
// CSC / write, pure DCT middle step, RGB-shaped so the component-quad A/B
// below measures the dual-port idea *without* growing the spatial working set.
miraculix::avx512_fn! {
    #[cfg(any(feature = "avx512-tests", feature = "simd-benches"))]
    pub fn dct_inverse_rgb_pair_components_seq(
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
        pairs: &mut [RgbPairBlocks],
    ) {
        let coef = Coefficients::new(avx512f);
        for (a, b) in pairs.iter_mut() {
            for c in 0..3 {
                inverse_pair(avx512f, avx, avx512dq, &coef, &mut a[c], &mut b[c]);
            }
        }
    }
}

// Dual-port candidate for the same RGB pair: hand-interleave R and G through
// `inverse_quad` (two independent 512-bit chains, same spatial pair, buffers
// already L1-resident in the fused path), then B alone via `inverse_pair`.
// Same total arithmetic as `dct_inverse_rgb_pair_components_seq`; only the
// issue order changes.
miraculix::avx512_fn! {
    #[cfg(any(feature = "avx512-tests", feature = "simd-benches"))]
    pub fn dct_inverse_rgb_pair_components_quad(
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
        pairs: &mut [RgbPairBlocks],
    ) {
        let coef = Coefficients::new(avx512f);
        for (a, b) in pairs.iter_mut() {
            // split_at_mut: disjoint R/G mut refs (indexing alone won't borrow-check).
            let (a_r, a_rest) = a.split_at_mut(1);
            let (a_g, a_b) = a_rest.split_at_mut(1);
            let (b_r, b_rest) = b.split_at_mut(1);
            let (b_g, b_b) = b_rest.split_at_mut(1);
            inverse_quad(
                avx512f,
                avx,
                avx512dq,
                &coef,
                &mut a_r[0],
                &mut b_r[0],
                &mut a_g[0],
                &mut b_g[0],
            );
            inverse_pair(avx512f, avx, avx512dq, &coef, &mut a_b[0], &mut b_b[0]);
        }
    }
}
