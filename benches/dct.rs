#[macro_use]
extern crate bencher;

extern crate exr;

use bencher::Bencher;
use exr::compression::simd_bench_support::{
    bench_blocks, dct_forward_8x8_forced_avx2, dct_forward_8x8_forced_avx2_batch,
    dct_forward_8x8_forced_scalar, dct_forward_8x8_forced_sse2, expect_avx2, expect_sse2,
};

const BLOCK_COUNT: usize = 4096;

fn bench_scalar(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_forward_8x8_forced_scalar(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_sse2(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);
    let v1 = expect_sse2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_forward_8x8_forced_sse2(v1, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);
    let v3 = expect_avx2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_forward_8x8_forced_avx2(v3, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);
    let v3 = expect_avx2();

    bench.iter(|| {
        dct_forward_8x8_forced_avx2_batch(v3, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

benchmark_group!(dct, bench_scalar, bench_sse2, bench_avx2, bench_avx2_batch);
benchmark_main!(dct);
