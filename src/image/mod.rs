
pub mod full;

use crate::meta::attributes::*;
use crate::compression::{Compression};
use crate::math::*;


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