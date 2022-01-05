
#[macro_use]
extern crate smallvec;
extern crate rand;
extern crate half;


// exr imports
extern crate exr;

/// Generate an image with channel groups and write it to a file.
/// Some legacy software may group layers that contain a `.` in the layer name.
///
/// Note: This is an OpenEXR legacy strategy. OpenEXR supports layers natively since 2013.
/// Use the natively supported exrs `Layer` types instead, if possible.
///
fn main() {
    use exr::prelude::*;
    let size = Vec2(512, 512);

    let create_channel = |name: &str| -> AnyChannel<FlatSamples> {
        let color: f16 = f16::from_bits(rand::random::<u16>());

        AnyChannel::new(
            name,
            FlatSamples::F16(vec![color; size.area() ])
        )
    };


    let layer = Layer::new(
        size,
        LayerAttributes::named("test-image"),
        Encoding::FAST_LOSSLESS,

        ChannelGroups::from_list([
            (
                // the foreground layer will be rgba
                "Foreground",
                AnyChannels::sort(smallvec![
                    create_channel("R"), create_channel("G"),
                    create_channel("B"), create_channel("A"),
                ])
            ),

            (
                // the background layer will be rgb
                "Background",
                AnyChannels::sort(smallvec![
                    create_channel("R"),
                    create_channel("G"),
                    create_channel("B")
                ])
            ),
        ]),
    );

    let image = Image::from_layer(layer);

    println!("writing image {:#?}", image);
    image.write().to_file("tests/images/out/groups.exr").unwrap();

    println!("created file groups.exr");
}