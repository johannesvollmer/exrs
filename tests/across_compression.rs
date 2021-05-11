use std::path::Path;
use exr::prelude::*;
use exr::image::validate_results::ValidateResult;


fn compare_compression_contents(sub_dir: &str, image_name: &str ) {

    let dir = Path::new("tests/images/valid/custom/compression_methods");

    let uncompressed_path = dir.join(sub_dir).join("uncompressed.exr");
    let mut uncompressed = read_first_flat_layer_from_file(uncompressed_path)
        .expect("uncompressed image could not be loaded");

    let path = dir.join(sub_dir).join(image_name);
    print!("{}/{}: ", sub_dir, image_name);

    match read_first_flat_layer_from_file(path) {
        Err(Error::NotSupported(message)) => println!("skipping ({})", message),
        Err(error) => panic!("unexpected error: {}", error),
        Ok(decompressed) => {

            // HACK: make metadata match artificially, to avoid failing the check due to meta data mismatch
            uncompressed.layer_data.encoding = decompressed.layer_data.encoding;

            debug_assert_eq!(uncompressed.layer_data.attributes, decompressed.layer_data.attributes, "attributes should not be affected by compression");
            debug_assert_eq!(uncompressed.layer_data.size, decompressed.layer_data.size, "size should not be affected by compression");

            // Note: Unimplemented methods may still work, if each compressed tile would be larger than uncompressed.
            println!("checking {} for equality to uncompressed data", decompressed.layer_data.encoding.compression);
            uncompressed.assert_equals_result(&decompressed);
        }
    }
}

#[test]
fn compare_compression_contents_zip_f32() {
    compare_compression_contents( "f32", "zip.exr");
}

#[test]
fn compare_compression_contents_zip_f16() {
    compare_compression_contents( "f16", "zip.exr");
}

#[test]
fn compare_compression_contents_zips_f32() {
    compare_compression_contents( "f32", "zips.exr");
}

#[test]
fn compare_compression_contents_zips_f16() {
    compare_compression_contents( "f16", "zips.exr");
}

#[test]
fn compare_compression_contents_b44_f32() {
    compare_compression_contents( "f32", "b44.exr");
}

#[test]
fn compare_compression_contents_b44_f16() {
    compare_compression_contents( "f16", "b44.exr");
}

#[test]
fn compare_compression_contents_b44a_f32() {
    compare_compression_contents( "f32", "b44a.exr");
}

#[test]
fn compare_compression_contents_b44a_f16() {
    compare_compression_contents( "f16", "b44a.exr");
}

#[test]
fn compare_compression_contents_dwaa_f32() {
    compare_compression_contents( "f32", "dwaa.exr");
}

#[test]
fn compare_compression_contents_dwaa_f16() {
    compare_compression_contents( "f16", "dwaa.exr");
}

#[test]
fn compare_compression_contents_dwab_f32() {
    compare_compression_contents( "f32", "dwab.exr");
}

#[test]
fn compare_compression_contents_dwab_f16() {
    compare_compression_contents( "f16", "dwab.exr");
}

#[test]
fn compare_compression_contents_piz_f32() {
    compare_compression_contents( "f32", "piz.exr");
}

#[test]
fn compare_compression_contents_piz_f16() {
    compare_compression_contents( "f16", "piz.exr");
}

#[test]
fn compare_compression_contents_rle_f32() {
    compare_compression_contents( "f32", "rle.exr");
}

#[test]
fn compare_compression_contents_rle_f16() {
    compare_compression_contents( "f16", "rle.exr");
}
