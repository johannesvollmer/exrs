use std::path::Path;
use exr::prelude::*;
use exr::image::validate_results::ValidateResult;


fn expect_eq_other(sub_dir: &str, image_name: &str, expected_image: &str) {
    let dir = Path::new("tests/images/valid/custom/compression_methods");

    let decompressed_path = dir.join(sub_dir).join(expected_image);
    let mut expected_decompressed = read_first_flat_layer_from_file(decompressed_path)
        .expect("uncompressed image could not be loaded");

    let path = dir.join(sub_dir).join(image_name);
    print!("{}/{}: ", sub_dir, image_name);

    match read_first_flat_layer_from_file(path) {
        Err(Error::NotSupported(message)) => println!("skipping ({})", message),
        Err(error) => panic!("unexpected error: {}", error),
        Ok(mut decompressed) => {

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
