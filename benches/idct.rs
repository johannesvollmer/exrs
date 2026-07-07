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
    bench_blocks,
    dct_inverse_8x8_forced_avx2,
    dct_inverse_8x8_forced_avx2_batch,
    dct_inverse_8x8_forced_scalar,
    dct_inverse_8x8_forced_sse2,
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

    if !dct_inverse_8x8_forced_sse2(&mut blocks[0]) {
        panic!("this CPU does not expose the SSE2 tier");
    }

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_inverse_8x8_forced_sse2(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_avx2(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);

    if !dct_inverse_8x8_forced_avx2(&mut blocks[0]) {
        panic!("this CPU does not expose the AVX2+FMA tier");
    }

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_inverse_8x8_forced_avx2(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_avx2_batch(bench: &mut Bencher) {
    let mut blocks = bench_blocks(BLOCK_COUNT);

    bench.iter(|| {
        if !dct_inverse_8x8_forced_avx2_batch(blocks.iter_mut()) {
            panic!("this CPU does not expose the AVX2+FMA tier");
        }

        bencher::black_box(&mut blocks);
    })
}

benchmark_group!(idct, bench_scalar, bench_sse2, bench_avx2, bench_avx2_batch);
benchmark_main!(idct);
