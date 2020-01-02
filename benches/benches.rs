#[macro_use]
extern crate bencher;

extern crate exr as exr_crate;
pub use exr_crate::prelude as exr;

use bencher::Bencher;


fn single_image_uncompressed(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_uncompressed.exr";

            let image = exr::Image::read_from_file(path, exr::ReadOptions::default()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn single_image_zips(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
            let image = exr::Image::read_from_file(path, exr::ReadOptions::default()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn single_image_rle(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_rle.exr";
            let image = exr::Image::read_from_file(path, exr::ReadOptions::default()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn single_image_non_parallel_zips(bench: &mut Bencher) {
    bench.bench_n(1, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
            let options = exr::ReadOptions {
                parallel_decompression: false,
                .. exr::ReadOptions::default()
            };

            let image = exr::Image::read_from_file(path, options).unwrap();
            bencher::black_box(image);
        })
    })
}

benchmark_group!(benches, single_image_uncompressed, single_image_zips, single_image_rle, single_image_non_parallel_zips);
benchmark_main!(benches);