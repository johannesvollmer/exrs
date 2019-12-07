#[macro_use]
extern crate bencher;

use bencher::Bencher;
use rs_exr::prelude as exr;

fn single_image_uncompressed(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = ::std::path::Path::new(
                "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
            );

            let image = exr::read(path, true).unwrap();
            bencher::black_box(image);
        })
    })
}

fn single_image_zips(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = ::std::path::Path::new(
                "D:/Pictures/openexr/crowskull/crow_zips.exr"
            );

            let image = exr::read(path, true).unwrap();
            bencher::black_box(image);
        })
    })
}

fn single_image_rle(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = ::std::path::Path::new(
                "D:/Pictures/openexr/crowskull/crow_rle.exr"
            );

            let image = exr::read(path, true).unwrap();
            bencher::black_box(image);
        })
    })
}

fn single_image_non_parallel_zips(bench: &mut Bencher) {
    bench.bench_n(1, |bencher| {
        bencher.iter(||{
            let path = ::std::path::Path::new(
                "D:/Pictures/openexr/crowskull/crow_zips.exr"
            );

            let image = exr::read(path, false).unwrap();
            bencher::black_box(image);
        })
    })
}

benchmark_group!(benches, single_image_uncompressed, single_image_zips, single_image_rle, single_image_non_parallel_zips);
benchmark_main!(benches);