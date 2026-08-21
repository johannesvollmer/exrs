// Throwaway encode-side benchmark for the `separate_byte_planes` fix
// (RLE alpha-channel byte-plane deinterleave, encode side). Reads an RGBA
// file's pixels once, then times only the write/encode step, writing to
// /dev/null-equivalent (a throwaway path), across DWAA/DWAB. Not meant to
// be kept long-term.
extern crate exr;

use std::{sync::Arc, time::Instant};

use exr::prelude::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: {} <file> <iters>", args[0]);
        std::process::exit(1);
    }
    let path = &args[1];
    let iters: usize = args[2].parse().expect("iters must be a number");
    let parallel = args.get(3).map(|a| a == "1").unwrap_or(true);

    let image = read_first_rgba_layer_from_file(
        path,
        |resolution, _| vec![vec![[0.0f32; 4]; resolution.width()]; resolution.height()],
        |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
            pixels[position.y()][position.x()] = [r, g, b, a];
        },
    )
    .expect("failed to read exr file");

    let size = image.layer_data.size;
    let attributes = image.layer_data.attributes.clone();
    let encoding = image.layer_data.encoding;
    let pixels = Arc::new(image.layer_data.channel_data.pixels);

    let out_path = "/tmp/dwa_encode_bench_out.exr";
    let mut total = std::time::Duration::ZERO;

    for _ in 0..iters {
        let pixels = Arc::clone(&pixels);
        let get_pixel = move |position: Vec2<usize>| {
            let p = pixels[position.y()][position.x()];
            (p[0], p[1], p[2], p[3])
        };
        let layer =
            Layer::new(size, attributes.clone(), encoding, SpecificChannels::rgba(get_pixel));
        let out_image = Image::from_layer(layer);

        let start = Instant::now();
        let mut writer = out_image.write();
        if !parallel {
            writer = writer.non_parallel();
        }
        writer.to_file(out_path).expect("failed to write exr file");
        total += start.elapsed();
    }

    println!(
        "file={} iters={} avg_ms={:.3}",
        path,
        iters,
        total.as_secs_f64() * 1000.0 / iters as f64
    );
}
