//! Contains some "test" functions that were be used for developing.

extern crate exr;
extern crate smallvec;

use exr::prelude::*;

use std::path::{PathBuf};
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::io;
use std::io::{Write, Cursor};
use exr::meta::header::Header;
use exr::image::pixel_vec::PixelVec;
use exr::image::validate_results::ValidateResult;

fn exr_files() -> impl Iterator<Item=PathBuf> {
    walkdir::WalkDir::new("tests/images/valid").into_iter().map(std::result::Result::unwrap)
        .filter(|entry| entry.path().extension() == Some(OsStr::new("exr")))
        .map(walkdir::DirEntry::into_path)
}

#[test]
#[ignore]
fn print_meta_of_all_files() {
    let files: Vec<PathBuf> = exr_files().collect();

    files.into_par_iter().for_each(|path| {
        let meta = MetaData::read_from_file(&path, false);
        println!("{:?}: \t\t\t {:?}", path.file_name().unwrap(), meta.unwrap());
    });
}

#[test]
#[ignore]
fn search_previews_of_all_files() {
    let files: Vec<PathBuf> = exr_files().collect();

    files.into_par_iter().for_each(|path| {
        let meta = MetaData::read_from_file(&path, false).unwrap();
        let has_preview = meta.headers.iter().any(|header: &Header|
            header.own_attributes.preview.is_some() || header.own_attributes.other.values()
                .any(|value| match value { AttributeValue::Preview(_) => true, _ => false })
        );

        if has_preview {
            println!("Found preview attribute in {:?}", path.file_name().unwrap());
        }
    });
}

#[test]
#[ignore]
pub fn test_roundtrip() {
    // works
    // let path = "tests/images/valid/custom/crowskull/crow_piz.exr";
    // let path = "tests/images/valid/custom/crowskull/crow_dwa.exr";
     let path = "tests/images/valid/custom/crowskull/crow_zip_half.exr";
    // let path = "tests/images/valid/openexr/Beachball/multipart.0001.exr";
    // let path = "tests/images/valid/openexr/Tiles/GoldenGate.exr";
    // let path = "tests/images/valid/openexr/v2/Stereo/composited.exr";
    // let path = "tests/images/valid\\custom\\crowskull\\crow_piz.exr";
    //let path = "tests/images/valid\\openexr\\IlmfmlmflmTest\\v1.7.test.1.exr";
    // let path = "tests/images/valid/openexr/MultiView/Balls.exr";
    // let path = "tests/images/valid/openexr/MultiResolution/Kapaa.exr"; // rip maps
    // let path = "tests/images/valid/openexr/MultiView/Impact.exr"; // mip maps

    // worksn't
    // let path = "tests/images/valid/openexr/Chromaticities/Rec709_YC.exr"; // subsampling
    // let path = "tests/images/valid/openexr/LuminanceChroma/Flowers.exr"; // subsampling

    // let path = "tests/images/valid/openexr/IlmfmlmflmTest/test_native1.exr";
    // let path = "tests/images/valid/openexr/IlmfmlmflmTest/test_native2.exr"; // contains NaN

    // deep data?
    // let path = "tests/images/valid/openexr/v2/Stereo/Balls.exr";
    // let path = "tests/images/valid/openexr/v2/Stereo/Ground.exr";

    let read_image = read()
        .no_deep_data().all_resolution_levels().all_channels().all_layers().all_attributes()
        .non_parallel();

    let image = read_image.clone().from_file(path).unwrap();

    let mut tmp_bytes = Vec::new();
    image.write().to_buffered(Cursor::new(&mut tmp_bytes)).unwrap();

    let image2 = read_image.from_buffered(Cursor::new(tmp_bytes)).unwrap();

    image.assert_equals_result(&image2);
}
