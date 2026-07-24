#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

#[macro_use]
extern crate bencher;

extern crate exr;

use bencher::Bencher;
use exr::compression::dwa::color_space_conversion::{x86::*, *};
use pulp::x86::{V1, V3};

fn csc_forward_bench_autovectorized(bench: &mut Bencher) {
    let mut blocks = bench_blocks();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            csc709_forward_8x8_autovectorized(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_forward_bench_sse2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let v1 = expect_sse2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            sse2::csc709_forward_8x8(v1, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_forward_bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let v3 = expect_avx2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            avx2::csc709_forward_8x8(v3, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_forward_bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let v3 = expect_avx2();

    bench.iter(|| {
        avx2::csc709_forward_8x8_batch(v3, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

fn csc_inverse_bench_autovectorized(bench: &mut Bencher) {
    let mut blocks = bench_blocks();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            csc709_inverse_8x8_autovectorized(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_inverse_bench_sse2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let v1 = expect_sse2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            sse2::csc709_inverse_8x8(v1, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_inverse_bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let v3 = expect_avx2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            avx2::csc709_inverse_8x8(v3, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_inverse_bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let v3 = expect_avx2();

    bench.iter(|| {
        avx2::csc709_inverse_8x8_batch(v3, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

fn bench_blocks() -> Vec<[[f32; 64]; 3]> {
    test::pseudo_random_triplets(4096)
}

fn expect_avx2() -> V3 {
    V3::try_new().expect("AVX2 SIMD mode requested, but the AVX2/FMA tier is unavailable")
}

fn expect_sse2() -> V1 {
    V1::try_new().expect("SSE2 SIMD mode requested, but the SSE2 tier is unavailable")
}

benchmark_group!(
    csc,
    csc_forward_bench_autovectorized,
    csc_forward_bench_sse2,
    csc_forward_bench_avx2,
    csc_forward_bench_avx2_batch,
    csc_inverse_bench_autovectorized,
    csc_inverse_bench_sse2,
    csc_inverse_bench_avx2,
    csc_inverse_bench_avx2_batch
);

benchmark_main!(csc);
