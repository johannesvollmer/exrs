#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

#[macro_use]
extern crate bencher;

extern crate exr;

use bencher::Bencher;
use exr::compression::dwa::discrete_cosine_transform::{x86::*, *};
use miraculix::x86::{
    detect_features,
    ops::{
        avx::avx::Avx,
        avx512::{avx512dq::Avx512Dq, avx512f::Avx512f},
        sse::sse::Sse,
    },
};

fn dct_forward_bench_autovectorized(bench: &mut Bencher) {
    let mut blocks = bench_blocks();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_forward_8x8_autovectorized(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn dct_forward_bench_sse2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let sse = expect_sse();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            sse::dct_forward_8x8(sse, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn dct_forward_bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            avx::dct_forward_8x8(avx, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn dct_forward_bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        avx::dct_forward_8x8_batch(avx, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

fn dct_inverse_bench_autovectorized(bench: &mut Bencher) {
    let mut blocks = bench_blocks();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_inverse_8x8_autovectorized(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn dct_inverse_bench_sse2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let sse = expect_sse();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            sse::dct_inverse_8x8(sse, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn dct_inverse_bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            avx::dct_inverse_8x8(avx, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn dct_inverse_bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        avx::dct_inverse_8x8_batch(avx, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

fn dct_inverse_bench_avx512_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let (avx512f, avx, avx512dq) = expect_avx512();

    bench.iter(|| {
        avx512dq::dct_inverse_8x8_batch(avx512f, avx, avx512dq, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

/// A/B against `dct_inverse_bench_avx512_batch`: same 1-pair-per-iteration
/// loop, but relies solely on LLVM's own loop unrolling for any ILP across
/// iterations -> see `avx512dq::inverse_quad`'s doc comment for what "quad"
/// changes.
fn dct_inverse_bench_avx512_batch_quad(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let (avx512f, avx, avx512dq) = expect_avx512();

    bench.iter(|| {
        avx512dq::dct_inverse_8x8_batch_quad(avx512f, avx, avx512dq, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

/// RGB-shaped baseline: 3 sequential `inverse_pair`s per spatial pair
/// (mirrors `decode_pair_dct_csc`'s DCT loop). A/B partner is
/// `dct_inverse_bench_avx512_rgb_comp_quad`.
fn dct_inverse_bench_avx512_rgb_comp_seq(bench: &mut Bencher) {
    let mut pairs = bench_rgb_pairs();
    let (avx512f, avx, avx512dq) = expect_avx512();

    bench.iter(|| {
        avx512dq::dct_inverse_rgb_pair_components_seq(avx512f, avx, avx512dq, &mut pairs);
        bencher::black_box(&mut pairs);
    })
}

/// RGB-shaped dual-port candidate: `inverse_quad(R,G)` + `inverse_pair(B)`.
/// Same buffers / arithmetic as `_rgb_comp_seq`; only issue order changes.
/// No extra spatial working set vs the fused pair path.
fn dct_inverse_bench_avx512_rgb_comp_quad(bench: &mut Bencher) {
    let mut pairs = bench_rgb_pairs();
    let (avx512f, avx, avx512dq) = expect_avx512();

    bench.iter(|| {
        avx512dq::dct_inverse_rgb_pair_components_quad(avx512f, avx, avx512dq, &mut pairs);
        bencher::black_box(&mut pairs);
    })
}

fn bench_blocks() -> Vec<[f32; 64]> {
    test::pseudo_random_blocks(4096)
}

/// 1024 RGB spatial pairs = 6144 blocks of work, same order of magnitude as
/// the flat 4096-block inverse benches (those process 4096 single blocks;
/// each RGB pair does 6).
fn bench_rgb_pairs() -> Vec<avx512dq::RgbPairBlocks> {
    let blocks = test::pseudo_random_blocks(1024 * 6);
    blocks.chunks_exact(6).map(|c| ([c[0], c[1], c[2]], [c[3], c[4], c[5]])).collect()
}

fn expect_avx() -> Avx {
    Avx::from_features(detect_features()).expect("AVX SIMD mode requested, but AVX is unavailable")
}

fn expect_sse() -> Sse {
    Sse::from_features(detect_features()).expect("SSE SIMD mode requested, but SSE is unavailable")
}

fn expect_avx512() -> (Avx512f, Avx, Avx512Dq) {
    let features = detect_features();
    (
        Avx512f::from_features(features)
            .expect("AVX-512 SIMD mode requested, but AVX-512F is unavailable"),
        Avx::from_features(features).expect("AVX-512 SIMD mode requested, but AVX is unavailable"),
        Avx512Dq::from_features(features)
            .expect("AVX-512 SIMD mode requested, but AVX-512DQ is unavailable"),
    )
}

benchmark_group!(
    dct,
    dct_forward_bench_autovectorized,
    dct_forward_bench_sse2,
    dct_forward_bench_avx2,
    dct_forward_bench_avx2_batch,
    dct_inverse_bench_autovectorized,
    dct_inverse_bench_sse2,
    dct_inverse_bench_avx2,
    dct_inverse_bench_avx2_batch,
    dct_inverse_bench_avx512_batch,
    dct_inverse_bench_avx512_batch_quad,
    dct_inverse_bench_avx512_rgb_comp_seq,
    dct_inverse_bench_avx512_rgb_comp_quad
);

benchmark_main!(dct);
