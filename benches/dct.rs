#[macro_use]
extern crate bencher;

extern crate exr;

use bencher::Bencher;
use exr::compression::dwa::discrete_cosine_transform::{
    dct_forward_8x8_autovectorized, dct_forward_8x8_batch, dct_inverse_8x8_autovectorized,
    dct_inverse_8x8_batch, test,
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

fn dct_inverse_bench_autovectorized(bench: &mut Bencher) {
    let mut blocks = bench_blocks();

    bench.iter(|| {
        for block in blocks.iter_mut() {
            dct_inverse_8x8_autovectorized(block);
        }

        bencher::black_box(&mut blocks);
    })
}

fn bench_blocks() -> Vec<[f32; 64]> {
    test::pseudo_random_blocks(4096)
}

fn dct_forward_bench_multiversion(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    bench.iter(|| {
        dct_forward_8x8_batch(blocks.iter_mut());
        bencher::black_box(&mut blocks);
    })
}

fn dct_inverse_bench_multiversion(bench: &mut Bencher) {
    let mut blocks = bench_blocks();
    bench.iter(|| {
        dct_inverse_8x8_batch(blocks.iter_mut());
        bencher::black_box(&mut blocks);
    })
}

benchmark_group!(
    dct,
    dct_forward_bench_autovectorized,
    dct_inverse_bench_autovectorized,
    dct_forward_bench_multiversion,
    dct_inverse_bench_multiversion
);

benchmark_main!(dct);
