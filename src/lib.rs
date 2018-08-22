pub mod decode;
pub mod attributes;
pub mod blocks;
pub mod compress;
pub mod file;

extern crate seek_bufread;
extern crate compression;
extern crate bit_field;
extern crate byteorder;
extern crate smallvec;
extern crate half;



#[cfg(test)]
pub mod test {

//    #[bench]
//    fn load_meta_only(){
//      TODO
//    }

    #[test]
    fn print_version_and_headers() {
        use std::time::{Duration, Instant};
        let now = Instant::now();

        let image = ::decode::read_file(
            "/home/johannes/Pictures/openexr/multipart.0005.exr"
        );

        // warning: highly unscientific benchmarks ahead!
        let elapsed = now.elapsed();
        let millis = elapsed.as_secs() * 1000 + elapsed.subsec_millis() as u64;

        if let Ok(image) = image {
            println!("headers: {:#?}", image.meta_data.headers);
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
}
