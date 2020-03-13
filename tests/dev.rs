extern crate exr;

extern crate smallvec;

use exr::prelude::*;
use exr::image::full::*;
use std::path::{PathBuf};
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use exr::meta::{MetaData, Header};

fn exr_files() -> impl Iterator<Item=PathBuf> {
    walkdir::WalkDir::new("D:\\Pictures\\openexr").into_iter().map(std::result::Result::unwrap)
        .filter(|entry| entry.path().extension() == Some(OsStr::new("exr")))
        .map(walkdir::DirEntry::into_path)
}

#[test]
fn print_meta_of_all_files() {
    let files: Vec<PathBuf> = exr_files().collect();

    files.into_par_iter().for_each(|path| {
        let meta = MetaData::read_from_file(&path);
        println!("{:?}: \t\t\t {:?}", path.file_name().unwrap(), meta.unwrap());
    });
}

#[test]
fn search_previews_of_all_files() {
    let files: Vec<PathBuf> = exr_files().collect();

    files.into_par_iter().for_each(|path| {
        let meta = MetaData::read_from_file(&path).unwrap();
        let has_preview = meta.headers.iter().any(|header: &Header|
            header.own_attributes.preview.is_some() || header.own_attributes.custom.values()
                .find(|value| value.to_preview().is_ok()).is_some()
        );

        if has_preview {
            println!("Found preview attribute in {:?}", path.file_name().unwrap());
        }
    });
}


#[test]
pub fn test_write_file() {
    let path =
        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"

//            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"
//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//"D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    let image = Image::read_from_file(path, read_options::high()).unwrap();
    Image::write_to_file(&image, "./testout/written.exr", write_options::high()).unwrap();
}

