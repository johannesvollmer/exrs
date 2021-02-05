
extern crate smallvec;
extern crate rand;
extern crate half;


// exr imports
extern crate exr;

///
fn main() {
    use exr::prelude::*;
    let size = Vec2(512, 512);

    // this is the content of our layers
    // TODO this could be a closure, but two different closures cannot be put into one slice
    #[derive(Debug)]
    struct SolidColorRgbPixels((f32, f32, f32)); // r,g,b

    // make our layer writable
    impl GetPixel for SolidColorRgbPixels {
        type Pixel = (f32,f32,f32); // r,g,b

        // return the same color for every pixel
        fn get_pixel(&self, _: Vec2<usize>) -> Self::Pixel { self.0 }
    }

    let layer1 = Layer::new(
        size,
        LayerAttributes::named("teal"),
        Encoding::FAST_LOSSLESS,
        SpecificChannels::rgb(SolidColorRgbPixels((0.2_f32, 0.8_f32, 0.8_f32))),
    );

    let layer2 = Layer::new(
        size,
        LayerAttributes::named("orange"),
        Encoding::FAST_LOSSLESS,
        SpecificChannels::rgb(SolidColorRgbPixels((0.8_f32, 0.8_f32, 0.2_f32))),
    );

    let layers = [layer2, layer1]; // could also be a `Vec<Layer<_>>` or `Layers`
    let image = Image::from_layers_slice(
        // define the visible area of the canvas
        ImageAttributes::new(IntegerBounds::from_dimensions(size)),
        &layers
    );

    println!("writing image {:#?}", image);
    image.write().to_file("tests/images/out/layers.exr").unwrap();

    println!("created file layers.exr");
}