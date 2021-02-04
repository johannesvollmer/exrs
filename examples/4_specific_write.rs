
// exr imports
extern crate exr;

/// Create an image with strange channels and write it to a file.
fn main() {
    use exr::prelude::*;

    let pixels = SpecificChannels::build()
        .with_channel("Kharthanasus Korthus")
        .with_channel(" Trochäus ")
        .with_channel("11023")
        .with_channel("*?!")
        .with_channel("`--\"")
        .with_channel("\r\r\r\n\n")
        .with_pixel_fn(|position|{
            if position.0 < 1000 {
                (f16::from_f32(0.2), 0.666_f32, 4_u32, 1532434.0213_f32, 0.99999_f32, 3.142594_f32/4.0)
            }
            else {
                (f16::from_f32(0.4), 0.777_f32, 8_u32, 102154.3_f32, 0.00001_f32, 3.142594_f32/4.0)
            }
        });

    let image = Image::with_single_layer((2000,1400), pixels);

    image.write()
        .on_progress(|progress| println!("progress: {:.1}", progress*100.0))
        .to_file("tests/images/out/strange_channels.exr").unwrap();

    println!("created file strange_channels.exr");
}