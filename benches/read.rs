#[macro_use]
extern crate bencher;

extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;
use std::io::Cursor;

/// Read uncompressed (always single core)
fn read_single_image_uncompressed_rgba(bench: &mut Bencher) {
    bench.iter(||{
        let path = "tests/images/valid/custom/crowskull/crow_uncompressed.exr";

        let image = read_all_rgba_layers_from_file(
            path, exr::image::pixel_vec::create_pixel_vec::<(f16,f16,f16,f16), _>, exr::image::pixel_vec::set_pixel_in_vec::<(f16,f16,f16,f16)>
        ).unwrap();

        bencher::black_box(image);
    })
}

/// Read from in-memory in parallel
fn read_single_image_uncompressed_from_buffer_rgba(bench: &mut Bencher) {
    let file = fs::read("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();

    bench.iter(||{
        let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .rgba_channels(exr::image::pixel_vec::create_pixel_vec::<(f16,f16,f16,f16), _>, exr::image::pixel_vec::set_pixel_in_vec)
            .all_layers().all_attributes()
            .from_buffered(Cursor::new(file.as_slice())).unwrap();

        bencher::black_box(image);
    })
}

/// Read with multi-core zip decompression
fn read_single_image_zips_rgba(bench: &mut Bencher) {
    bench.iter(||{
        let path = "tests/images/valid/custom/crowskull/crow_zips.exr";

        let image = read_all_rgba_layers_from_file(
            path, exr::image::pixel_vec::create_pixel_vec::<(f16,f16,f16,f16), _>, exr::image::pixel_vec::set_pixel_in_vec
        ).unwrap();

        bencher::black_box(image);
    })
}

/// Read with multi-core RLE decompression
fn read_single_image_rle_all_channels(bench: &mut Bencher) {
    bench.iter(||{
        let path = "tests/images/valid/custom/crowskull/crow_rle.exr";

        let image = read_all_flat_layers_from_file(path).unwrap();
        bencher::black_box(image);
    })
}

/// Read without multi-core ZIP decompression
fn read_single_image_non_parallel_zips_rgba(bench: &mut Bencher) {
    bench.iter(||{
        let path = "tests/images/valid/custom/crowskull/crow_zips.exr";

        let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .rgba_channels(exr::image::pixel_vec::create_pixel_vec::<(f16,f16,f16,f16), _>, exr::image::pixel_vec::set_pixel_in_vec)
            .all_layers().all_attributes()
            .non_parallel()
            .from_file(path).unwrap();

        bencher::black_box(image);
    })
}


benchmark_group!(read,
    read_single_image_uncompressed_from_buffer_rgba,
    read_single_image_uncompressed_rgba,
    read_single_image_zips_rgba,
    read_single_image_rle_all_channels,
    read_single_image_non_parallel_zips_rgba
);

benchmark_main!(read);