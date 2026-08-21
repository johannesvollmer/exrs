extern crate exr;

use std::time::{Duration, Instant};

use exr::prelude::*;

fn bithash_half(width: usize, pixels: &[Vec<[u16; 3]>], mut h: u64) -> u64 {
    for channel_index in 0..3 {
        for row in pixels {
            for pixel in row.iter().take(width) {
                let bits = pixel[channel_index] as u64;
                h ^=
                    bits.wrapping_add(0x9e3779b97f4a7c15).wrapping_add(h << 6).wrapping_add(h >> 2);
            }
        }
    }
    h
}

// Same hash for the legacy `[f32; 4]` storage, which has to convert back to
// half first. Both modes therefore produce identical hashes for the same file.
fn bithash_f32(width: usize, pixels: &[Vec<[f32; 4]>], mut h: u64) -> u64 {
    for channel_index in 0..3 {
        for row in pixels {
            for pixel in row.iter().take(width) {
                let bits = f16::from_f32(pixel[channel_index]).to_bits() as u64;
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
        eprintln!("usage: {} <file> <0|1 parallel> <iters> [half|f32x4]", args[0]);
        eprintln!("  half   three half channels, 6 bytes/pixel (default; matches");
        eprintln!("         bench_openexr.cpp's three `Array2D<half>` planes)");
        eprintln!("  f32x4  RGBA as f32, 16 bytes/pixel (the previous default)");
        std::process::exit(1);
    }

    let path = &args[1];
    let parallel = args[2] == "1";
    let iters: usize = args[3].parse().expect("iters must be a number");
    let storage = args.get(4).map(String::as_str).unwrap_or("half");

    let hash_enabled = std::env::var_os("DWA_BENCH_NO_HASH").is_none();

    #[cfg(feature = "dwa-profile")]
    exr::compression::dwa::profile::reset();

    macro_rules! run_mode {
        ($channels:expr, $create:expr, $set:expr, $hash:expr) => {{
            let mut total = Duration::ZERO;
            let mut hash = 0u64;

            let mut reader =
                $channels.collect_pixels($create, $set).first_valid_layer().all_attributes();

            if !parallel {
                reader = reader.non_parallel();
            }

            for _ in 0..iters {
                let start = Instant::now();

                let pixels = reader
                    .clone()
                    .from_file(path)
                    .expect("failed to read exr file")
                    .layer_data
                    .channel_data
                    .pixels;

                total += start.elapsed();
                if hash_enabled {
                    let width = pixels.first().map_or(0, Vec::len);
                    hash = $hash(width, &pixels, hash);
                }
            }

            (total, hash)
        }};
    }

    let channels = || read().no_deep_data().largest_resolution_level().specific_channels();

    let (total, hash) = match storage {
        "half" => run_mode!(
            channels().required("R").required("G").required("B"),
            |resolution, _| vec![vec![[0u16; 3]; resolution.width()]; resolution.height()],
            |vec: &mut Vec<Vec<[u16; 3]>>, pos: Vec2<usize>, (r, g, b): (f16, f16, f16)| {
                vec[pos.y()][pos.x()] = [r.to_bits(), g.to_bits(), b.to_bits()];
            },
            bithash_half
        ),

        // What this benchmark used before, kept so earlier numbers stay
        // reproducible: RGBA as f32, 16 bytes/pixel.
        "f32x4" => run_mode!(
            channels().required("R").required("G").required("B").optional("A", 1.0f32),
            |resolution, _| vec![vec![[0.0f32; 4]; resolution.width()]; resolution.height()],
            |vec: &mut Vec<Vec<[f32; 4]>>, pos: Vec2<usize>, (r, g, b, a): (f32, f32, f32, f32)| {
                vec[pos.y()][pos.x()] = [r, g, b, a];
            },
            bithash_f32
        ),

        other => {
            eprintln!("unknown storage mode {:?}, expected `half` or `f32x4`", other);
            std::process::exit(1);
        }
    };

    println!(
        "file={} parallel={} iters={} storage={} avg_ms={:.3} bithash={:016x}",
        path,
        parallel,
        iters,
        storage,
        total.as_secs_f64() * 1000.0 / iters as f64,
        hash
    );

    #[cfg(feature = "dwa-profile")]
    exr::compression::dwa::profile::report(iters as u64);
}
