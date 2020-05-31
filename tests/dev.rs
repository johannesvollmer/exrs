//! Contains some "test" functions that were be used for developing.

extern crate exr;
extern crate smallvec;

use exr::prelude::common::*;

use std::path::{PathBuf};
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::io;
use std::io::{Write, Cursor};
use exr::image::rgba;
use exr::meta::header::Header;

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
            header.own_attributes.preview.is_some() || header.own_attributes.custom.values()
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
    // let path = "tests/images/valid/openexr/IlmfmlmflmTest/test_native1.exr";
    let path = "tests/images/valid/openexr/TestImages/AllHalfValues.exr";
    // let path = "tests/images/valid/custom/crowskull/crow_rle.exr";

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let meta = MetaData::read_from_file(path, false).unwrap();
    println!("{:#?}", meta);

    let (image_info, pixels) = rgba::ImageInfo::read_pixels_from_file(
        path, read_options::low(),
        rgba::pixels::create_flattened_f16,
        rgba::pixels::flattened_pixel_setter()
    ).unwrap();

    println!("...read 1 successfull");

    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    image_info.write_pixels_to_buffered(
        &mut Cursor::new(&mut tmp_bytes), write_options::low(),
        rgba::pixels::flattened_pixel_getter(&pixels)
    ).unwrap();

    println!("...write successfull: {}mb", tmp_bytes.len() as f32/ 1000000.0);

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let (image_info_2, pixels2) = rgba::ImageInfo::read_pixels_from_buffered(
        Cursor::new(&tmp_bytes),
        read_options::low(),
        rgba::pixels::create_flattened_f16,
        rgba::pixels::flattened_pixel_setter()
    ).unwrap();

    println!("...read 2 successfull");


    assert_eq!(image_info, image_info_2);

    // custom compare function: considers nan equal to nan
    assert!(pixels.samples.iter().map(|f| f.to_bits()).eq(pixels2.samples.iter().map(|f| f.to_bits())));
}
