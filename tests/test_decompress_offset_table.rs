#[cfg(feature = "deep-data")]
mod test {
    use exr::block;
    use exr::compression::Compression;
    use std::fs::File;
    
    #[test]
    fn test_decompress_offset_table_only() {
        let file = File::open("test_data/Balls.exr").expect("Failed to open");
        let mut reader = block::read(file, false).expect("Failed to create reader");
        
        let meta = reader.meta_data().clone();
        let header = &meta.headers[0];
        
        println!("Header: {}x{}, compression={:?}", 
            header.layer_size.width(), header.layer_size.height(), header.compression);
        
        let chunks = reader.all_chunks(false).expect("Failed to get chunks");
        
        for (i, chunk_result) in chunks.enumerate() {
            if i >= 1 { break; }
            
            let chunk = chunk_result.expect("Failed to read chunk");
            
            if let exr::block::chunk::CompressedBlock::DeepScanLine(block) = &chunk.compressed_block {
                println!("\nChunk {}:", i);
                println!("  Width (from header): {}", header.layer_size.width());
                println!("  Compressed offset table size: {}", block.compressed_pixel_offset_table.len());
                println!("  Expected uncompressed size: {} bytes (764 pixels * 4)", 764 * 4);
                
                // Try to decompress offset table manually
                println!("\n  Testing offset table decompression...");
                let result = header.compression.decompress_deep_offset_table(
                    &block.compressed_pixel_offset_table,
                    764  // num_pixels
                );
                
                match result {
                    Ok(table) => {
                        println!("  SUCCESS! Decompressed {} entries", table.len());
                        if table.len() > 0 {
                            println!("  First 5 values: {:?}", &table[..5.min(table.len())]);
                        }
                    },
                    Err(e) => {
                        println!("  FAILED: {:?}", e);
                    }
                }
            }
        }
    }
}
