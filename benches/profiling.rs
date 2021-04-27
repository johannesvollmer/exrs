#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;
use std::io::Cursor;
use exr::image::pixel_vec::PixelVec;

/// Read image from file
fn read_single_image_all_channels(bench: &mut Bencher) {
    let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";

    bench.iter(||{
        let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .all_channels()
            .all_layers().all_attributes()
            .non_parallel()
            .from_file(path).unwrap();

        bencher::black_box(image);
    })
}

/// Read image from in-memory buffer
fn read_single_image_from_buffer_rgba_channels(bench: &mut Bencher) {
    let file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();

    bench.iter(||{
        let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .rgba_channels(PixelVec::<(f16,f16,f16,f16)>::constructor, PixelVec::set_pixel)
            .all_layers().all_attributes()
            .non_parallel()
            .from_buffered(Cursor::new(file.as_slice())).unwrap();

        bencher::black_box(image);
    })
}


benchmark_group!(profiling,
    read_single_image_from_buffer_rgba_channels,
    read_single_image_all_channels,
);

benchmark_main!(profiling);