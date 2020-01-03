#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::io::Cursor;
use std::fs;


fn read_single_image_uncompressed(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_uncompressed.exr";

            let image = exr::image::read_from_file(path, ReadOptions::default()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn read_single_image_uncompressed_from_buffer(bench: &mut Bencher) {
    let file = fs::read("D:/Pictures/openexr/crowskull/crow_uncompressed.exr").unwrap();

    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let mut read = file.as_slice();

            let image = exr::image::read_from_buffered(read, ReadOptions::fast()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn read_single_image_zips(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
            let image = exr::image::read_from_file(path, ReadOptions::default()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn read_single_image_rle(bench: &mut Bencher) {
    bench.bench_n(4, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_rle.exr";
            let image = exr::image::read_from_file(path, ReadOptions::default()).unwrap();
            bencher::black_box(image);
        })
    })
}

fn read_single_image_non_parallel_zips(bench: &mut Bencher) {
    bench.bench_n(1, |bencher| {
        bencher.iter(||{
            let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
            let options = ReadOptions {
                parallel_decompression: false,
                .. ReadOptions::default()
            };

            let image = exr::image::read_from_file(path, options).unwrap();
            bencher::black_box(image);
        })
    })
}

fn write_single_image_parallel_zip(bench: &mut Bencher) {
    let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
    let options = ReadOptions {
        parallel_decompression: false,
        .. ReadOptions::default()
    };

    let image = exr::image::read_from_file(path, options).unwrap();

    bench.bench_n(1, |bencher| {
        bencher.iter(||{
            let mut result = Vec::new();
            exr::image::write_to_buffered(&image, Cursor::new(&mut result), WriteOptions::debug()).unwrap();
            bencher::black_box(result);
        })
    })
}

benchmark_group!(benches,
    read_single_image_uncompressed_from_buffer,
    write_single_image_parallel_zip,
    read_single_image_uncompressed,
    read_single_image_zips,
    read_single_image_rle,
    read_single_image_non_parallel_zips
);

benchmark_main!(benches);