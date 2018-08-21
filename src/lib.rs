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

    #[test]
    fn main() {
        let image = ::decode::read_file(
            "/home/johannes/Pictures/openexr/multipart.0005.exr"
        );

        println!("{:?}", image);
    }

    // TODO allow loading only meta data,
    // TODO and allow seek-loading tiles based on offset tables afterwards
}
