//! Contains some "test" functions that were be used for developing.

extern crate exr;
extern crate smallvec;

use std::path::{PathBuf};
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use exr::meta::{MetaData, Header};
use std::io;
use exr::prelude::*;
use std::io::{Write, Cursor};

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
        let meta = MetaData::read_from_file(&path);
        println!("{:?}: \t\t\t {:?}", path.file_name().unwrap(), meta.unwrap());
    });
}

#[test]
#[ignore]
fn search_previews_of_all_files() {
    let files: Vec<PathBuf> = exr_files().collect();

    files.into_par_iter().for_each(|path| {
        let meta = MetaData::read_from_file(&path).unwrap();
        let has_preview = meta.headers.iter().any(|header: &Header|
            header.own_attributes.preview.is_some() || header.own_attributes.custom.values()
                .any(|value| value.to_preview().is_ok())
        );

        if has_preview {
            println!("Found preview attribute in {:?}", path.file_name().unwrap());
        }
    });
}

#[test]
#[ignore]
pub fn test_roundtrip() {
    let path = "tests/images/valid/custom/crowskull/crow_piz_noisy_rgb.exr";

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let meta = MetaData::read_from_file(path).unwrap();
    println!("{:#?}", meta);

    let (image, pixels) = rgba::ImageInfo::read_pixels_from_file(path, read_options::low(), rgba::pixels::flattened_f16).unwrap();
    println!("...read 1 successfull");

    let write_options = write_options::low();
    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    image.write_pixels_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options, &pixels).unwrap();
    println!("...write successfull");

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let (image2, pixels2) = rgba::ImageInfo::read_pixels_from_buffered(
        Cursor::new(&tmp_bytes),
        read_options::low(),
        rgba::pixels::flattened_f16
    ).unwrap();

    println!("...read 2 successfull");

    if !path.to_lowercase().contains("nan") {
        assert_eq!(image, image2);
        assert_eq!(pixels, pixels2);
    }
}
