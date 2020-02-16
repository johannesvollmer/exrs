#[macro_use]
extern crate bencher;

extern crate exr;
use exr::image::full::*;

use bencher::Bencher;
use std::io::Cursor;

/// Write with multicore zip compression
fn write_single_image_parallel_zip(bench: &mut Bencher) {
    let path = "D:/Pictures/openexr/crowskull/crow_rle.exr";
    let image = Image::read_from_file(path, read_options::high()).unwrap();

    bench.iter(||{
        let mut result = Vec::new();
        Image::write_to_buffered(&image, Cursor::new(&mut result), write_options::high()).unwrap();
        bencher::black_box(result);
    })
}

/// Write with singlecore zip compression
fn write_single_image_zip(bench: &mut Bencher) {
    let path = "D:/Pictures/openexr/crowskull/crow_rle.exr";
    let image = Image::read_from_file(path, read_options::high()).unwrap();

    bench.iter(||{
        let mut result = Vec::new();
        Image::write_to_buffered(&image, Cursor::new(&mut result), write_options::low()).unwrap();
        bencher::black_box(result);
    })
}

benchmark_group!(write,
    write_single_image_parallel_zip,
    write_single_image_zip
);

benchmark_main!(write);