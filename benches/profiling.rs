#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;


fn read_single_image_uncompressed(bench: &mut Bencher) {
    let path = "D:/Pictures/openexr/crowskull/crow_rle.exr";

    bench.iter(||{
        FullImage::read_from_file(path, ReadOptions::debug()).unwrap();
    })
}

fn read_single_image_uncompressed_from_buffer(bench: &mut Bencher) {
    let file = fs::read("D:/Pictures/openexr/crowskull/crow_zips.exr").unwrap();

    bench.iter(||{
        FullImage::read_from_buffered(file.as_slice(), ReadOptions::debug()).unwrap();
    })
}


benchmark_group!(profiling,
    read_single_image_uncompressed_from_buffer,
    read_single_image_uncompressed,
);

benchmark_main!(profiling);