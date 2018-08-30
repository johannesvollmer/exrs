
use ::file::meta::Header;
//use ::std::io::{Read, Seek, SeekFrom};
use ::smallvec::SmallVec;
use ::file::data::uncompressed::*;
//use ::file::attributes::{Text};
//use ::file::chunks::TileCoordinates;


/// any openexr image, loads all available data immediately into memory
/// can be constructed using `::file::File`
pub struct Image {
    pub version: ::file::meta::Version,
    pub parts: Parts, // TODO HashMap<Text, Part> ?
}

pub type Parts = SmallVec<[Part; 3]>;

pub struct Part {
    pub header: Header,

    /// only the data for this single part,
    /// index can be computed from pixel location and block_kind.
    /// one part can only have one block_kind, not a different kind per block
    ///
    /// That Vec contains one entry per mip map level, or only one if it does not have any,
    /// or a row-major flattened vector of all rip maps like
    /// 1x1, 1x2, 1x4, 1x8, and then
    /// 2x1, 2x2, 2x4, 2x8, and then
    /// 4x1, 4x2, 4x4, 4x8, and then
    /// 8x1, 8x2, 8x4, 8x8.
    pub levels: Vec<PartData>

    // offset tables are already processed while loading 'data'
    // TODO skip reading offset tables if not required?
}

/// one `type` per Part
pub enum PartData {
    /// One single array containing all pixels, row major left to right, top to bottom
    /// same length as `Part.channels` field
    Flat(PerChannel<Array>),

    /// scan line blocks are stored from top to bottom, row major.
    DeepScanLine(PerChannel<Vec<DeepScanLineBlock>>),

    /// Blocks are stored from top left to bottom right, row major.
    DeepTile(PerChannel<Vec<DeepTileBlock>>),
}



use ::file::meta::MetaData;
use ::file::data::compressed::Chunks;
use ::file::io::*;

#[must_use]
pub fn read_file(path: &::std::path::Path) -> ReadResult<Image> {
    buffered_read(::std::fs::File::open(path)?)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn buffered_read<R: Read + Seek>(unbuffered: R) -> ReadResult<Image> {
    read_seekable_buffered(&mut SeekBufRead::new(unbuffered))
}

/// assumes that `Read` is buffered
#[must_use]
pub fn read_seekable_buffered<R: Read + Seek>(read: &mut R) -> ReadResult<Image> {
    let MetaData { version, headers, offset_tables } = MetaData::read(read)?;

    // TODO parallel decompressing
    let chunks = Chunks::read(read, &meta_data)?;

    Ok(Image {
        version,
        parts: headers.into_iter().zip(offset_tables.into_iter())
            .map(|header, table|{
                Part {
                    header,
                    levels: unimplemented!(),
                }
            })
            .collect(),
    })
}


#[must_use]
pub fn write_file(path: &str, image: &Image) -> WriteResult {
    write(&mut ::std::fs::File::open(path)?, image)
}

#[must_use]
pub fn write<W: Write>(write: &mut W, image: &Image) -> WriteResult {
    // image.meta_data.write(write)?;
    // image.chunks.write(write, &image.meta_data)
    unimplemented!()
}





