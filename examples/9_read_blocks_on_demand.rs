
extern crate exr;

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use exr::block::chunk::Chunk;
use exr::block::UncompressedBlock;
use exr::image::read::specific_channels::{read_specific_channels, RecursivePixelReader};
use exr::prelude::{IntegerBounds, ReadSpecificChannel, Vec2};

/// load only some specific pixel sections from the file, just when they are needed.
/// load blocks of pixels into a sparse texture (illustrated with a hashmap in this example).
/// the process is as follows:
///
/// 1. prepare some state (open the file, read meta data, define the channels we want to read)
/// 2. when needed, load more pixel blocks from the file
///    a. load compressed chunks for a specific pixel section
///    b. decompress chunks and extract rgba pixels from the packed channel data in the block
///    c. write the loaded rgba pixel blocks into the sparse texture
fn main() {

    // this is where we will store our loaded data.
    // for this example, we use a hashmap instead of a real sparse texture.
    // it stores blocks of rgba pixels, indexed by the position of the block (i32, i32) and its size
    let mut my_sparse_texture: HashMap<(Pos, Size), Vec<[f32; 4]>> = Default::default();
    type Pos = (i32, i32);
    type Size = (usize, usize);


    let file = BufReader::new(
        File::open("3GB.exr")
            .expect("run example `7_write_raw_blocks` to generate this image file")
    );

    // initializes a lazy decoder (reads meta data immediately)
    let mut chunk_reader = exr::block::read(file, true).unwrap()
        .on_demand_chunks().unwrap();

    let layer_index = 0; // only load pixels from the first "header" (assumes first layer has rgb channels)
    let mip_level = (0, 0); // only load largest mip map

    let exr_info = &chunk_reader.meta_data().clone();
    let layer_info = &exr_info.headers[layer_index];
    let channel_info = &layer_info.channels;
    println!("loading header #0 from {:#?}", exr_info);

    // this object can decode packed exr blocks to simple rgb (can be shared or cloned across threads)
    let rgb_from_block_extractor = read_specific_channels()
            .required("R").required("G").required("B")
            .optional("A", 1.0)
            .create_recursive_reader(channel_info).unwrap(); // this will fail if the image does not contain rgb channels


    // ...
    // later in your app, maybe when the view changed:
    when_new_pixel_section_must_be_loaded(|pixel_section| {

        // todo: only load blocks that are not loaded yet. maybe an additional filter? or replace this with a more modular filtering architecture?
        let compressed_chunks = chunk_reader
            .load_all_chunks_for_display_space_section(layer_index, mip_level, pixel_section)

            // in this example, we use .flatten(), this simply discards all errors and only continues with the successfully loaded chunks
            // in this example, we collect here due to borrowing meta data
            .flatten().collect::<Vec<Chunk>>();

        // this could be done in parallel, e.g. by using rayon par_iter
        let packed_pixel_blocks = compressed_chunks.into_iter()
            .map(|chunk| UncompressedBlock::decompress_chunk(chunk, exr_info, true))
            .flatten();

        // the exr blocks may contain arbitrary channels, but we are only interested in rgba.
        // so we convert each exr block to an rgba block (vec of [f32; 4])
        let rgba_blocks = packed_pixel_blocks.map(|block| {
            assert_eq!(block.index.layer, layer_index);

            let size = block.index.pixel_size;
            let position = block.index.pixel_position.to_i32() + layer_info.own_attributes.layer_position;
            let mut rgba_buffer = vec![[0.0; 4]; size.area()]; // rgba = 4 floats

            // decode individual pixels into our f32 buffer
            // automatically converts f16 samples to f32 if required
            // ignores all other channel data
            rgb_from_block_extractor.read_pixels_from_block(channel_info, block, |position, (r,g,b,a)|{
                rgba_buffer[position.flat_index_for_size(size)] = [r,g,b,a];
            });

            (position, size, rgba_buffer)
        });

        for (position, size, block) in rgba_blocks {
            my_sparse_texture.insert((position.into(), size.into()), block);
        }
    });

    // we're done! print something
    println!("\n\nsparse texture now contains {} blocks", my_sparse_texture.len());

    // write a png for each block
    for (index, ((_pos, (width, height)), block)) in my_sparse_texture.into_iter().enumerate() {
        exr::prelude::write_rgba_file(
            format!("block #{}.png", index), width, height,
            |x,y| {
                let [r,g,b,a] = block[Vec2(x,y).flat_index_for_size((width, height))];
                (r,g,b,a)
            }
        ).unwrap();
    }
}

/// request to load a specific sub-rect into view
/// (loads a single view once, as this is a stub implementation)
fn when_new_pixel_section_must_be_loaded<'a>(mut load_for_view: impl 'a + FnMut(IntegerBounds)){
    let image_sub_section = IntegerBounds::new(
        (831, 739), // position
        (32, 91) // size
    );

    load_for_view(image_sub_section);
}