#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::common::*;
use exr::prelude::rgba_image as rgb;

use bencher::Bencher;
use std::fs;

/// Read image from file
fn read_single_image(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";

    bench.iter(||{
        rgb::ImageInfo::read_pixels_from_file(
            path, read_options::low(),
            rgb::pixels::create_flattened_f16,
            rgb::pixels::flattened_pixel_setter()
        ).unwrap();
    })
}

/// Read image from in-memory buffer
fn read_single_image_from_buffer(bench: &mut Bencher) {
    let file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();

    bench.iter(||{
        rgb::ImageInfo::read_pixels_from_buffered(
            std::io::Cursor::new(&file), read_options::low(),
            rgb::pixels::create_flattened_f16,
            rgb::pixels::flattened_pixel_setter()
        ).unwrap();
    })
}


benchmark_group!(profiling,
    read_single_image_from_buffer,
    read_single_image,
);

benchmark_main!(profiling);