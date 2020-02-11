#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::io::Cursor;


fn write_single_image_parallel_zip(bench: &mut Bencher) {
    let path = "D:/Pictures/openexr/crowskull/crow_zips.exr";
    let options = ReadOptions {
        parallel_decompression: false,
        .. ReadOptions::default()
    };

    let image = FullImage::read_from_file(path, options).unwrap();

    bench.iter(||{
        let mut result = Vec::new();
        FullImage::write_to_buffered(&image, Cursor::new(&mut result), WriteOptions::debug()).unwrap();
        bencher::black_box(result);
    })
}

benchmark_group!(write,
    write_single_image_parallel_zip
);

benchmark_main!(write);