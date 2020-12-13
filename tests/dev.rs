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
    // let path = "tests/images/valid/custom/crowskull/crow_zip_half.exr";
    // let path = "tests/images/valid/openexr/Beachball/multipart.0001.exr";
    // let path = "tests/images/valid/openexr/Tiles/GoldenGate.exr";
    // let path = "tests/images/valid/openexr/v2/Stereo/composited.exr";
    // let path = "tests/images/valid/openexr/MultiView/Balls.exr";

    // worksn't (probably because of Mipmaps and Ripmaps?)
    let path = "tests/images/valid/openexr/MultiResolution/Kapaa.exr";
    // let path = "tests/images/valid/openexr/MultiView/Impact.exr";

    // deep data?
    // let path = "tests/images/valid/openexr/v2/Stereo/Balls.exr";
    // let path = "tests/images/valid/openexr/v2/Stereo/Ground.exr";

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let meta = MetaData::read_from_file(path, false).unwrap();
    println!("{:#?}", meta);

    let image = read() // Image::read_from_file(path, read_options::low()).unwrap();
        .no_deep_data().all_resolution_levels().all_channels().all_layers().pedantic().non_parallel()
        .read_from_file(path).unwrap();

    println!("...read 1 successfull");

    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    // image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options::low()).unwrap();
    image.write().non_parallel().to_buffered(&mut Cursor::new(&mut tmp_bytes)).unwrap();

    println!("...write successfull: {}mb", tmp_bytes.len() as f32 / 1000000.0);

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let image2 = read() // Image::read_from_buffered(Cursor::new(&tmp_bytes),ReadOptions { pedantic: true, .. read_options::low() }).unwrap();
        .no_deep_data().all_resolution_levels().all_channels().all_layers().pedantic()
        .read_from_buffered(Cursor::new(&tmp_bytes))
        .unwrap();

    println!("...read 2 successfull");

    assert!(!image.contains_nan_pixels() && !image2.contains_nan_pixels());
    assert_eq!(image, image2);
}
