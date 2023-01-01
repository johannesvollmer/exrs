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

/// Read image from file
fn read_single_image_from_buffer_all_channels(bench: &mut Bencher) {
    let mut file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();
    bencher::black_box(&mut file);

    bench.iter(||{
        let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .all_channels()
            .all_layers().all_attributes()
            .non_parallel()
            .from_buffered(Cursor::new(file.as_slice())).unwrap();

        bencher::black_box(image);
    })
}


/// Read image from in-memory buffer
fn read_single_image_from_buffer_rgba_channels(bench: &mut Bencher) {
    let mut file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();
    bencher::black_box(&mut file);

    bench.iter(||{
        let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .rgba_channels(PixelVec::<(f32,f32,f32,f32)>::constructor, PixelVec::set_pixel)
            .all_layers().all_attributes()
            .non_parallel()
            .from_buffered(Cursor::new(file.as_slice())).unwrap();

        bencher::black_box(image);
    })
}

/// Read image from in-memory buffer and convert the samples to u32 (from native f32)
fn read_single_image_from_buffer_rgba_channels_to_u32(bench: &mut Bencher) {
    let mut file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();
    bencher::black_box(&mut file);

    bench.iter(||{
        let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .rgba_channels(PixelVec::<(u32,u32,u32,u32)>::constructor, PixelVec::set_pixel)
            .all_layers().all_attributes()
            .non_parallel()
            .from_buffered(Cursor::new(file.as_slice())).unwrap();

        bencher::black_box(image);
    })
}

/// Read image from in-memory buffer and convert the samples to u16 (from native f32)
/// f16 is not natively supported by CPUs, which introduces unique performance pitfalls
fn read_single_image_from_buffer_rgba_channels_to_f16(bench: &mut Bencher) {
    let mut file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();
    bencher::black_box(&mut file);

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
    read_single_image_all_channels,
    read_single_image_from_buffer_all_channels,
    read_single_image_from_buffer_rgba_channels,
    read_single_image_from_buffer_rgba_channels_to_u32,
    read_single_image_from_buffer_rgba_channels_to_f16,
);

benchmark_main!(profiling);