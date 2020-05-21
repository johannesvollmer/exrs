#[macro_use]
extern crate bencher;

extern crate exr;
use exr::image::full::*;

use bencher::Bencher;
use std::io::Cursor;
use exr::image::{read_options, write_options};
use exr::prelude::common::*;

/// Write with multi-core compression
fn write_single_image_parallel(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_rle.exr";
    let image = Image::read_from_file(path, read_options::high()).unwrap();

    bench.iter(||{
        let mut result = Vec::new();
        Image::write_to_buffered(&image, Cursor::new(&mut result), write_options::high()).unwrap();
        bencher::black_box(result);
    })
}

/// Write with multi-core compression
fn write_zip1(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_rle.exr";
    let mut image = Image::read_from_file(path, read_options::high()).unwrap();
    for layer in &mut image.layers { layer.compression = Compression::ZIP1 };

    bench.iter(||{
        let mut result = Vec::new();
        Image::write_to_buffered(&image, Cursor::new(&mut result), write_options::high()).unwrap();
        println!("zip1 size: {}", result.len() as f32 / 1000000.0);
        bencher::black_box(result);
    })
}

/// Write with multi-core compression
fn write_zip16(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_rle.exr";
    let mut image = Image::read_from_file(path, read_options::high()).unwrap();
    for layer in &mut image.layers { layer.compression = Compression::ZIP16 };

    bench.iter(||{
        let mut result = Vec::new();
        Image::write_to_buffered(&image, Cursor::new(&mut result), write_options::high()).unwrap();
        println!("zip16 size: {}", result.len() as f32 / 1000000.0);
        bencher::black_box(result);
    })
}

/// Write with singlecore compression
fn write_single_image(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_rle.exr";
    let image = Image::read_from_file(path, read_options::high()).unwrap();

    bench.iter(||{
        let mut result = Vec::new();
        Image::write_to_buffered(&image, Cursor::new(&mut result), write_options::low()).unwrap();
        bencher::black_box(result);
    })
}

/// Write with singlecore zip compression
fn write_single_image_uncompressed(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";
    let image = Image::read_from_file(path, read_options::high()).unwrap();

    bench.iter(||{
        let mut result = Vec::new();
        Image::write_to_buffered(&image, Cursor::new(&mut result), write_options::high()).unwrap();
        bencher::black_box(result);
    })
}

benchmark_group!(write,
    // write_single_image_parallel,
    // write_single_image_uncompressed,
    write_zip1,
    write_zip16,
    // write_single_image
);

benchmark_main!(write);