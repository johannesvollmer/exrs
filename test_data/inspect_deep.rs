// Quick test to inspect the deep data file with current exrs
// This should fail with "deep data not supported"

use exrs::prelude::*;

fn main() {
    let path = "Balls.exr";

    println!("Attempting to read: {}", path);

    // Try to read metadata first
    match exrs::meta::MetaData::read_from_file(path, false) {
        Ok(metadata) => {
            println!("Successfully read metadata!");
            println!("Requirements: {:?}", metadata.requirements);
            println!("Number of headers: {}", metadata.headers.len());

            for (i, header) in metadata.headers.iter().enumerate() {
                println!("\n=== Header {} ===", i);
                println!("Deep: {}", header.deep);
                println!("Compression: {:?}", header.compression);
                println!("Blocks: {:?}", header.blocks);
                println!("Layer size: {:?}", header.layer_size);
                println!("Chunk count: {}", header.chunk_count);

                if let Some(max_samples) = header.max_samples_per_pixel {
                    println!("Max samples per pixel: {}", max_samples);
                }

                println!("\nChannels:");
                for channel in &header.channels.list {
                    println!("  - {} ({:?})", channel.name, channel.sample_type);
                }

                println!("\nLayer attributes:");
                if let Some(name) = &header.own_attributes.layer_name {
                    println!("  Layer name: {}", name);
                }
            }
        }
        Err(e) => {
            println!("Error reading metadata: {}", e);
        }
    }

    println!("\n\nAttempting to read full image...");

    // Try to read the full image
    match read().no_deep_data().all_layers().all_channels().all_attributes().from_file(path) {
        Ok(_image) => {
            println!("Successfully read image (unexpected!)");
        }
        Err(e) => {
            println!("Error reading image: {}", e);
        }
    }
}
