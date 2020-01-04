#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;


fn read_single_image_uncompressed(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_uncompressed.exr";

            let image = FullImage::read_from_file(path, ReadOptions::default()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn read_single_image_uncompressed_from_buffer(bench: &mut Bencher) {
    let file = fs::read("D:/Pictures/openexr/crowskull/crow_uncompressed.exr").unwrap();

    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let image = FullImage::read_from_buffered(file.as_slice(), ReadOptions::fast_loading()).unwrap();
            bencher::black_box(image);
        })
    })
}


benchmark_group!(profiling,
    read_single_image_uncompressed_from_buffer,
    read_single_image_uncompressed,
);

benchmark_main!(profiling);