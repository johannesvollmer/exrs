#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;
use std::io::Cursor;
use exr::image::pixel_vec::PixelVec;

/// Read an image from an in-memory buffer into its native f32 format
fn read_image_rgba_f32_to_f32(bench: &mut Bencher) {
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

/// Read image and convert the samples to u32 (from native f32)
fn read_image_rgba_f32_to_u32(bench: &mut Bencher) {
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

/// f16 is not natively supported by CPUs, which introduces unique performance pitfalls
fn read_image_rgba_f32_to_f16(bench: &mut Bencher) {
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

benchmark_group!(pixel_format_conversion,
    read_image_rgba_f32_to_f32,
    read_image_rgba_f32_to_u32,
    read_image_rgba_f32_to_f16,
);

benchmark_main!(pixel_format_conversion);