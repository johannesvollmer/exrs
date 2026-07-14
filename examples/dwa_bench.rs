//! Benchmark for DWA (DWAA/DWAB) decompression speed.
//!
//! Build (release, native CPU features - matters for a fair AVX2 comparison):
//!   RUSTFLAGS="-C target-cpu=native" cargo build --release --example dwa_bench
//!
//! Usage:
//!   dwa_bench <file.exr> [0|1] [iters]
//!     file.exr  path to a DWAA/DWAB-compressed EXR file
//!     0|1       0 = single-threaded decode (.non_parallel()), 1 = parallel (rayon); default 0
//!     iters     number of decode iterations to run; default 8
//!
//! Prints per-iteration wall-clock time in ms, then the best and average
//! across all iterations. On the last iteration it also prints a
//! `bithash` - a rolling hash over the R/G/B channel bits, handy for
//! diffing against another decoder
extern crate exr;
use exr::prelude::*;
use std::{env, time::Instant};

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).expect("usage: dwa_bench <file.exr> [parallel] [iters]");
    let parallel = args.get(2).map(|s| s == "1").unwrap_or(false);
    let iters: usize = args.get(3).map(|s| s.parse().unwrap()).unwrap_or(8);

    let file = std::fs::read(path).unwrap();

    let mut best = f64::MAX;
    let mut total = 0.0;
    for it in 0..iters {
        let t0 = Instant::now();

        let reader = exr::prelude::read()
            .no_deep_data()
            .largest_resolution_level()
            .all_channels()
            .first_valid_layer()
            .all_attributes();

        let image = if parallel {
            reader.from_buffered(std::io::Cursor::new(file.as_slice())).unwrap()
        } else {
            reader.non_parallel().from_buffered(std::io::Cursor::new(file.as_slice())).unwrap()
        };

        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        total += ms;
        if ms < best {
            best = ms;
        }
        eprintln!("iter {}: {:.2} ms", it, ms);

        std::hint::black_box(&image);

        if it == iters - 1 {
            // exact bitwise hash over R,G,B (interleaved like the C++ bench)
            let channel = |name: &str| {
                image
                    .layer_data
                    .channel_data
                    .list
                    .iter()
                    .find(|c| c.name.eq(name))
                    .expect("channel missing")
                    .sample_data
                    .values_as_f32()
                    .map(f16::from_f32)
                    .collect::<Vec<f16>>()
            };
            let (r, g, b) = (channel("R"), channel("G"), channel("B"));
            let mut bits: u64 = 0;
            for i in 0..r.len() {
                bits = bits.wrapping_mul(1000003).wrapping_add(r[i].to_bits() as u64);
                bits = bits.wrapping_mul(1000003).wrapping_add(g[i].to_bits() as u64);
                bits = bits.wrapping_mul(1000003).wrapping_add(b[i].to_bits() as u64);
            }
            eprintln!("bithash={}", bits);
        }
    }

    println!(
        "exrs (Rust) parallel={} best={:.2} ms avg={:.2} ms",
        parallel,
        best,
        total / iters as f64
    );
}
