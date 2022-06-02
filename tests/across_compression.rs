use std::path::Path;
use exr::prelude::*;
use exr::image::validate_results::ValidateResult;

fn dir() -> &'static Path { Path::new("tests/images/valid/custom/compression_methods") }

fn expect_eq_other(sub_dir: &str, image_name: &str, expected: &str) {
    let path = dir().join(sub_dir).join(image_name);

    match read_first_flat_layer_from_file(path) {
        Err(Error::NotSupported(message)) => println!("skipping ({})", message),
        Err(error) => panic!("unexpected error: {}", error),
        Ok(mut decompressed) => {
            let decompressed_path = dir().join(sub_dir).join(expected);
            let mut expected_decompressed = read_first_flat_layer_from_file(decompressed_path)
                .expect("uncompressed image could not be loaded");

            // HACK: make metadata match artificially, to avoid failing the check due to meta data mismatch
            // (the name of the compression methods should not be equal, as we test between compression methods)
            expected_decompressed.layer_data.encoding.compression = Compression::Uncompressed;
            decompressed.layer_data.encoding.compression = Compression::Uncompressed;

            debug_assert_eq!(expected_decompressed.layer_data.attributes, decompressed.layer_data.attributes, "attributes should not be affected by compression");
            debug_assert_eq!(expected_decompressed.layer_data.size, decompressed.layer_data.size, "size should not be affected by compression");

            // Note: Unimplemented methods may still work, if each compressed tile would be larger than uncompressed.
            expected_decompressed.assert_equals_result(&decompressed);
        }
    }
}

fn expect_eq_png(image_name: &str) {
    let exr_path = dir().join("u8").join(image_name);
    let png_from_exr = read_first_rgba_layer_from_file(
        exr_path,

        |resolution, _channels: &RgbaChannels| -> ::image::RgbImage {
            ::image::ImageBuffer::new(
                resolution.width() as u32,
                resolution.height() as u32
            )
        },

        // set each pixel in the png buffer from the exr file
        |png_pixels: &mut ::image::RgbImage, position: Vec2<usize>, (r,g,b,_): (f32,f32,f32,f32)| {
            png_pixels.put_pixel(
                position.x() as u32, position.y() as u32,
                ::image::Rgb([to_u8(r), to_u8(g), to_u8(b)])
            );
        }
    );

    fn to_u8(num: f32) -> u8 { (num.powf(1.0/2.2).clamp(0.0, 1.0) * 255.0).round() as u8 }

    match png_from_exr {
        Err(Error::NotSupported(message)) => println!("skipping ({})", message),
        Err(error) => panic!("unexpected error: {}", error),
        Ok(decompressed) => {
            let truth_path = dir().join("u8").join("ground_truth.png");
            let truth_dyn_img = image::open(truth_path).unwrap();
            dbg!(truth_dyn_img.color());

            let ground_truth_png = truth_dyn_img.to_rgb8();
            let exr_as_png_px = decompressed.layer_data.channel_data.pixels;
            debug_assert_eq!(ground_truth_png.dimensions(), exr_as_png_px.dimensions(), "size should not be affected by compression");

            let expected_px = ground_truth_png.pixels()
                .flat_map(|px| px.0.iter().copied());

            let actual_px = exr_as_png_px.pixels()
                .flat_map(|px| px.0.iter().copied());

            for (exp, val) in expected_px.zip(actual_px) {
                assert!(exp.abs_diff(val) < 8);
            }
        }
    }
}

fn expect_eq_uncompressed(sub_dir: &str, image_name: &str) {
    expect_eq_other(sub_dir, image_name, "uncompressed.exr")
}


#[test]
fn compare_compression_contents_zip_f32() {
    expect_eq_uncompressed("f32", "zip.exr");
}

#[test]
fn compare_compression_contents_zip_f16() {
    expect_eq_uncompressed("f16", "zip.exr");
}

#[test]
fn compare_compression_contents_zips_f32() {
    expect_eq_uncompressed("f32", "zips.exr");
}

#[test]
fn compare_compression_contents_zips_f16() {
    expect_eq_uncompressed("f16", "zips.exr");
}

#[test]
fn compare_compression_contents_b44_f32() {
    expect_eq_uncompressed("f32", "b44.exr"); // f32s are not compressed in b44 and can be compared exactly
}

#[test]
fn compare_compression_contents_b44_f16() {
    expect_eq_other("f16", "b44.exr", "decompressed_b44.exr");
}

#[test]
fn compare_compression_contents_b44a_f32() {
    expect_eq_uncompressed("f32", "b44a.exr"); // f32s are not compressed in b44 and can be compared exactly
}

#[test]
fn compare_compression_contents_b44a_f16() {
    expect_eq_other("f16", "b44a.exr", "decompressed_b44a.exr");
}

#[test]
fn compare_compression_contents_dwaa_f32() {
    expect_eq_other("f32", "dwaa.exr", "decompressed_dwaa.exr");
}

#[test]
fn compare_compression_contents_dwaa_f16() {
    expect_eq_other("f16", "dwaa.exr", "decompressed_dwaa.exr");
}

#[test]
fn compare_compression_contents_dwab_f32() {
    expect_eq_other("f32", "dwab.exr", "decompressed_dwab.exr");
}

#[test]
fn compare_compression_contents_dwab_f16() {
    expect_eq_other("f16", "dwab.exr", "decompressed_dwab.exr");
}

#[test]
fn compare_compression_contents_piz_f32() {
    expect_eq_uncompressed("f32", "piz.exr");
}

#[test]
fn compare_compression_contents_piz_f16() {
    expect_eq_uncompressed("f16", "piz.exr");
}

#[test]
fn compare_compression_contents_rle_f32() {
    expect_eq_uncompressed("f32", "rle.exr");
}

#[test]
fn compare_compression_contents_rle_f16() {
    expect_eq_uncompressed("f16", "rle.exr");
}



#[test]
fn compare_png_to_uncompressed_f16() {
    expect_eq_png("f16_uncompressed.exr");
}

#[test]
fn compare_png_to_piz_f16() {
    expect_eq_png("f16_piz.exr");
}

#[test]
fn compare_png_to_rle_f16() {
    expect_eq_png("f16_rle.exr");
}

#[test]
fn compare_png_to_zip_f16() {
    expect_eq_png("f16_zip.exr");
}

#[test]
fn compare_png_to_zips_f16() {
    expect_eq_png("f16_zips.exr");
}




#[test]
fn compare_png_to_uncompressed_f32() {
    expect_eq_png("f32_uncompressed.exr");
}

#[test]
fn compare_png_to_piz_f32() {
    expect_eq_png("f32_piz.exr");
}

#[test]
fn compare_png_to_rle_f32() {
    expect_eq_png("f32_rle.exr");
}

#[test]
fn compare_png_to_zip_f32() {
    expect_eq_png("f32_zip.exr");
}

#[test]
fn compare_png_to_zips_f32() {
    expect_eq_png("f32_zips.exr");
}