extern crate exr;


use exr::prelude::*;
use exr::image::full::*;
use exr::image::ReadOptions;
use std::{fs, panic, io};
use std::io::{Cursor, Write};
use std::panic::catch_unwind;
use std::path::PathBuf;
use std::ffi::OsStr;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use exr::compression::Compression;

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
#[test]
fn read_all_files() {
    #[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
    enum Result { Ok, Error(String), Panic };

    let files: Vec<PathBuf> = exr_files().collect();
    let mut results: Vec<(PathBuf, Result)> = files.into_par_iter()
        .map(|file| {
            let image = catch_unwind(||{ // FIXME does not catch errors from other thread?
                let prev_hook = panic::take_hook();
                panic::set_hook(Box::new(|_| (/* do not println panics */)));
                let image = FullImage::read_from_file(&file, ReadOptions::debug());
                panic::set_hook(prev_hook);

                image
            });

            let result = match image {
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
        let image = FullImage::read_from_buffered(bytes.as_slice(), ReadOptions::debug()).unwrap();
        bencher::black_box(image);
    }

    println!("finished");
}

#[test]
pub fn test_roundtrip() {
    let path =
//            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"
            "D:/Pictures/openexr/MultiResolution/Bonita.exr"

//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//"D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    print!("starting read 1... ");
    io::stdout().flush().unwrap();

    let image = FullImage::read_from_file(path, ReadOptions::debug()).unwrap();
    println!("...read 1 successfull");

    let write_options = WriteOptions {
//        compression_method: Compression::ZIP16,
        compression_method: Compression::Uncompressed,
        blocks: BlockOptions::ScanLineBlocks,
//        tiles: BlockOptions::TileBlocks { size: (128, 128), rounding: RoundingMode::Down },
        .. WriteOptions::debug()
    };

    let mut tmp_bytes = Vec::new();

    print!("starting write... ");
    io::stdout().flush().unwrap();

    image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options).unwrap();
    println!("...write successfull");

    print!("starting read 2... ");
    io::stdout().flush().unwrap();

    let image2 = FullImage::read_from_buffered(&mut tmp_bytes.as_slice(), ReadOptions::debug()).unwrap();
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

    let image = FullImage::read_from_file(path, ReadOptions::debug()).unwrap();

    let write_options = WriteOptions {
        compression_method: Compression::ZIP1,
        .. WriteOptions::debug()
    };

    FullImage::write_to_file(&image, "./testout/written.exr", write_options).unwrap();
}

#[test]
pub fn convert_to_png() {
    let now = ::std::time::Instant::now();

    let path =
        "D:/Pictures/openexr/BeachBall/multipart.0001.exr"  // FIXME attempts to sub with overflow in parrallel mode


//            "D:/Pictures/openexr/MultiResolution/Bonita.exr"

//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//            "D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
    ;

    let image = FullImage::read_from_file(path, ReadOptions::debug()).unwrap();

    // warning: highly unscientific benchmarks ahead!
    let elapsed = now.elapsed();
    let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;
    println!("\ndecoded file in {:?}s", millis as f32 * 0.001);


    fn save_f32_image_as_png(data: &[f32], size: (usize, usize), name: String) {
        let mut png_buffer = image::GrayImage::new(size.0 as u32, size.1 as u32);
        let min = data.iter().cloned().fold(0.0/0.0, f32::max);
        let max = data.iter().cloned().fold(1.0/0.0, f32::min);

        for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
            let v = data[(y * size.0 as u32 + x) as usize];
            let v = (v - min) / (max - min);
            *pixel = image::Luma([(v * 255.0) as u8]);
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

