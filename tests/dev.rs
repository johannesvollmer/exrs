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
pub fn dev_roundtrip() {
    // let path = "tests/images/valid/openexr/TestImages/GammaChart.exr";
    let path = "tests/images/valid/custom/crowskull/crow_pxr24.exr";

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let meta = MetaData::read_from_file(path, false).unwrap();
    println!("{:#?}", meta);

    let (image, pixels) = rgba::ImageInfo::read_pixels_from_file(
        path, read_options::low(),
        rgba::pixels::create_flattened_f16,
        rgba::pixels::flattened_pixel_setter()
    ).unwrap();
    println!("...read 1 successfull");

    let write_options = write_options::low();
    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    image.write_pixels_to_buffered(
        &mut Cursor::new(&mut tmp_bytes), write_options,
        rgba::pixels::flattened_pixel_getter(&pixels)
    ).unwrap();

    println!("...write successfull: {}mb", tmp_bytes.len() as f32/ 1000000.0);

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let (image2, pixels2) = rgba::ImageInfo::read_pixels_from_buffered(
        Cursor::new(&tmp_bytes),
        read_options::low(),
        rgba::pixels::create_flattened_f16,
        rgba::pixels::flattened_pixel_setter()
    ).unwrap();

    println!("...read 2 successfull");

    if !path.to_lowercase().contains("nan") {
        assert_eq!(image, image2);
        assert_eq!(pixels, pixels2);
    }
}
