
extern crate exr;

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use exr::block::chunk::Chunk;
use exr::block::UncompressedBlock;
use exr::image::{AnyChannel, AnyChannels, FlatSamples, Image};
use exr::prelude::{IntegerBounds, WritableImage};

/// load only some specific pixel sections from the file, just when they are needed.
/// load blocks of pixels into a sparse texture (illustrated with a hashmap in this example).
/// the process is as follows:
///
/// 1. prepare some state (open the file, read meta data)
/// 2. when needed, load more pixel blocks from the file
///    a. load compressed chunks for a specific pixel section
///    b. decompress chunks and extract pixels from the packed channel data in the block
///    c. write the loaded pixel blocks into the sparse texture
fn main() {

    // this is where we will store our loaded data.
    // for this example, we use a hashmap instead of a real sparse texture.
    // it stores a vector of channels, each containing either f32, f16, or u32 samples
    let mut my_sparse_texture: HashMap<(Pos, Size), Vec<FlatSamples>> = Default::default();
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
    let channel_info = &layer_info.channels.list;
    println!("loading header #0 from {:#?}", exr_info);

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

        // exr blocks store line by line, each line stores all the channels.
        // what we might want instead is to store channel by channel, each channel containing all the lines for this block.
        let unpacked_blocks = packed_pixel_blocks.map(|block|{
            // obtain a vector of channels, where each channel contains the whole block
            let channels = block.unpack_channels(layer_info);

            let size = block.index.pixel_size;
            let position = block.index.pixel_position.to_i32() + layer_info.own_attributes.layer_position;

            (position, size, channels)
        });

        for (position, size, block) in unpacked_blocks {
            my_sparse_texture.insert((position.into(), size.into()), block);
        }
    });


    println!("\n\nsparse texture now contains {} blocks", my_sparse_texture.len());

    // write a png for each block
    for (index, ((_pos, (width, height)), channel_data)) in my_sparse_texture.into_iter().enumerate() {
        let path = format!("block #{}.exr", index);
        let channel_names = channel_info.iter().map(|c| c.name.clone());

        let image = Image::from_channels((width, height), AnyChannels::sort(
            channel_names.zip(channel_data)
                .map(|(chan, channel_data)| AnyChannel::new(chan, channel_data))
                .collect()
        ));

        image.write().to_file(path).unwrap();
    }

    println!("Written the blocks as exr files.");
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