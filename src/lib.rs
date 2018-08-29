pub mod file;
pub mod image;

extern crate seek_bufread;
extern crate libflate;
extern crate bit_field;
extern crate byteorder;
extern crate smallvec;
extern crate half;

// TODO various compiler tweaks, such as export RUSTFLAGS='-Ctarget-cpu=native'

pub mod prelude {
    pub use file::io::read_file;
    pub use file::io::ReadError;

    pub use file::io::write_file;
    pub use file::io::WriteError;

    pub use file::meta::MetaData;
    pub use file::File;
}


#[cfg(test)]
pub mod test {

//    #[bench]
//    fn load_meta_only(){
//      TODO
//    }



    #[test]
    fn test_all_files() {
        use ::std::path::Path;
        use ::prelude::*;

        // TODO test if reading pushed the reader to the very end of the file?

        fn test_exr_files(path: &Path){
            if let Some("exr") = path.extension().and_then(|os| os.to_str()) {
                print!("testing file {:?}... ", path.file_name().unwrap());
                println!("{}", read_file(path).map(|_| "no errors").unwrap());

            } else if path.is_dir() {
                for sub_dir in ::std::fs::read_dir(path).unwrap() {
                    test_exr_files(&sub_dir.unwrap().path());
                }
            }
        }

        test_exr_files(::std::path::Path::new("/home/johannes/Pictures/openexr"))
    }

    #[test]
    fn print_version_and_headers() {
        use std::time::Instant;
        use ::prelude::*;

        let now = Instant::now();

        let image = read_file(::std::path::Path::new(
            "/home/johannes/Pictures/openexr/openexr-images-master/MultiResolution/ColorCodedLevels.exr"
        ));

        // warning: highly unscientific benchmarks ahead!
        let elapsed = now.elapsed();
        let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;

        if let Ok(image) = image {
            println!("header_0 channels: {:#?}", image.meta_data.headers[0].channels());
            println!("\nversion: {:#?}", image.meta_data.version);
            println!("\ndecoded file in {:?} ms", millis);

        } else {
            println!("Error: {:?}", image);
        }
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
