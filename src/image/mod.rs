
pub mod full;

use crate::image::full::FullImage;
use crate::io::*;
use crate::meta::*;
use crate::meta::attributes::*;
use crate::compression::{Compression};
use crate::error::{ReadResult, WriteResult};
use crate::math::*;
use std::io::{BufReader, BufWriter, Seek, Cursor};


#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {
    pub parallel_compression: bool,
    pub compression_method: Compression,
    pub line_order: LineOrder,
    pub tiles: TileOptions
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TileOptions {
    ScanLineBlocks,
    Tiles {
        size: (u32, u32),
        rounding: RoundingMode
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReadOptions {
    pub parallel_decompression: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions {
            parallel_compression: true,
            compression_method: Compression::RLE,
            line_order: LineOrder::IncreasingY,
            tiles: TileOptions::ScanLineBlocks
        }
    }
}

impl WriteOptions {
    pub fn debug() -> Self {
        WriteOptions {
            parallel_compression: false,
            compression_method: Compression::Uncompressed,
            line_order: LineOrder::IncreasingY,
            tiles: TileOptions::ScanLineBlocks
        }
    }
}

impl ReadOptions {
    pub fn debug() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions {
            parallel_decompression: true
        }
    }
}


impl TileOptions {
    pub fn has_tiles(&self) -> bool {
        match self {
            TileOptions::Tiles { .. } => true,
            _ => false
        }
    }
}

#[must_use]
pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions) -> ReadResult<FullImage> {
    self::read_from_unbuffered(std::fs::File::open(path)?, options)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn read_from_unbuffered(unbuffered: impl Read, options: ReadOptions) -> ReadResult<FullImage> {
    self::read_from_buffered(BufReader::new(unbuffered), options)
}

/// assumes the reader is buffered (if desired)
#[must_use]
pub fn read_from_buffered(read: impl Read, options: ReadOptions) -> ReadResult<FullImage> {
    let mut read = PeekRead::new(read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let offset_tables = MetaData::read_offset_tables(&mut read, meta_data.requirements, &meta_data.headers)?;
    self::read_data_by_meta(meta_data, offset_tables, &mut read, options)
}


/// assumes the reader is buffered (if desired)
#[must_use]
pub fn read_data_by_meta(meta_data: MetaData, offset_tables: OffsetTables, read: &mut impl Read, options: ReadOptions) -> ReadResult<FullImage> {
    FullImage::read_data_by_meta(meta_data, offset_tables, read, options)
}


#[must_use]
pub fn write_to_file(image: &FullImage, path: impl AsRef<std::path::Path>, options: WriteOptions) -> WriteResult {
    self::write_to_unbuffered(image, std::fs::File::create(path)?, options)
}

/// needs more memory but allows for non-seeking write operations
#[must_use]
pub fn write_without_seek(image: &FullImage, mut unbuffered: impl Write, options: WriteOptions) -> WriteResult {
    let mut bytes = Vec::new();

    // write the image to the seekable vec
    self::write_to_buffered(image, Cursor::new(&mut bytes), options)?;

    // write the vec into the actual output
    unbuffered.write_all(&bytes)?;
    Ok(())
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn write_to_unbuffered(image: &FullImage, unbuffered: impl Write + Seek, options: WriteOptions) -> WriteResult {
    self::write_to_buffered(image, BufWriter::new(unbuffered), options)
}

/// assumes the reader is buffered
#[must_use]
pub fn write_to_buffered(image: &FullImage, write: impl Write + Seek, options: WriteOptions) -> WriteResult {
    FullImage::write_to_buffered(image, write, options)
}