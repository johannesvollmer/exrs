extern crate exr;

extern crate smallvec;

use exr::prelude::*;
use exr::image::full::*;
use std::{fs, panic, io};
use std::io::{Cursor, Write};
use std::panic::catch_unwind;
use std::path::{PathBuf, Path};
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use exr::math::Vec2;
use std::cmp::Ordering;

fn exr_files() -> impl Iterator<Item=PathBuf> {
    walkdir::WalkDir::new("D:\\Pictures\\openexr").into_iter()
        .map(Result::unwrap).filter(|entry| entry.path().extension() == Some(OsStr::new("exr")))
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
            let result = catch_unwind(||{ // FIXME does not catch errors from other thread?
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

#[test]
fn read_all_files() {
    check_files(|path| Image::read_from_file(path, ReadOptions::debug()))
}

#[test]
fn round_trip_all_files() {
    check_files(|path| {
        let image = Image::read_from_file(path, ReadOptions::debug())?;
        let write_options = WriteOptions::debug();

        let mut tmp_bytes = Vec::new();
        image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options)?;

        let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), ReadOptions::debug())?;

        assert_eq!(image, image2);

        let mut tmp_bytes2 = Vec::new();
        image2.write_to_buffered(&mut Cursor::new(&mut tmp_bytes2), write_options)?;

        assert_eq!(tmp_bytes, tmp_bytes2);
        Ok(())
    })
}


#[test]
fn loop_read() {
    let path =
//        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"
            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//"D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"

//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
        ;

    let bytes = fs::read(path).unwrap();

    println!("starting loop...");

    for _ in 0..1024 {
        let image = Image::read_from_buffered(bytes.as_slice(), ReadOptions::debug()).unwrap();
        bencher::black_box(image);
    }

    println!("finished");
}

#[test]
pub fn test_roundtrip() {
    let path =

//        "D:/Pictures/openexr/TestImages/BrightRingsNanInf.exr"
        "D:/Pictures/openexr/Tiles/Ocean.exr"
//        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"
//            "D:/Pictures/openexr/v2/Stereo/composited.exr"
//            "D:/Pictures/openexr/MultiResolution/Bonita.exr"

//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//"D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let image = Image::read_from_file(path, ReadOptions::fast_loading()).unwrap();
    println!("...read 1 successfull");

    let write_options = WriteOptions::debug();
    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options).unwrap();
    println!("...write successfull");

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), ReadOptions::fast_loading()).unwrap();
    println!("...read 2 successfull");

    assert_eq!(image, image2);
}

#[test]
pub fn test_write_file() {
    let path =
        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"

//            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"  // FIXME attempts to sub with overflow in parrallel mode
//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//"D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    let image = Image::read_from_file(path, ReadOptions::debug()).unwrap();

    let write_options = WriteOptions {
        .. WriteOptions::debug()
    };

    Image::write_to_file(&image, "./testout/written.exr", write_options).unwrap();
}

#[test]
pub fn convert_to_png() {
    let now = ::std::time::Instant::now();

    let path =
        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"
//        "D:/Pictures/openexr/Tiles/Ocean.exr"
//        "D:/Pictures/openexr/MultiResolution/Kapaa.exr"
//        "D:/Pictures/openexr/MultiView/Impact.exr"
//        "D:/Pictures/openexr/MultiResolution/KernerEnvCube.exr"
//        "D:/Pictures/openexr/MultiResolution/Bonita.exr"


//            "D:/Pictures/openexr/MultiResolution/Bonita.exr"

//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//            "D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    let image = Image::read_from_file(path, ReadOptions::debug()).unwrap();

    // warning: highly unscientific benchmarks ahead!
    let elapsed = now.elapsed();
    let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;
    println!("\ndecoded file in {:?}s", millis as f32 * 0.001);

    fn save_f32_image_as_png(data: &[f32], size: Vec2<usize>, name: String) {
        let mut png_buffer = image::GrayImage::new(size.0 as u32, size.1 as u32);
        let mut sorted = Vec::from(data);
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Less));

        // sixth percentile normalization
        let max = sorted[7 * sorted.len() / 8];
        let min = sorted[1 * sorted.len() / 8];

        let tone = |v: f32| (v - 0.5).tanh() * 0.5 + 0.5;
        let max_toned = tone(*sorted.last().unwrap());
        let min_toned = tone(*sorted.first().unwrap());

        for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
            let v = data[(y as usize * size.0 + x as usize)];
            let v = (v - min) / (max - min);
            let v = tone(v);

            let v = (v - min_toned) / (max_toned - min_toned);
            *pixel = image::Luma([(v.max(0.0).min(1.0) * 255.0) as u8]);
        }

        png_buffer.save(&name).unwrap();
    }

    fs::remove_dir_all("testout").unwrap_or_default();
    fs::create_dir("testout").unwrap();

    for (part_index, part) in image.parts.iter().enumerate() {
        for channel in &part.channels {
            match &channel.content {
                ChannelData::F16(levels) => {
                    let levels = levels.as_flat_samples().unwrap();
                    for sample_block in levels.as_slice() {
                        let data : Vec<f32> = sample_block.samples.iter().map(|f16| f16.to_f32()).collect();

                        save_f32_image_as_png(&data, sample_block.resolution, format!(
                            "testout/{} ({}) {}_f16_{}x{}.png",
                            part_index,
                            part.name.as_ref().map(attributes::Text::to_string).unwrap_or(String::from("1")),
                            channel.name,
                            sample_block.resolution.0,
                            sample_block.resolution.1,
                        ))
                    }
                },
                ChannelData::F32(levels) => {
                    let levels = levels.as_flat_samples().unwrap();
                    for sample_block in levels.as_slice() {
                        save_f32_image_as_png(&sample_block.samples, sample_block.resolution, format!(
                            "testout/{} ({}) {}_f16_{}x{}.png",
                            part_index,
                            part.name.as_ref().map(attributes::Text::to_string).unwrap_or(String::from("1")),
                            channel.name,
                            sample_block.resolution.0,
                            sample_block.resolution.1,
                        ))
                    }
                },
                _ => panic!()
            }
        }
    }
}

