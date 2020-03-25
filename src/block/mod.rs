//! Handle uncompressed pixel byte blocks.
//! Includes compression and decompression.

pub mod lines;
pub mod samples;

use crate::compression::{ByteVec};
use crate::math::*;
use crate::error::{Result, Error, usize_to_i32};
use crate::meta::{MetaData, Header, Blocks};
use crate::chunk::{Chunk, Block, TileBlock, ScanLineBlock, TileCoordinates};


/// Specifies where a block of pixel data should be placed in the actual image.
/// This is a globally unique identifier which
/// includes the layer, level index, and pixel location.
#[derive(Clone, Copy, Eq, Hash, PartialEq, Debug)]
pub struct BlockIndex {

    /// Index of the layer.
    pub layer: usize,

    /// Pixel position of the bottom left corner of the block.
    pub pixel_position: Vec2<usize>,

    /// Pixel size of the block.
    pub pixel_size: Vec2<usize>,

    /// Index of the mip or rip level in the image.
    pub level: Vec2<usize>,
}

/// Contains a block of pixel data and where that data should be placed in the actual image.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UncompressedBlock {

    /// Location of the data inside the image.
    pub index: BlockIndex,

    /// Uncompressed pixel values of the whole block.
    /// One or more scan lines may be stored together as a scan line block.
    /// This byte vector contains all pixel rows, one after another.
    /// For each line in the tile, for each channel, the row values are contiguous.
    pub data: ByteVec,
}



impl UncompressedBlock {

    /// Decompress the possibly compressed chunk and returns an `UncompressedBlock`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    #[inline]
    #[must_use]
    pub fn decompress_chunk(chunk: Chunk, meta_data: &MetaData) -> Result<Self> {
        let header: &Header = meta_data.headers.get(chunk.layer_index)
            .ok_or(Error::invalid("chunk layer index"))?;

        let tile_data_indices = header.get_block_data_indices(&chunk.block)?;
        let absolute_indices = header.get_absolute_block_indices(tile_data_indices)?;

        absolute_indices.validate(Some(header.data_size))?;

        match chunk.block {
            Block::Tile(TileBlock { compressed_pixels, .. }) |
            Block::ScanLine(ScanLineBlock { compressed_pixels, .. }) => Ok(UncompressedBlock {
                data: header.compression.decompress_image_section(header, compressed_pixels, absolute_indices)?,
                index: BlockIndex {
                    layer: chunk.layer_index,
                    pixel_position: absolute_indices.position.to_usize("data indices start")?,
                    level: tile_data_indices.level_index,
                    pixel_size: absolute_indices.size,
                }
            }),

            _ => return Err(Error::unsupported("deep data not supported yet"))
        }
    }

    /// Consume this block by compressing it, returning a `Chunk`.
    // for uncompressed data, the ByteVec in the chunk is moved all the way
    #[inline]
    #[must_use]
    pub fn compress_to_chunk(self, meta_data: &MetaData) -> Result<Chunk> {
        let UncompressedBlock { data, index } = self;

        let header: &Header = meta_data.headers.get(index.layer)
            .expect("block layer index bug");

        let expected_byte_size = header.channels.bytes_per_pixel * self.index.pixel_size.area(); // TODO sampling??
        if expected_byte_size != data.len() {
            panic!("get_line byte size should be {} but was {}", expected_byte_size, data.len());
        }

        let compressed_data = header.compression.compress_image_section(data)?;

        Ok(Chunk {
            layer_index: index.layer,
            block : match header.blocks {
                Blocks::ScanLines => Block::ScanLine(ScanLineBlock {
                    compressed_pixels: compressed_data,

                    // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
                    y_coordinate: usize_to_i32(index.pixel_position.1) + header.own_attributes.data_position.1,
                }),

                Blocks::Tiles(tiles) => Block::Tile(TileBlock {
                    compressed_pixels: compressed_data,
                    coordinates: TileCoordinates {
                        level_index: index.level,

                        // FIXME this calculation should not be made here but elsewhere instead (in meta::header?)
                        tile_index: index.pixel_position / tiles.tile_size,
                    },

                }),
            }
        })
    }
}