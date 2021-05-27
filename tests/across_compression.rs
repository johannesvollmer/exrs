use std::path::Path;
use exr::prelude::*;
use exr::image::validate_results::ValidateResult;

#[test]
fn compare_compression_contents(){
    println!("comparing pixels compressed with all compression methods...");
    let dir = Path::new("tests/images/valid/custom/compression_methods");

    for sub_dir in &["f32", "f16"] {
        let uncompressed_path = dir.join(sub_dir).join("uncompressed.exr");
        let mut uncompressed = read_first_flat_layer_from_file(uncompressed_path)
            .expect("uncompressed image could not be loaded");

        for image_name in &[
            "zip.exr", "zips.exr", "b44.exr", "b44a.exr",
            "dwaa.exr", "dwab.exr", "piz.exr", "rle.exr"
        ]{
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
    }

}