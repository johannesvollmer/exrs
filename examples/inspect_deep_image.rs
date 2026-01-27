//! Inspect deep image metadata to understand structure

use std::path::PathBuf;

fn inspect_image(name: &str) {
    let test_data_dir = PathBuf::from("test_data");
    let path = test_data_dir.join(name);

    if !path.exists() {
        eprintln!("{} not found", name);
        return;
    }

    // Read using low-level API to get metadata
    let file = std::fs::File::open(&path).expect("Failed to open file");
    let reader = exr::block::read(file, false).expect("Failed to read file");
    let meta = reader.meta_data();

    println!("\n=== {} ===", name);
    for (idx, header) in meta.headers.iter().enumerate() {
        println!(
            "Header {}: {}x{}, {} deep, {:?} compression, {} channels",
            idx,
            header.layer_size.width(),
            header.layer_size.height(),
            if header.deep { "IS" } else { "NOT" },
            header.compression,
            header.channels.list.len()
        );

        let data_window = header.data_window();
        println!(
            "  Data Window: min=({},{}), size={}x{}",
            data_window.position.x(),
            data_window.position.y(),
            data_window.size.x(),
            data_window.size.y()
        );
    }

    // Count actual blocks (only for deep images)
    if meta.headers[0].deep {
        let blocks = exr::image::read::deep::read_deep_from_file(&path, false)
            .expect("Failed to read blocks");
        println!("Actual blocks: {}", blocks.len());

        // Check block positions
        if !blocks.is_empty() {
            println!(
                "First block: y={} size={}x{}",
                blocks[0].index.pixel_position.y(),
                blocks[0].index.pixel_size.x(),
                blocks[0].index.pixel_size.y()
            );
            println!(
                "Last block: y={} size={}x{}",
                blocks.last().unwrap().index.pixel_position.y(),
                blocks.last().unwrap().index.pixel_size.x(),
                blocks.last().unwrap().index.pixel_size.y()
            );
        }
    }
}

fn main() {
    inspect_image("Balls.exr");
    inspect_image("Ground.exr");
    inspect_image("Leaves.exr");
    inspect_image("Trunks.exr");
    inspect_image("composited.exr");
}
