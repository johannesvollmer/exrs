
// exr imports
extern crate exr;
use exr::prelude::*;
use exr::image::rgba::Pixels;

/// Read an RGBA image and then write it back.
/// Uses multicore compression where appropriate.
fn main() {
    let mut image = rgba::Image::read_from_file("./testout/written.exr", read_options::high()).unwrap();
    println!("loaded image {:#?}", image);

    // invert the central horizontal line:
    let y = image.resolution.1 / 2;
    for x in 0..image.resolution.0 {
        let index = image.vector_index_of_first_pixel_component(Vec2(x, y));
        match &mut image.data {
            Pixels::F16(rgba) => {
                invert(&mut rgba[index + 0]);
                invert(&mut rgba[index + 1]);
                invert(&mut rgba[index + 2]);
            },

            _ => unimplemented!()
        }
    }

    /// Invert a single sample brightness, assuming a max value of 1.0
    fn invert(value: &mut f16) {
        *value = f16::from_f32(1.0 - value.to_f32())
    }

    image.write_to_file("./testout/written_copy.exr", write_options::high()).unwrap();

    // just a quick check that the images are equivalent:
    assert_eq!(image, rgba::Image::read_from_file("./testout/written_copy.exr", read_options::high()).unwrap());
}