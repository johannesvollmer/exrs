extern crate exr;

extern crate smallvec;

use exr::image::full::*;
use std::{panic};
use std::io::{Cursor};
use std::panic::catch_unwind;
use std::path::{PathBuf, Path};
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use exr::image::{read_options, write_options, simple, rgba};

fn exr_files() -> impl Iterator<Item=PathBuf> {
    walkdir::WalkDir::new("tests/images/valid").into_iter().map(std::result::Result::unwrap)
        .filter(|entry| entry.path().extension() == Some(OsStr::new("exr")))
        .map(walkdir::DirEntry::into_path)
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

            match &result {
                Result::Panic => println!("✗ Panic when processing {:?}", file),
                _ => println!("✓ No Panic when processing {:?}", file)
            };

            (file, result)
        })
        .collect();

    results.sort_by(|(_, a), (_, b)| a.cmp(b));

    println!("{:#?}", results.iter().map(|(path, result)| {
        format!("{:?}: {}", result, path.to_str().unwrap())
    }).collect::<Vec<_>>());

    assert!(results.len() >= 100, "Not all files were tested!");
    assert_ne!(results.last().unwrap().1, Result::Panic, "A file triggered a panic");
}

#[test]
fn round_trip_all_files_full() {
    check_files(|path| {
        let image = Image::read_from_file(path, read_options::low())?;

        let mut tmp_bytes = Vec::new();
        image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options::low())?;

        let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), read_options::low())?;
        if !path.to_str().unwrap().to_lowercase().contains("nan") {
            assert_eq!(image, image2);
        }

        Ok(())
    })
}

#[test]
fn round_trip_all_files_simple() {
    check_files(|path| {
        let image = simple::Image::read_from_file(path, read_options::low())?;

        let mut tmp_bytes = Vec::new();
        image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options::low())?;

        let image2 = simple::Image::read_from_buffered(Cursor::new(&tmp_bytes), read_options::low())?;
        if !path.to_str().unwrap().to_lowercase().contains("nan") {
            assert_eq!(image, image2);
        }

        Ok(())
    })
}

#[test]
fn round_trip_all_files_rgba() {
    check_files(|path| {
        let (image, pixels) = rgba::ImageInfo::read_pixels_from_file(
            path, read_options::low(),
            rgba::pixels::create_flattened_f16,
            rgba::pixels::flattened_pixel_setter()
        )?;

        let mut tmp_bytes = Vec::new();
        image.write_pixels_to_buffered(
            &mut Cursor::new(&mut tmp_bytes), write_options::low(),
            rgba::pixels::flattened_pixel_getter(&pixels)
        )?;

        let (image2, pixels2) = rgba::ImageInfo::read_pixels_from_buffered(
            Cursor::new(&tmp_bytes), read_options::low(),
            rgba::pixels::create_flattened_f16,
            rgba::pixels::flattened_pixel_setter()
        )?;

        if !path.to_str().unwrap().to_lowercase().contains("nan") {
            assert_eq!(image, image2);
            assert_eq!(pixels, pixels2);
        }

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

        if !path.to_str().unwrap().to_lowercase().contains("nan") {
            assert_eq!(image, image2);
        }

        Ok(())
    })
}


