#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;

/// Read image from file
fn read_single_image(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";

    bench.iter(||{
        rgba::Image::read_from_file(path, read_options::low(), rgba::pixels::flattened_f16).unwrap();
    })
}

/// Read image from in-memory buffer
fn read_single_image_from_buffer(bench: &mut Bencher) {
    let file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();

    bench.iter(||{
        rgba::Image::read_from_buffered(std::io::Cursor::new(&file), read_options::low(), rgba::pixels::flattened_f16).unwrap();
    })
}


benchmark_group!(profiling,
    read_single_image_from_buffer,
    read_single_image,
);

benchmark_main!(profiling);