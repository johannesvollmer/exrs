#[macro_use]
extern crate bencher;

extern crate exr;
use exr::image::full::*;

use bencher::Bencher;
use std::fs;
use exr::image::read_options;

/// Read RLE image from file
fn read_single_image(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_rle.exr";

    bench.iter(||{
        Image::read_from_file(path, read_options::low()).unwrap();
    })
}

/// Read zip image from in-memory buffer
fn read_single_image_from_buffer(bench: &mut Bencher) {
    let file = fs::read("tests/images/valid/custom/crowskull/crow_rle.exr").unwrap();

    bench.iter(||{
        Image::read_from_buffered(file.as_slice(), read_options::low()).unwrap();
    })
}


benchmark_group!(profiling,
    read_single_image_from_buffer,
    read_single_image,
);

benchmark_main!(profiling);