
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
    }
}


pub mod file;
pub mod image;
pub mod error;

#[macro_use]
extern crate smallvec;

#[cfg(test)]
extern crate image as piston_image;


// TODO various compiler tweaks, such as export RUSTFLAGS='-Ctarget-cpu=native'

pub mod prelude {
    // image functions
    pub use crate::image::{
        read_from_file as read_file,
        read_from_unbuffered as read
    };

    // meta functions
    pub use crate::image::meta::{
        read_from_file as read_file_meta,
        read_from_unbuffered as read_meta
    };

    // data types
    pub use crate::file::meta::{ MetaData, attributes::{ Attribute, AttributeValue, Text }, Header };
    pub use crate::image::{Image, Channel, ChannelData, SampleMaps, Levels, RipMaps, SampleBlock, DeepSamples, FlatSamples };
    pub use crate::error::{ ReadResult, WriteResult, ReadError, WriteError };

    // re export external stuff
    pub use std::path::Path;
    pub use half::f16;
}


#[cfg(test)]
pub mod test {
    use crate::prelude::*;
    use crate::image::Part;

    #[test]
    fn print_meta_of_all_files() {
        fn print_exr_files(path: &Path){
            if let Some("exr") = path.extension().and_then(|os| os.to_str()) {
                print!("inspecting file {:?}:   ", path.file_name().unwrap());
                let (meta, _read) = read_file_meta(path).unwrap();
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
                println!("testing file {:?}... ", path.file_name().unwrap());
                let image = read_file(path, true);
                if let Err(error) = image {
                    eprintln!("{:?}", error);
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
    pub fn convert_to_png() {
        let now = ::std::time::Instant::now();

        let path = Path::new(
//            "D:/Pictures/openexr/BeachBall/multipart.0001.exr"
//            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"
        "D:/Pictures/openexr/crowskull/crow_zips.exr"
//            "D:/Pictures/openexr/crowskull/crow_rle.exr"
//            "D:/Pictures/openexr/crowskull/crow_zip_half.exr"


//        "D:/Pictures/openexr/v2/Stereo/Trunks.exr" // deep data, stereo
        );

        let image = read_file(path, true).unwrap();

        // warning: highly unscientific benchmarks ahead!
        let elapsed = now.elapsed();
        let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;
        println!("\ndecoded file in {:?} s", millis as f32 * 0.001);

        let part: &Part = &image.parts[0];
        let content: &ChannelData = &part.channels[1].content;

        match content {
            ChannelData::F16(levels) => {
                expect_variant!(levels, SampleMaps::Flat(levels) => {
                    let sample_block : &SampleBlock<FlatSamples<f16>> = levels.largest();

                    let mut png_buffer = ::piston_image::GrayImage::new(sample_block.resolution.0 as u32, sample_block.resolution.1 as u32);

                    for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
                        let v = sample_block.samples[(y * sample_block.resolution.0 as u32 + x) as usize];
                        *pixel = ::piston_image::Luma([(v.to_f32().powf(1.0/2.2) * 100.0) as u8]);
                    }

                    png_buffer.save(Path::new("test.png")).unwrap();
                })
            },
            ChannelData::F32(levels) => {
                expect_variant!(levels, SampleMaps::Flat(levels) => {
                    let sample_block : &SampleBlock<FlatSamples<f32>> = levels.largest();

                    let mut png_buffer = ::piston_image::GrayImage::new(sample_block.resolution.0 as u32, sample_block.resolution.1 as u32);

                    for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
                        let v = sample_block.samples[(y * sample_block.resolution.0 as u32 + x) as usize];
                        *pixel = ::piston_image::Luma([(v.powf(1.0/2.2) * 100.0) as u8]);
                    }

                    png_buffer.save(Path::new("test.png")).unwrap();
                })
            },
            _ => unimplemented!()
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
