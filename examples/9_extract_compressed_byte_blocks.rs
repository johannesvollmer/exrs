use std::io::{BufReader};
use std::fs::File;
use exr::block::chunk::CompressedBlock;
use std::path::Path;
use exr::prelude::*;

/// Extracts the compressed byte blocks from an image.
/// They could be decompressed using the appropriate zlib function.
fn main() {
    let out_folder_dir = Path::new("compressed_chunks/");
    let _ = std::fs::create_dir_all(out_folder_dir);

    let input_file = Path::new("tests/images/valid/custom/crowskull/crow_zip_half.exr");
    let file = BufReader::new(File::open(input_file).unwrap());

    // start reading the file, extracting the meta data of the image
    let image_reader = exr::block::read(file, true).unwrap();

    // print some debug info
    for header in &image_reader.meta_data().headers {
        match header.compression {
            Compression::ZIP1 => println!("image contains line-by-line zip compression"),
            Compression::ZIP16 => println!("image contains zip compression of 16 lines at once"),
            _ => panic!("image contains non-zip contents"),
        }

        println!("the image pixels are split up by: {:#?}", header.blocks);
        println!("the channels are: {:#?}", header.channels);
    }

    // create a reader that loads all chunks from the file
    // note: chunks are the zip compressed pixel sections from the image
    let chunks = image_reader.all_chunks(false).unwrap();
    let chunk_count = chunks.len();

    // load the chunks without decompressing
    for (index, chunk) in chunks.enumerate() {
        let chunk = bytes_only_from_chunk(chunk.unwrap().compressed_block);

        let path = out_folder_dir.join(
            input_file.file_name().unwrap().to_str().unwrap().to_owned()
                + "_chunk_" + index.to_string().as_str() + ".bin"
        );

        println!("chunk #{} has {:?} bytes", index, chunk.len());
        println!("writing chunk {} to {:?}", index, &path);
        std::fs::write(&path, &chunk).unwrap();
    }

    println!("\nunpacked {} compressed chunks", chunk_count);
}

fn bytes_only_from_chunk(block: CompressedBlock) -> Vec<u8> {
    match block {
        CompressedBlock::ScanLine(block) => block.compressed_pixels,
        CompressedBlock::Tile(block) => block.compressed_pixels,
        CompressedBlock::DeepScanLine(block) => block.compressed_sample_data,
        CompressedBlock::DeepTile(block) => block.compressed_sample_data
    }
}