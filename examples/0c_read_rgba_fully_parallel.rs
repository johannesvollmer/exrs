extern crate exr;
use exr::prelude::*;

use bencher::Bencher;
use std::fs;
use std::io::{Cursor, BufReader};
use exr::image::pixel_vec::PixelVec;
use exr::block::UncompressedBlock;
use exr::compression::Compression::Uncompressed;

fn main(){
    read_single_image_uncompressed_rgba_fully_parallel();
}

/// Read from in-memory in parallel
fn read_single_image_uncompressed_rgba() {
        let file = fs::File::open("tests/images/valid/custom/crowskull/crow_uncompressed.exr").unwrap();

        /*let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .rgba_channels(PixelVec::<(f32,f32,f32,f32)>::constructor, PixelVec::set_pixel)
            .all_layers().all_attributes()
            .from_buffered(Cursor::new(file.as_slice())).unwrap();*/
        let reader = exr::block::reader::Reader::read_from_buffered(
            BufReader::new(file), true
        ).unwrap();

        use exr::block::reader::ChunksReader;
        reader
            .all_chunks(true).unwrap()
            .decompress_parallel(
                true,
                |meta, uncompressed_block: UncompressedBlock| {
                    bencher::black_box(uncompressed_block);
                    Ok(())
                }
            ).unwrap();
}

/// Read from in-memory in fully parallel
fn read_single_image_uncompressed_rgba_fully_parallel() {
        let file = std::path::PathBuf::from("tests/images/valid/custom/crowskull/crow_uncompressed.exr");

        /*let image = exr::prelude::read()
            .no_deep_data().largest_resolution_level()
            .rgba_channels(PixelVec::<(f32,f32,f32,f32)>::constructor, PixelVec::set_pixel)
            .all_layers().all_attributes()
            .from_buffered(Cursor::new(file.as_slice())).unwrap();*/

        exr::block::reader::read_all_blocks_fully_parallel::<UncompressedBlock>(
            file,
            |x| Ok(x),
            |uncompressed_block: UncompressedBlock| {
                bencher::black_box(uncompressed_block);
                Ok(())
            },
            true
        ).unwrap();

        fn uncompressed_block_to_uncompressed_block(block: UncompressedBlock) -> UncompressedBlock { block }
}
