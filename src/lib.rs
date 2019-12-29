#![forbid(unsafe_code)]
#![deny(clippy::all)]
// TODO #![warn(missing_docs)]


pub mod io;
pub mod chunks;
pub mod compression;
pub mod meta;
pub mod image;
pub mod error;

#[macro_use]
extern crate smallvec;

#[cfg(test)]
extern crate image as piston_image;


// TODO various compiler tweaks, such as export RUSTFLAGS='-Ctarget-cpu=native'

pub mod prelude {
    // main exports
    pub use crate::image::Image;
    pub use crate::meta::MetaData;

    // core data types
    pub use crate::image::{
        ReadOptions, WriteOptions, TileOptions,
        Channel, ChannelData, SampleMaps, Levels, RipMaps, SampleBlock, DeepSamples, FlatSamples, Samples
    };

    // secondary data types
    pub use crate::meta;
    pub use crate::meta::attributes;
    pub use crate::error;

    // re-export external stuff
    pub use std::path::Path;
    pub use half::f16;
}


#[cfg(test)]
pub mod test {
    use crate::prelude::*;
    use crate::image::{ReadOptions};
    use std::{fs, panic};
    use std::io::Cursor;
    use std::panic::catch_unwind;
    use std::path::PathBuf;
    use std::ffi::OsStr;
    use rayon::iter::{IntoParallelIterator, ParallelIterator};
    use crate::compression::Compression;

    fn exr_files() -> impl Iterator<Item=PathBuf> {
        walkdir::WalkDir::new("D:\\Pictures\\openexr").into_iter()
            .map(Result::unwrap).filter(|entry| entry.path().extension() == Some(OsStr::new("exr")))
            .map(walkdir::DirEntry::into_path)
    }

    #[test]
    fn print_meta_of_all_files() {
        fn print_exr_files(path: &Path){
            if let Some("exr") = path.extension().and_then(|os| os.to_str()) {
                print!("inspecting file {:?}:   ", path.file_name().unwrap());
                let meta = MetaData::read_from_file(path).unwrap();
                println!("{:?} {:?}", meta.requirements, meta.headers);
            }
            else if path.is_dir() {
                for sub_dir in ::std::fs::read_dir(path).unwrap() {
                    print_exr_files(&sub_dir.unwrap().path());
                }
            }
        }

        print_exr_files(Path::new("D:/Pictures/openexr"))
    }

    /// read all images in a directory.
    /// does not check any content, just checks whether a read error or panic happened.
    #[test]
    fn read_all_files() {
        #[derive(Debug, Eq, PartialEq, Ord, PartialOrd)]
        enum Result { Ok, Err, Panic };

        let files: Vec<PathBuf> = exr_files().collect();
        let mut results: Vec<(PathBuf, Result)> = files.into_par_iter()
            .map(|file| {
                let image = catch_unwind(||{ // FIXME does not catch errors from other thread?
                    let prev_hook = panic::take_hook();
                    panic::set_hook(Box::new(|_| (/* do not println panics */)));
                    let image = Image::read_from_file(&file, ReadOptions::debug());
                    panic::set_hook(prev_hook);

                    image
                });

                let result = match image {
                    Ok(Ok(_)) => Result::Ok,
                    Ok(Err(_)) => Result::Err,
                    Err(_) => Result::Panic,
                };

                (file, result)
            })
            .collect();

        results.sort_by(|(_, a), (_, b)| a.cmp(b));

        println!("{:#?}", results.iter().map(|(path, result)| {
            format!("{:?}: {}", result, path.file_name().unwrap().to_str().unwrap())
        }).collect::<Vec<_>>());
    }


    // TODO check for completeness of file
    // TODO handle incomplete files based on if the offset_table is complete (last thing written)
    // TODO memory-mapping

    // TODO let the user decide how to store something,
    // don't just read the pixels into a buffer and let the user convert the data into new data again
    // in order to avoid too much memory allocations
    // (something like  read_pixels(|index, pixel| pixels[index] = RGBA::new(pixel[0], pixel[1], ...) )


    #[test]
    pub fn test_roundtrip() {
        let path = Path::new(
//            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"  // FIXME attempts to sub with overflow in parrallel mode
            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
        );

        let image = Image::read_from_file(path, ReadOptions::debug()).unwrap();
        println!("read 1 successfull, beginning write");

        let write_options = WriteOptions {
            compression_method: Compression::ZIP1,
            .. WriteOptions::debug()
        };

        let mut tmp_bytes = Vec::new();
        image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), write_options).unwrap();
        println!("write successfull, beginning read 2");

        let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), ReadOptions::debug()).unwrap();
        println!("read 2 successfull");

        assert_eq!(image, image2);
        println!("equal");
    }

    #[test]
    pub fn test_write_file() {
        let path = Path::new(
//            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"  // FIXME attempts to sub with overflow in parrallel mode
//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
"D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
        );

        let image = Image::read_from_file(path, ReadOptions::debug()).unwrap();

        let write_options = WriteOptions {
            compression_method: Compression::ZIP1,
            .. WriteOptions::debug()
        };

        image.write_to_file(Path::new("./testout/written.exr"), write_options).unwrap();
    }

    #[test]
    pub fn convert_to_png() {
        let now = ::std::time::Instant::now();

        let path = Path::new(
            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"  // FIXME attempts to sub with overflow in parrallel mode
//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crows/kull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//            "D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
        );

        let image = Image::read_from_file(path, ReadOptions::default()).unwrap();

        // warning: highly unscientific benchmarks ahead!
        let elapsed = now.elapsed();
        let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;
        println!("\ndecoded file in {:?}s", millis as f32 * 0.001);


        fn save_f32_image_as_png(data: &[f32], size: (usize, usize), name: String) {
            let mut png_buffer = ::piston_image::GrayImage::new(size.0 as u32, size.1 as u32);
            let min = data.iter().cloned().fold(0.0/0.0, f32::max);
            let max = data.iter().cloned().fold(1.0/0.0, f32::min);

            for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
                let v = data[(y * size.0 as u32 + x) as usize];
                let v = (v - min) / (max - min);
                *pixel = ::piston_image::Luma([(v * 255.0) as u8]);
            }

            png_buffer.save(Path::new(&name)).unwrap();
        }

        fs::remove_dir_all("testout").unwrap();
        fs::create_dir("testout").unwrap();

        for part in &image.parts {
            for channel in &part.channels {
                match &channel.content {
                    ChannelData::F16(levels) => {
                        let levels = levels.flat_samples().unwrap();
                        for sample_block in levels.levels() {
                            let data : Vec<f32> = sample_block.samples.iter().map(|f16| f16.to_f32()).collect();

                            save_f32_image_as_png(&data, sample_block.resolution, format!(
                                "testout/{}_{}_f16_{}x{}.png",
                                part.name.as_ref().map(attributes::Text::to_string).unwrap_or(String::from("1")),
                                channel.name,
                                sample_block.resolution.0,
                                sample_block.resolution.1,
                            ))
                        }
                    },
                    ChannelData::F32(levels) => {
                        let levels = levels.flat_samples().unwrap();
                        for sample_block in levels.levels() {
                            save_f32_image_as_png(&sample_block.samples, sample_block.resolution, format!(
                                "testout/{}_{}_f16_{}x{}.png",
                                part.name.as_ref().map(attributes::Text::to_string).unwrap_or(String::from("1")),
                                channel.name,
                                sample_block.resolution.0,
                                sample_block.resolution.1,
                            ))
                        }
                    },
                    _ => unimplemented!()
                }
            }
        }
    }

}
