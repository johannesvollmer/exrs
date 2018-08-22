pub mod file;
pub mod image;

extern crate seek_bufread;
extern crate compression;
extern crate bit_field;
extern crate byteorder;
extern crate smallvec;
extern crate half;

pub mod prelude {
    pub use file::decode::read_file;
    pub use file::decode::Error;

    pub use file::MetaData;
    pub use file::RawImage;
}


#[cfg(test)]
pub mod test {

//    #[bench]
//    fn load_meta_only(){
//      TODO
//    }

    #[test]
    fn print_version_and_headers() {
        use std::time::Instant;
        use ::prelude::*;

        let now = Instant::now();

        let image = read_file(
            // "/home/johannes/Pictures/openexr/openexr-images-master/Beachball/multipart.0001.exr"
            // "/home/johannes/Pictures/openexr/openexr-images-master/DisplayWindow/t01.exr"
            // "/home/johannes/Pictures/openexr/openexr-images-master/LuminanceChroma/Flowers.exr"
            // "/home/johannes/Pictures/openexr/openexr-images-master/MultiResolution/StageEnvCube.exr"
            // "/home/johannes/Pictures/openexr/openexr-images-master/MultiView/Balls.exr" // large file
            // "/home/johannes/Pictures/openexr/openexr-images-master/ScanLines/StillLife.exr"
            // "/home/johannes/Pictures/openexr/openexr-images-master/Tiles/Spirals.exr"
            "/home/johannes/Pictures/openexr/openexr-images-master/Tiles/Spirals.exr"
        );

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
}
