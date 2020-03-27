#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;

/// Read uncompressed (always single core)
fn read_rgba(bench: &mut Bencher) {
    bench.iter(||{
        let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";

        let image = rgba::Image::read_from_file(path, read_options::low(), rgba::pixels::flattened_f16).unwrap();
        bencher::black_box(image);
    })
}

/// Read uncompressed (always single core)
fn read_full(bench: &mut Bencher) {
    bench.iter(||{
        let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";

        let image = exr::image::full::Image::read_from_file(path, read_options::low()).unwrap();
        bencher::black_box(image);
    })
}

/// Read uncompressed (always single core)
fn read_simple(bench: &mut Bencher) {
    bench.iter(||{
        let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";

        let image = simple::Image::read_from_file(path, read_options::low()).unwrap();
        bencher::black_box(image);
    })
}

benchmark_group!(rgba,
    read_rgba,
    read_full,
    read_simple
);

benchmark_main!(rgba);