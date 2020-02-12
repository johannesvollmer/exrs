#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;


fn read_single_image_uncompressed(bench: &mut Bencher) {
    bench.iter(||{
        let path = "D:/Pictures/openexr/crowskull/crow_uncompressed.exr";

        let image = full::Image::read_from_file(path, full::ReadOptions::default()).unwrap();
        bencher::black_box(image);
    })
}

fn read_single_image_uncompressed_from_buffer(bench: &mut Bencher) {
    let file = fs::read("D:/Pictures/openexr/crowskull/crow_uncompressed.exr").unwrap();

    bench.iter(||{
        let image = full::Image::read_from_buffered(file.as_slice(), full::ReadOptions::fast_loading()).unwrap();
        bencher::black_box(image);
    })
}

fn read_single_image_zips(bench: &mut Bencher) {
    bench.iter(||{
        let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
        let image = full::Image::read_from_file(path, full::ReadOptions::default()).unwrap();
        bencher::black_box(image);
    })
}

fn read_single_image_rle(bench: &mut Bencher) {
    bench.iter(||{
        let path = "D:/Pictures/openexr/crowskull/crow_rle.exr";
        let image = full::Image::read_from_file(path, full::ReadOptions::default()).unwrap();
        bencher::black_box(image);
    })
}

fn read_single_image_non_parallel_zips(bench: &mut Bencher) {
    bench.iter(||{
        let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
        let options = full::ReadOptions {
            parallel_decompression: false,
            .. full::ReadOptions::default()
        };

        let image = full::Image::read_from_file(path, options).unwrap();
        bencher::black_box(image);
    })
}


benchmark_group!(read,
    read_single_image_uncompressed_from_buffer,
    // write_single_image_parallel_zip,
    read_single_image_uncompressed,
    read_single_image_zips,
    read_single_image_rle,
    read_single_image_non_parallel_zips
);

benchmark_main!(read);