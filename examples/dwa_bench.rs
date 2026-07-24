extern crate exr;

use std::time::Instant;

use exr::prelude::*;

// Matches bench_openexr.cpp's bithash: whole R plane, then whole G plane,
// then whole B plane (row-major), so hashes are directly comparable across
// the exrs and OpenEXR C++ implementations for the same file.
fn bithash(pixels: &[Vec<[f32; 4]>], mut h: u64) -> u64 {
    for channel_index in 0..3 {
        for row in pixels {
            for pixel in row {
                let bits = half::f16::from_f32(pixel[channel_index]).to_bits() as u64;
                h ^=
                    bits.wrapping_add(0x9e3779b97f4a7c15).wrapping_add(h << 6).wrapping_add(h >> 2);
            }
        }
    }
    h
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: {} <file> <0|1 parallel> <iters>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let parallel = args[2] == "1";
    let iters: usize = args[3].parse().expect("iters must be a number");

    let mut total = std::time::Duration::ZERO;
    let mut hash = 0u64;

    for _ in 0..iters {
        let start = Instant::now();

        let mut reader = read()
            .no_deep_data()
            .largest_resolution_level()
            .rgba_channels(
                |resolution, _| vec![vec![[0.0f32; 4]; resolution.width()]; resolution.height()],
                |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                    pixels[position.y()][position.x()] = [r, g, b, a];
                },
            )
            .first_valid_layer()
            .all_attributes();

        if !parallel {
            reader = reader.non_parallel();
        }

        let image = reader.from_file(path).expect("failed to read exr file");
        total += start.elapsed();

        hash = bithash(&image.layer_data.channel_data.pixels, hash);
    }

    println!(
        "file={} parallel={} iters={} avg_ms={:.3} bithash={:016x}",
        path,
        parallel,
        iters,
        total.as_secs_f64() * 1000.0 / iters as f64,
        hash
    );
}
