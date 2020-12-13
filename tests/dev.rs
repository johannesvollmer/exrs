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
use exr::image::read::{read_all_rgba_layers_from_file};

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
    // let path = "tests/images/valid/custom/crowskull/crow_zip_half.exr";
    // let path = "tests/images/valid/openexr/Beachball/multipart.0001.exr";
    // let path = "tests/images/valid/openexr/Tiles/GoldenGate.exr";
    // let path = "tests/images/valid/openexr/v2/Stereo/composited.exr";
    // let path = "tests/images/valid/openexr/MultiView/Balls.exr";

    // worksn't (probably because of Mipmaps and Ripmaps?)
    // let path = "tests/images/valid/openexr/MultiResolution/Kapaa.exr"; // rip maps
    // let path = "tests/images/valid/openexr/MultiView/Impact.exr"; // mip maps
    let path = "tests/images/valid\\openexr\\Chromaticities\\Rec709_YC.exr"; // mip maps

    /*
    "Error(\"Panic\"): tests/images/valid\\custom\\crowskull\\crow_dwa.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\Chromaticities\\Rec709_YC.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\Chromaticities\\XYZ_YC.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\IlmfmlmflmTest\\comp_b44.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\IlmfmlmflmTest\\comp_dwaa_v1.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\IlmfmlmflmTest\\comp_dwaa_v2.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\IlmfmlmflmTest\\comp_dwab_v1.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\IlmfmlmflmTest\\comp_dwab_v2.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\LuminanceChroma\\CrissyField.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\LuminanceChroma\\Flowers.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\LuminanceChroma\\MtTamNorth.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\LuminanceChroma\\StarField.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\MultiView\\Adjuster.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\ScanLines\\Cannon.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LeftView\\Balls.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LeftView\\Ground.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LeftView\\Leaves.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LeftView\\Trunks.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LowResLeftView\\Balls.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LowResLeftView\\Ground.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LowResLeftView\\Leaves.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\LowResLeftView\\Trunks.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\Stereo\\Balls.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\Stereo\\Ground.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\Stereo\\Leaves.exr",
    "Error(\"Panic\"): tests/images/valid\\openexr\\v2\\Stereo\\Trunks.exr",
    */

    // deep data?
    // let path = "tests/images/valid/openexr/v2/Stereo/Balls.exr";
    // let path = "tests/images/valid/openexr/v2/Stereo/Ground.exr";

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let meta = MetaData::read_from_file(path, false).unwrap();
    println!("{:#?}", meta);

    let image =
        read_all_rgba_layers_from_file(
            path,
            read::rgba_channels::pixels::create_flattened_f32,
            read::rgba_channels::pixels::set_flattened_pixel
        ).unwrap();

        // read_all_data_from_file(path).unwrap();
        // read() // Image::read_from_file(path, read_options::low()).unwrap();
        // .no_deep_data().all_resolution_levels().all_channels().all_layers().pedantic().non_parallel()
        // .read_from_file(path).unwrap();

    println!("...read 1 successfull");

    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    // image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options::low()).unwrap();
    image.write().non_parallel().to_buffered(&mut Cursor::new(&mut tmp_bytes)).unwrap();

    println!("...write successfull: {}mb", tmp_bytes.len() as f32 / 1000000.0);

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let image2 =
        read_all_rgba_layers_from_file(
            path,
            read::rgba_channels::pixels::create_flattened_f32,
            read::rgba_channels::pixels::set_flattened_pixel
        ).unwrap();

        // read() // Image::read_from_buffered(Cursor::new(&tmp_bytes),ReadOptions { pedantic: true, .. read_options::low() }).unwrap();
        // .no_deep_data().all_resolution_levels().all_channels().all_layers().pedantic()
        // .read_from_buffered(Cursor::new(&tmp_bytes))
        // .unwrap();

    println!("...read 2 successfull");

    assert!(!image.contains_nan_pixels() && !image2.contains_nan_pixels());
    assert_eq!(image, image2);
}
