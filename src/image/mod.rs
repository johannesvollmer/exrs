
pub mod full;

use crate::image::full::FullImage;
use crate::io::*;
use crate::meta::*;
use crate::meta::attributes::*;
use crate::compression::{Compression};
use crate::error::{Result, PassiveResult};
use crate::math::*;
use std::io::{BufReader, BufWriter, Seek, Cursor};


#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct WriteOptions {
    pub parallel_compression: bool,
    pub compression_method: Compression,
    pub line_order: LineOrder,
    pub tiles: BlockOptions
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum BlockOptions {
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


// TODO also return the corresponding WriteOptions which can be used to write the most similar file to this?
#[must_use]
pub fn read_from_file(path: impl AsRef<std::path::Path>, options: ReadOptions) -> Result<FullImage> {
    self::read_from_unbuffered(std::fs::File::open(path)?, options)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it.
#[must_use]
pub fn read_from_unbuffered(unbuffered: impl Read, options: ReadOptions) -> Result<FullImage> {
    self::read_from_buffered(BufReader::new(unbuffered), options)
}

/// performs many small read operations, and should thus be buffered
#[must_use]
pub fn read_from_buffered(read: impl Read, options: ReadOptions) -> Result<FullImage> {
    let mut read = PeekRead::new(read);
    let meta_data = MetaData::read_from_buffered_peekable(&mut read)?;
    let chunk_count = MetaData::skip_offset_tables(&mut read, &meta_data.headers)? as usize;
    self::read_data_by_meta(meta_data, chunk_count, &mut read, options)
}

/// performs many small read operations, and should thus be buffered
#[must_use]
pub fn read_data_by_meta(meta_data: MetaData, chunk_count: usize, read: &mut impl Read, options: ReadOptions) -> Result<FullImage> {
    FullImage::read_data_by_meta(meta_data, chunk_count, read, options)
}


#[must_use]
pub fn write_to_file(image: &FullImage, path: impl AsRef<std::path::Path>, options: WriteOptions) -> PassiveResult {
    self::write_to_unbuffered(image, std::fs::File::create(path)?, options)
}

/// needs more memory but allows for non-seeking write operations
#[must_use]
pub fn write_without_seek(image: &FullImage, mut unbuffered: impl Write, options: WriteOptions) -> PassiveResult {
    let mut bytes = Vec::new();

    // write the image to the seekable vec
    self::write_to_buffered(image, Cursor::new(&mut bytes), options)?;

    // write the vec into the actual output
    unbuffered.write_all(&bytes)?;
    Ok(())
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn write_to_unbuffered(image: &FullImage, unbuffered: impl Write + Seek, options: WriteOptions) -> PassiveResult {
    self::write_to_buffered(image, BufWriter::new(unbuffered), options)
}

/// assumes the reader is buffered
#[must_use]
pub fn write_to_buffered(image: &FullImage, write: impl Write + Seek, options: WriteOptions) -> PassiveResult {
    FullImage::write_to_buffered(image, write, options)
}




impl Default for WriteOptions {
    fn default() -> Self { Self::fast_writing() }
}

impl Default for ReadOptions {
    fn default() -> Self { Self::fast_loading() }
}


impl WriteOptions {
    pub fn fast_writing() -> Self {
        WriteOptions {
            parallel_compression: true,

            // RLE has low runtime cost but great compression for areas with solid color
            compression_method: Compression::RLE,

            line_order: LineOrder::Unspecified,
            tiles: BlockOptions::ScanLineBlocks
        }
    }

    pub fn small_image() -> Self {
        WriteOptions {
            parallel_compression: true,
            compression_method: Compression::ZIP16, // TODO test if this is one of the smallest
            line_order: LineOrder::Unspecified,
            tiles: BlockOptions::ScanLineBlocks
        }
    }

    pub fn small_writing() -> Self {
        WriteOptions {
            parallel_compression: false,
            compression_method: Compression::Uncompressed,
            line_order: LineOrder::Unspecified,
            tiles: BlockOptions::ScanLineBlocks
        }
    }

    pub fn debug() -> Self {
        WriteOptions {
            parallel_compression: false,
            compression_method: Compression::Uncompressed,
            line_order: LineOrder::Unspecified,
            tiles: BlockOptions::ScanLineBlocks
        }
    }
}

impl ReadOptions {

    pub fn fast_loading() -> Self {
        ReadOptions {
            parallel_decompression: true
        }
    }

    pub fn small_loading() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }

    pub fn debug() -> Self {
        ReadOptions {
            parallel_decompression: false
        }
    }

}

impl BlockOptions {
    pub fn has_tiles(&self) -> bool {
        match self {
            BlockOptions::Tiles { .. } => true,
            _ => false
        }
    }
}