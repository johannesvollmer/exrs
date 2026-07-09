// Compares the DWA inverse-DCT SIMD kernels against each other and against
// the scalar fallback, each called directly (bypassing runtime dispatch), so
// the numbers reflect one specific tier rather than whatever `dct_inverse_8x8`
// happens to pick on the machine running the benchmark.
//
// AVX2 and SSE2 timings are only meaningful on real hardware

#[macro_use]
extern crate bencher;

extern crate exr;

use bencher::Bencher;
use exr::compression::simd_bench_support::{
    bench_blocks, dct_inverse_8x8_forced_avx2, dct_inverse_8x8_forced_avx2_batch,
    dct_inverse_8x8_forced_scalar, dct_inverse_8x8_forced_sse2, expect_avx2, expect_sse2,
};

const BLOCK_COUNT: usize = 4096;

fn bench_scalar(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_inverse_8x8_forced_scalar(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_sse2(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);
    let v1 = expect_sse2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_inverse_8x8_forced_sse2(v1, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);
    let v3 = expect_avx2();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_inverse_8x8_forced_avx2(v3, block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);
    let v3 = expect_avx2();

    bench.iter(|| {
        dct_inverse_8x8_forced_avx2_batch(v3, blocks.iter_mut());

        bencher::black_box(&mut blocks);
    })
}

benchmark_group!(idct, bench_scalar, bench_sse2, bench_avx2, bench_avx2_batch);
benchmark_main!(idct);
