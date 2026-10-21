//! Read deep images, merge and flatten them, and write the results.
//! Requires the `deep` feature: `cargo run --example 9_deep_data --features
//! deep`.

extern crate exr;

use exr::{
    image::deep::{compositing, AnyDeepChannels},
    prelude::*,
};

fn main() {
    let directory = "tests/images/valid/openexr/v2/LowResLeftView";

    // each of these images contains a single deep layer
    let images: Vec<DeepImage> = ["Ground.exr", "Trunks.exr", "Balls.exr", "Leaves.exr"]
        .iter()
        .map(|name| {
            read()
                .deep_data()
                .first_valid_layer()
                .from_file(format!("{}/{}", directory, name))
                .expect("cannot read deep image")
        })
        .collect();

    for image in &images {
        let layer = &image.layer_data;
        let statistics = layer.channel_data.statistics();

        println!(
            "layer {:?} at {:?}: {} samples in {} pixels, at most {} samples per pixel",
            layer.attributes.layer_name,
            layer.attributes.layer_position,
            statistics.total_sample_count,
            statistics.pixel_count,
            statistics.max_samples_per_pixel,
        );
    }

    // access the samples of a single pixel
    let balls: &AnyDeepChannels = &images[2].layer_data.channel_data;
    let samples = balls.rgba_samples_at(Vec2(430, 0));
    println!("samples of a pixel: {:#?}", samples);

    // combine all images into a single deep layer, with the samples sorted by
    // depth
    let layers = images.iter().map(|image| &image.layer_data);
    let merged = compositing::merge(layers).expect("cannot merge deep layers");

    // write the merged deep layer, using the display window of the first image
    let merged_image = Image::new(images[0].attributes.clone(), merged);
    merged_image.write().to_file("merged_deep.exr").expect("cannot write deep image");

    // composite the samples of each pixel, from front to back, and write a flat
    // image
    let flat = compositing::flatten(&merged_image.layer_data).expect("cannot flatten deep layer");
    let flat_image = Image::new(images[0].attributes.clone(), flat);
    flat_image.write().to_file("flattened_deep.exr").expect("cannot write flat image");

    println!("created files `merged_deep.exr` and `flattened_deep.exr`");
}
