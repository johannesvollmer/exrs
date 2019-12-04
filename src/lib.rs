
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

#[macro_use]
extern crate smallvec;

#[cfg(test)]
extern crate image as piston_image;


// TODO various compiler tweaks, such as export RUSTFLAGS='-Ctarget-cpu=native'

pub mod prelude {
//    pub use crate::file::io::read_file;
    pub use crate::file::io::ReadError;

//    pub use crate::file::io::write_file;
    pub use crate::file::io::WriteError;



}



#[cfg(test)]
pub mod test {

//    #[bench]
//    fn load_meta_only(){
//        let meta_data = MetaData::read(read)?;
//    }


    use ::std::path::Path;

    #[test]
    fn read_all_files() {

        // TODO test if reading pushed the reader to the very end of the file?

        fn test_exr_files(path: &Path){
            if let Some("exr") = path.extension().and_then(|os| os.to_str()) {
                print!("testing file {:?}... ", path.file_name().unwrap());
                load_file_or_print_err(path)

            } else if path.is_dir() {
                for sub_dir in ::std::fs::read_dir(path).unwrap() {
                    test_exr_files(&sub_dir.unwrap().path());
                }
            }
        }

        test_exr_files(Path::new("D:/Pictures/openexr"))
    }

    fn load_file_or_print_err(path: &Path){
        let image = crate::image::immediate::read_raw_data(path);

        match image {
            Ok((meta, _)) => println!("{:#?}", meta),
            Err(error) => println!("{:?}", error),
        }
//        println!("{:?}", image.map(|(meta, chunks)| format!("{:#?}", meta)));
        //println!("{}", ::image::immediate::read_file(path).map(|_| "no errors").unwrap());
    }


    #[test]
    fn convert_to_png() {
        let now = ::std::time::Instant::now();

        let path = ::std::path::Path::new(
//            "D:/Pictures/openexr/ScanLines/Blobbies.exr" FIXME
            "D:/Pictures/openexr/crowskull/crow_uncompressed.exr"


//            "D:/Pictures/openexr/debug/32x32_r10g5b0_f16_rle.exr"
//            "/home/johannes/Pictures/openexr/samuel-zeller/samuel_zeller_rgb_f16_rle.exr"
        );

        let (meta, chunks) = crate::image::immediate::read_raw_data(&path).unwrap();
        println!("\nmeta: {:#?}", meta);


        let parts = crate::image::immediate::decode_content(meta, chunks).unwrap();


        // warning: highly unscientific benchmarks ahead!
        let elapsed = now.elapsed();
        let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;

        assert_eq!(parts.len(), 1);
        let part = &parts[0];
        println!("header_0: {:#?}", part.header);


        println!("\ndecoded file in {:?} ms", millis);

        let header = &part.header;
        let channels = part.levels.full();
        let full_res = header.data_window.dimensions();

        let mut png_buffer = ::piston_image::GrayImage::new(full_res.0, full_res.1);

        // BUGHUNT CHECKLIST
        // - [x] rust-f16 encoding is the same as openexr-f16 encoding
        // - [x] compression+unpacking vs unpacking+compression order
        // - [ ] compression alrogithm
        // - [ ] mixing channels up, interleaving channels, in uncompressed::unpack
        // - [ ] unpacking/reconstruction c algorithms translation into rust


        // actually do the conversion to png
        expect_variant!(channels, crate::image::data::PartData::Flat(ref channels) => {
            expect_variant!(channels[1], crate::file::data::uncompressed::Array::F32(ref channel) => {
                for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
                // TODO assumes channel is not subsampled
                    let v = channel[(y * full_res.0 + x) as usize];
                    *pixel = ::piston_image::Luma([(v.powf(1.0/2.2) * 100.0) as u8]);
                }
            })
        });

        png_buffer.save(path.with_extension("png").file_name().unwrap()).unwrap();
    }

    // TODO allow loading only meta data,
    // TODO and allow seek-loading tiles based on offset tables afterwards

    // TODO check for completeness of file
    // TODO handle incomplete files based on if the offset_table is complete (last thing written)
    // TODO memory-mapping

    // TODO let the user decide how to store something,
    // don't just read the pixels into a buffer and let the user convert the data into new data again
    // in order to avoid too much memory allocations
    // (something like  read_pixels(|index, pixel| pixels[index] = RGBA::new(pixel[0], pixel[1], ...) )
}
