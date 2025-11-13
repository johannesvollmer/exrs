#[cfg(feature = "deep-data")]
mod test_balls {
    use exr::block;
    use std::fs::File;
    
    #[test]
    fn test_read_balls_debug() {
        let file = File::open("test_data/Balls.exr").expect("Failed to open Balls.exr");
        let mut reader = block::read(file, false).expect("Failed to create reader");
        
        println!("=== Balls.exr Metadata ===");
        let meta = reader.meta_data().clone();
        println!("Headers: {}", meta.headers.len());
        
        for (i, header) in meta.headers.iter().enumerate() {
            println!("Header {}: {}x{}, compression={:?}, deep={}",
                i,
                header.layer_size.width(),
                header.layer_size.height(),
                header.compression,
                header.deep
            );
            println!("  max_samples_per_pixel: {:?}", header.max_samples_per_pixel);
        }
        
        println!("\n=== Reading First Chunk ===");
        let chunks = reader.all_chunks(false).expect("Failed to get chunks");
        
        for (i, chunk_result) in chunks.enumerate() {
            if i >= 1 { break; }
            
            match chunk_result {
                Ok(chunk) => {
                    println!("Chunk {}: layer={}", i, chunk.layer_index);
                    
                    match &chunk.compressed_block {
                        exr::block::chunk::CompressedBlock::DeepScanLine(block) => {
                            println!("  DeepScanLine:");
                            println!("    y: {}", block.y_coordinate);
                            println!("    offset_table_size: {}", block.compressed_pixel_offset_table.len());
                            println!("    sample_data_size: {}", block.compressed_sample_data_le.len());
                            println!("    decompressed_size: {}", block.decompressed_sample_data_size);
                            
                            println!("\n  Attempting decompression...");
                            let result = exr::block::UncompressedDeepBlock::decompress_chunk(chunk, &meta, false);
                            match result {
                                Ok(uncompressed) => {
                                    println!("  SUCCESS!");
                                    println!("    Pixels: {}", uncompressed.pixel_offset_table.len());
                                    println!("    Sample bytes: {}", uncompressed.sample_data.len());
                                },
                                Err(e) => {
                                    println!("  FAILED: {:?}", e);
                                }
                            }
                        },
                        _ => println!("  Not DeepScanLine"),
                    }
                },
                Err(e) => println!("Chunk {}: ERROR - {:?}", i, e),
            }
        }
    }
}
