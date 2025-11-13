#[cfg(feature = "deep-data")]
mod test {
    use exr::block;
    use std::fs::File;
    
    #[test]
    fn test_decompress_sample_data_only() {
        let file = File::open("test_data/Balls.exr").expect("Failed to open");
        let mut reader = block::read(file, false).expect("Failed to create reader");
        
        let meta = reader.meta_data().clone();
        let header = &meta.headers[0];
        
        let chunks = reader.all_chunks(false).expect("Failed to get chunks");
        
        for (i, chunk_result) in chunks.enumerate() {
            if i >= 1 { break; }
            
            let chunk = chunk_result.expect("Failed to read chunk");
            
            if let exr::block::chunk::CompressedBlock::DeepScanLine(block) = &chunk.compressed_block {
                println!("Chunk {}:", i);
                println!("  Compressed sample data size: {}", block.compressed_sample_data_le.len());
                println!("  Expected decompressed size: {}", block.decompressed_sample_data_size);
                
                println!("\n  Testing sample data decompression...");
                let result = header.compression.decompress_deep_sample_data(
                    header,
                    block.compressed_sample_data_le.clone(),
                    block.decompressed_sample_data_size
                );
                
                match result {
                    Ok(data) => {
                        println!("  SUCCESS! Decompressed {} bytes", data.len());
                    },
                    Err(e) => {
                        println!("  FAILED: {:?}", e);
                    }
                }
            }
        }
    }
}
