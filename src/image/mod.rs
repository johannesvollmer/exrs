//! The `image` module is for interpreting the loaded file data.
//!

pub mod immediate;
pub mod late;

pub use crate::file::meta::MetaData;

pub mod meta {

    use std::io::{Read, Seek};
    use crate::file::meta::MetaData;
    use crate::file::io::ReadResult;
    pub use ::seek_bufread::BufReader as SeekBufRead;

    #[must_use]
    pub fn read_file(path: &::std::path::Path) -> ReadResult<MetaData> {
        read(::std::fs::File::open(path)?)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn read<R: Read + Seek>(unbuffered: R) -> ReadResult<MetaData> {
        read_seekable_prebuffered(&mut SeekBufRead::new(unbuffered))
    }

    /// assumes that the provided reader is buffered
    #[must_use]
    pub fn read_seekable_prebuffered<R: Read + Seek>(buffered: &mut R) -> ReadResult<MetaData> {
        MetaData::read(buffered)
    }
}


pub mod data {
    use smallvec::SmallVec;
    use crate::file::meta::Header;
    use crate::file::data::uncompressed::{PerChannel, Array, DeepScanLineBlock};

    /// an exr image can store multiple parts (multiple bitmaps inside one image)
    pub type Parts = SmallVec<[Part; 3]>;

    pub struct Part {
        pub header: Header,

        /// only the data for this single part,
        /// index can be computed from pixel location and block_kind.
        /// one part can only have one block_kind, not a different kind per block
        /// number of x and y levels can be computed using the header
        ///
        /// That Vec contains one entry per mip map level, or only one if it does not have any,
        /// or a row-major flattened vector of all rip maps like
        /// 1x1, 2x1, 4x1, 8x1, and then
        /// 1x2, 2x2, 4x2, 8x2, and then
        /// 1x4, 2x4, 4x4, 8x4, and then
        /// 1x8, 2x8, 4x8, 8x8.
        ///
        // FIXME should be descending and starting with full-res instead!
        pub levels: Levels

        // offset tables are already processed while loading 'data'
        // TODO skip reading offset tables if not required?
    }

    pub enum Levels {
        Singular(PartData),
        Mip(SmallVec<[PartData; 16]>),
        Rip(RipMaps)
    }

    pub struct RipMaps {
        data: Vec<PartData>,
        x_levels: usize,
        y_levels: usize,
    }

    /// one `type` per Part
    pub enum PartData {
        /// One single array containing all pixels, row major left to right, top to bottom
        /// same length as `Part.channels` field
        // TODO should store sampling_x/_y for simple accessors?
        Flat(PerChannel<Array>),

        /// scan line blocks are stored from top to bottom, row major.
        Deep/*ScanLine*/(PerChannel<Vec<DeepScanLineBlock>>),

        // /// Blocks are stored from top left to bottom right, row major.
        // DeepTile(PerChannel<Vec<DeepTileBlock>>),
    }



    impl Levels {
        pub fn full(&self) -> &PartData {
            match *self {
                Levels::Singular(ref data) => data,
                Levels::Mip(ref data) => &data[0],
                Levels::Rip(ref rip_map) => &rip_map.data[0], // TODO test!
            }
        }
    }

}



