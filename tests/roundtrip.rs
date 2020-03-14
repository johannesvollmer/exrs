extern crate exr;

extern crate smallvec;

use exr::image::full::*;
use std::{panic, io};
use std::io::{Cursor, Write};
use std::panic::catch_unwind;
use std::path::{PathBuf, Path};
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use exr::image::{read_options, write_options};
use exr::meta::MetaData;

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

/// read all images in a directory.
/// does not check any content, just checks whether a read error or panic happened.
fn check_files<T>(operation: impl Sync + std::panic::RefUnwindSafe + Fn(&Path) -> exr::error::Result<T>) {
    #[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
    enum Result { Ok, Error(String), Panic };

    let files: Vec<PathBuf> = exr_files().collect();
    let mut results: Vec<(PathBuf, Result)> = files.into_par_iter()
        .map(|file| {
            let result = catch_unwind(||{
                let prev_hook = panic::take_hook();
                panic::set_hook(Box::new(|_| (/* do not println panics */)));
                let result = operation(&file);
                panic::set_hook(prev_hook);

                result
            });

            let result = match result {
                Ok(Ok(_)) => Result::Ok,
                Ok(Err(error)) => Result::Error(format!("{:?}", error)),
                Err(_) => Result::Panic,
            };

            (file, result)
        })
        .collect();

    results.sort_by(|(_, a), (_, b)| a.cmp(b));

    println!("{:#?}", results.iter().map(|(path, result)| {
        format!("{:?}: {}", result, path.to_str().unwrap())
    }).collect::<Vec<_>>());
}

/// Read all files without checking anything
#[test]
fn read_all_files() {
    check_files(|path| Image::read_from_file(path, read_options::low()))
}

#[test]
fn round_trip_all_files() {
    check_files(|path| {
        let image = Image::read_from_file(path, read_options::low())?;

        let mut tmp_bytes = Vec::new();
        image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options::low())?;

        let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), read_options::low())?;
        assert_eq!(image, image2);

        Ok(())
    })
}

#[test]
fn round_trip_parallel_files() {
    check_files(|path| {
        let image = Image::read_from_file(path, read_options::high())?;

        let mut tmp_bytes = Vec::new();
        image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options::high())?;

        let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), read_options::high())?;

        assert_eq!(image, image2);
        Ok(())
    })
}


#[test]
pub fn test_roundtrip() {
    let path =

//        "D:/Pictures/openexr/TestImages/BrightRingsNanInf.exr"
//         "D:/Pictures/openexr/Tiles/Ocean.exr"
//        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"
//            "D:/Pictures/openexr/v2/Stereo/composited.exr"
//            "D:/Pictures/openexr/MultiResolution/Bonita.exr"

           "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//"D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let image = Image::read_from_file(path, read_options::high()).unwrap();
    println!("...read 1 successfull");

    let write_options = write_options::high();
    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options).unwrap();
    println!("...write successfull");

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), read_options::high()).unwrap();
    println!("...read 2 successfull");

    assert_eq!(image, image2);
}

