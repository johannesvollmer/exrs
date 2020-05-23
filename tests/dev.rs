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
pub fn read_piz_images() {
    let images: Vec<PathBuf> = exr_files().filter(|file| {
        let meta = MetaData::read_from_file(file.as_path(), true).unwrap();
        meta.headers.iter().any(|header: &Header| header.compression == Compression::PIZ)
    }).collect();

    for image in images {
        use exr::prelude::simple_image::*;
        println!();

        let meta = MetaData::read_from_file(&image, true).unwrap();
        // println!("any non-half channels: {:?}", meta.headers.iter()
        //     .any(|header: &Header| header.channels.list.iter().any(|channel| channel.sample_type != SampleType::F16)));

        // println!("header count: {}", meta.headers.len());

        let result = Image::read_from_file(&image, read_options::low())
            .map(|_| "Success!");

        if let Err(Error::NotSupported(_)) = result {
            continue;
        }

        debug_assert_eq!(meta.headers.len(), 1);
        let header: &Header = &meta.headers[0];

        // println!("block mode: {:?}", header.blocks);
        println!("{:?}", header.max_block_pixel_size());
        println!("{:?}", header.max_block_byte_size());


        println!("{:?}: {:?}", result, image)
    }
}

#[test]
pub fn test_roundtrip() {
    // let path = "tests/images/valid/custom/crowskull/crow_piz_noisy_rgb.exr"; //     ERROR: invalid code
    let path = "tests/images/valid/openexr/TestImages/GammaChart.exr"; //   76.800        First read works, but second read produces ERROR: less data than expected
    // let path = "tests/images/valid/openexr/Tiles/GoldenGate.exr"; //  works
    // let path = "tests/images/valid/custom/crowskull/crow_pxr24.exr"; //             ERROR: more data than expected
    // let path = "tests/images/valid/custom/crowskull/crow_rle.exr"; //               ERROR: more data than expected
    // let path = "tests/images/valid/custom/crowskull/crow_zip_half.exr"; //   245.760       ERROR: more data than expected
    // let path = "tests/images/valid/custom/crowskull/crow_piz.exr";

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let meta = MetaData::read_from_file(path, false).unwrap();
    println!("{:#?}", meta);

    let (mut image, pixels) = rgba::ImageInfo::read_pixels_from_file(
        path, read_options::low(),
        rgba::pixels::create_flattened_f16,
        rgba::pixels::flattened_pixel_setter()
    ).unwrap();
    println!("...read 1 successfull");

    image.encoding.compression = Compression::PIZ;
    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    image.write_pixels_to_buffered(
        &mut Cursor::new(&mut tmp_bytes), write_options::low(),
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
