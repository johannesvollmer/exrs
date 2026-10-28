//! Read deep images, inspect their samples, and write them again.

extern crate exr;

use exr::{image::deep::DeepChannels, prelude::*};

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
    let balls: &DeepChannels = &images[2].layer_data.channel_data;
    let samples = balls.sample_range(Vec2(430, 0));

    for channel in balls.channels() {
        let values: Vec<f32> = samples
            .clone()
            .map(|index| channel.sample_data.value_by_flat_index(index).to_f32())
            .collect();

        println!("samples of channel {} in a pixel: {:?}", channel.name, values);
    }

    // deep images are written just like flat images
    images[2].write().to_file("deep_balls.exr").expect("cannot write deep image");
    println!("created file `deep_balls.exr`");
}
