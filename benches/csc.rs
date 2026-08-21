#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

#[macro_use]
extern crate bencher;

extern crate exr;

use bencher::Bencher;
use exr::compression::dwa::color_space_conversion::{x86::*, *};
use miraculix::x86::{
    detect_features,
    ops::{avx::avx::Avx, avx512::avx512f::Avx512f, sse::sse::Sse},
};

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
    let sse = expect_sse();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            sse::csc709_forward_8x8(sse, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_forward_bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            avx::csc709_forward_8x8(avx, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_forward_bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        avx::csc709_forward_8x8_batch(avx, blocks.iter_mut());

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
    let sse = expect_sse();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            sse::csc709_inverse_8x8(sse, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_inverse_bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            avx::csc709_inverse_8x8(avx, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn csc_inverse_bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx = expect_avx();

    bench.iter(|| {
        avx::csc709_inverse_8x8_batch(avx, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

fn csc_inverse_bench_avx512_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    let avx512f = expect_avx512f();

    bench.iter(|| {
        avx512f::csc709_inverse_8x8_batch(avx512f, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

fn bench_blocks() -> Vec<[[f32; 64]; 3]> {
    test::pseudo_random_triplets(4096)
}

fn expect_avx() -> Avx {
    Avx::from_features(detect_features()).expect("AVX SIMD mode requested, but AVX is unavailable")
}

fn expect_sse() -> Sse {
    Sse::from_features(detect_features()).expect("SSE SIMD mode requested, but SSE is unavailable")
}

fn expect_avx512f() -> Avx512f {
    Avx512f::from_features(detect_features())
        .expect("AVX-512 SIMD mode requested, but AVX-512F is unavailable")
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
    csc_inverse_bench_avx2_batch,
    csc_inverse_bench_avx512_batch
);

benchmark_main!(csc);
