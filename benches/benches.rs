#[macro_use]
extern crate bencher;

use bencher::Bencher;
use rs_exr::prelude as exr;

fn single_image(bench: &mut Bencher) {
    bench.bench_n(5, |bencher| {
        bencher.iter(||{
            let path = ::std::path::Path::new(
                "D:/Pictures/openexr/crowskull/crow_zips.exr"
            );

            let image = exr::read(path, true).unwrap();
            bencher::black_box(image);
        })
    })
}

fn single_image_non_parallel(bench: &mut Bencher) {
    bench.bench_n(3, |bencher| {
        bencher.iter(||{
            let path = ::std::path::Path::new(
                "D:/Pictures/openexr/crowskull/crow_zips.exr"
            );

            let image = exr::read(path, false).unwrap();
            bencher::black_box(image);
        })
    })
}

benchmark_group!(benches, single_image, single_image_non_parallel);
benchmark_main!(benches);