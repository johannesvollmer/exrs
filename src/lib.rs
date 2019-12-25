/*
#[macro_use]
pub mod util {
    macro_rules! expect_variant {
        ($value: expr, $variant: pat => $then: expr) => {
            if let $variant = $value {
                $then

            } else {
                panic!("Expected variant `{}` in `{}`", stringify!($variant), stringify!($value))
            }
        };

        ($value: expr, $variant: pat) => {
            match $value {
                $variant => value,
                _ => panic!("Expected value in variant `{}` in `{}`", stringify!($variant), stringify!($value))
            }
        }
    }
}*/


pub mod file;
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
    pub use crate::file::meta::MetaData;

    // core data types
    pub use crate::image::{
        ReadOptions, WriteOptions, TileOptions,
        Channel, ChannelData, SampleMaps, Levels, RipMaps, SampleBlock, DeepSamples, FlatSamples, Samples
    };

    // secondary data types
    pub use crate::file::meta;
    pub use crate::file::meta::attributes;
    pub use crate::error;

    // re-export external stuff
    pub use std::path::Path;
    pub use half::f16;
}


#[cfg(test)]
pub mod test {
    use crate::prelude::*;
    use crate::image::{ReadOptions};
    use std::fs;
    use std::io::Cursor;
    use std::panic::catch_unwind;

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

    #[test]
    fn read_all_files() {
        fn test_exr_files(path: &Path){
            if let Some("exr") = path.extension().and_then(|os| os.to_str()) {
                print!("testing file {:?}... ", path.file_name().unwrap());
                let image = catch_unwind(||{ // FIXME does not catch errors from other thread
                    Image::read_from_file(path, ReadOptions::default())
                });

                match image {
                    Ok(Ok(_)) => println!("Ok"),
                    Ok(Err(_)) => eprintln!("Error"),
                    Err(_) => eprintln!("Panic")
                }
            }
            else if path.is_dir() {
                for sub_dir in ::std::fs::read_dir(path).unwrap() {
                    test_exr_files(&sub_dir.unwrap().path());
                }
            }
        }

        test_exr_files(Path::new("D:/Pictures/openexr"))
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
//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
"D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
        );

        let image = Image::read_from_file(path, ReadOptions::default()).unwrap();

        let mut tmp_bytes = Vec::new();
        image.write_to_buffered(&mut Cursor::new(&mut tmp_bytes), WriteOptions::default()).unwrap();
        let image2 = Image::read_from_buffered(&mut tmp_bytes.as_slice(), ReadOptions::default()).unwrap();
    }

    #[test]
    pub fn convert_to_png() {
        let now = ::std::time::Instant::now();

        let path = Path::new(
//            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"  // FIXME attempts to sub with overflow in parrallel mode
//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
//        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
            "D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
        );

        let image = Image::read_from_file(path, ReadOptions::default()).unwrap();

        // warning: highly unscientific benchmarks ahead!
        let elapsed = now.elapsed();
        let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;
        println!("\ndecoded file in {:?}s", millis as f32 * 0.001);


        fn save_f16_as_png(data: &[f32], size: (usize, usize), name: String) {
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

                            save_f16_as_png(&data, sample_block.resolution, format!(
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
                            save_f16_as_png(&sample_block.samples, sample_block.resolution, format!(
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

        // expect_variant!(channels, crate::image::PartData::Flat(ref pixels) => {

//            match pixels.channel_data[1] {
//                PixelArray::F32(ref channel) => {
//                    for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
//                        let v = channel[(y * pixels.dimensions.0 + x) as usize];
//                        *pixel = ::piston_image::Luma([(v.powf(1.0/2.2) * 100.0) as u8]);
//                    }
//                },
//                PixelArray::F16(ref channel) => {
//                    for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
//                        let v = channel[(y * pixels.dimensions.0 + x) as usize];
//                        *pixel = ::piston_image::Luma([(v.to_f32().powf(1.0/2.2) * 100.0) as u8]);
//                    }
//                },
//                _ => panic!()
//            }

        // });

    }

}
