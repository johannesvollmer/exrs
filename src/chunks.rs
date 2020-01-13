
use crate::meta::attributes::{Box2I32};

// TODO SEE PAGE 14 IN TECHNICAL INTRODUCTION


#[derive(Debug, Clone)]
pub struct Chunk {
    /// index of which header this pixel data belongs to
    /// (data can be in any order in the file)
    // PDF says u64, but source code seems to be i32
    pub part_number: i32,
    pub block: Block,
}

/// Each part in a multipart file can have a different type
#[derive(Debug, Clone)]
pub enum Block {
    /// type attribute “scanlineimage”
    ScanLine(ScanLineBlock),

    /// type attribute “tiledimage”
    Tile(TileBlock),

    /// type attribute “deepscanline”,
    DeepScanLine(DeepScanLineBlock),

    /// type attribute “deeptile”
    DeepTile(DeepTileBlock),
}


#[derive(Debug, Clone)]
pub struct ScanLineBlock {
    /// The block's y coordinate is equal to the pixel space y
    /// coordinate of the top scan line in the block.
    /// The top scan line block in the image is aligned with the top edge
    /// of the data window (that is, the y coordinate of the top scan line block
    /// is equal to the data window's minimum y)
    pub y_coordinate: i32,

    /// For scan line images and deep scan line images, one or more scan lines
    /// may be stored together as a scan line block. The number of scan lines per block
    /// depends on how the pixel data are compressed
    pub compressed_pixels: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TileBlock {
    pub coordinates: TileCoordinates,
    pub compressed_pixels: Vec<u8>,
}

/// indicates the tile's position and resolution level
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct TileCoordinates {
    // TODO make these u32 as they are all indices?
    pub tile_index: Vec2<i32>,
    pub level_index: Vec2<i32>,
}

/// Deep scan line images are indicated by a type attribute of “deepscanline”.
/// Each chunk of deep scan line data is a single scan line of data.
#[derive(Debug, Clone)]
pub struct DeepScanLineBlock {
    pub y_coordinate: i32,
    pub decompressed_sample_data_size: u64,

    /// (Taken from DeepTileBlock)
    /// The pixel offset table is a list of ints, one for each column within the dataWindow.
    /// Each entry n in the table indicates the total number of samples required
    /// to store the pixel in n as well as all pixels to the left of it.
    /// Thus, the first samples stored in each channel of the pixel data are for
    /// the pixel in column 0, which contains table[1] samples.
    /// Each channel contains table[width-1] samples in total
    pub compressed_pixel_offset_table: Vec<i8>,
    pub compressed_sample_data: Vec<u8>,
}

/// Tiled images are indicated by a type attribute of “deeptile”.
/// Each chunk of deep tile data is a single tile
#[derive(Debug, Clone)]
pub struct DeepTileBlock {
    pub coordinates: TileCoordinates,
    pub decompressed_sample_data_size: u64,

    /// The pixel offset table is a list of ints, one for each column within the dataWindow.
    /// Each entry n in the table indicates the total number of samples required
    /// to store the pixel in n as well as all pixels to the left of it.
    /// Thus, the first samples stored in each channel of the pixel data are for
    /// the pixel in column 0, which contains table[1] samples.
    /// Each channel contains table[width-1] samples in total
    pub compressed_pixel_offset_table: Vec<i8>,

    /// When decompressed, the unpacked chunk consists of the
    /// channel data stored in a non-interleaved fashion
    /// Exception: For ZIP_COMPRESSION only there will be
    /// up to 16 scanlines in the packed sample data block
    pub compressed_sample_data: Vec<u8>,
}


use crate::io::*;

impl TileCoordinates {
    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.tile_index.0.write(write)?;
        self.tile_index.1.write(write)?;
        self.level_index.0.write(write)?;
        self.level_index.1.write(write)?;

        Ok(())
    }

    pub fn read(read: &mut impl Read) -> Result<Self> {
        let tile_x = i32::read(read)?;
        let tile_y = i32::read(read)?;

        let level_x = i32::read(read)?;
        let level_y = i32::read(read)?;

        Ok(TileCoordinates {
            tile_index: Vec2(tile_x, tile_y),
            level_index: Vec2(level_x, level_y)
        })
    }

    pub fn from_absolute_coordinates(level: Vec2<i32>, tile: Vec2<i32>, tile_size: Vec2<i32>, data_window: Box2I32) -> Self {
        TileCoordinates {
            tile_index: tile / tile_size + data_window.start,
            level_index: level,
        }
    }

    pub fn to_data_indices(&self, tile_size: Vec2<u32>, max: Vec2<u32>) -> Box2I32 {
        let start = Vec2::try_from(self.tile_index).unwrap() * tile_size;

        Box2I32 {
            start: Vec2::try_from(start).unwrap(),
            size: Vec2(
                calculate_block_size(max.0, tile_size.0, start.0),
                calculate_block_size(max.1, tile_size.0, start.1),
            ),
        }
    }

    pub fn to_absolute_indices(&self, tile_size: Vec2<u32>, data_window: Box2I32) -> Box2I32 {
        let data = self.to_data_indices(tile_size, data_window.size);
        data.with_origin(data_window.start)
    }
}



use crate::meta::{Header, MetaData, Blocks, calculate_block_size};

impl ScanLineBlock {
    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.y_coordinate.write(write)?;
        u8::write_i32_sized_slice(write, &self.compressed_pixels)?;
        Ok(())
    }

    pub fn read(read: &mut impl Read, max_block_byte_size: usize) -> Result<Self> {
        let y_coordinate = i32::read(read)?;
        let compressed_pixels = u8::read_i32_sized_vec(read, max_block_byte_size, Some(max_block_byte_size))?;
        Ok(ScanLineBlock { y_coordinate, compressed_pixels })
    }
}

impl TileBlock {
    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.coordinates.write(write)?;
        u8::write_i32_sized_slice(write, &self.compressed_pixels)?;
        Ok(())
    }

    pub fn read(read: &mut impl Read, max_block_byte_size: usize) -> Result<Self> {
        let coordinates = TileCoordinates::read(read)?;
        let compressed_pixels = u8::read_i32_sized_vec(read, max_block_byte_size, Some(max_block_byte_size))?;
        Ok(TileBlock { coordinates, compressed_pixels })
    }
}

impl DeepScanLineBlock {
    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.y_coordinate.write(write)?;
        (self.compressed_pixel_offset_table.len() as u64).write(write)?;
        (self.compressed_sample_data.len() as u64).write(write)?; // TODO just guessed
        self.decompressed_sample_data_size.write(write)?;
        i8::write_slice(write, &self.compressed_pixel_offset_table)?;
        u8::write_slice(write, &self.compressed_sample_data)?;
        Ok(())
    }

    pub fn read(read: &mut impl Read, max_block_byte_size: usize) -> Result<Self> {
        let y_coordinate = i32::read(read)?;
        let compressed_pixel_offset_table_size = u64::read(read)?;
        let compressed_sample_data_size = u64::read(read)?;
        let decompressed_sample_data_size = u64::read(read)?;

        // TODO don't just panic-cast
        // doc said i32, try u8
        let compressed_pixel_offset_table = i8::read_vec(
            read, compressed_pixel_offset_table_size as usize, 6 * std::u16::MAX as usize, Some(max_block_byte_size)
        )?;

        let compressed_sample_data = u8::read_vec(
            read, compressed_sample_data_size as usize, 6 * std::u16::MAX as usize, Some(max_block_byte_size)
        )?;

        Ok(DeepScanLineBlock {
            y_coordinate,
            decompressed_sample_data_size,
            compressed_pixel_offset_table,
            compressed_sample_data,
        })
    }
}


impl DeepTileBlock {
    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.coordinates.write(write)?;
        (self.compressed_pixel_offset_table.len() as u64).write(write)?;
        (self.compressed_sample_data.len() as u64).write(write)?; // TODO just guessed
        self.decompressed_sample_data_size.write(write)?;
        i8::write_slice(write, &self.compressed_pixel_offset_table)?;
        u8::write_slice(write, &self.compressed_sample_data)?;
        Ok(())
    }

    pub fn read(read: &mut impl Read, hard_max_block_byte_size: usize) -> Result<Self> {
        let coordinates = TileCoordinates::read(read)?;
        let compressed_pixel_offset_table_size = u64::read(read)? as usize;
        let compressed_sample_data_size = u64::read(read)? as usize; // TODO u64 just guessed
        let decompressed_sample_data_size = u64::read(read)?;

        let compressed_pixel_offset_table = i8::read_vec(
            read, compressed_pixel_offset_table_size, 6 * std::u16::MAX as usize, Some(hard_max_block_byte_size)
        )?;

        let compressed_sample_data = u8::read_vec(
            read, compressed_sample_data_size, 6 * std::u16::MAX as usize, Some(hard_max_block_byte_size)
        )?;

        Ok(DeepTileBlock {
            coordinates,
            decompressed_sample_data_size,
            compressed_pixel_offset_table,
            compressed_sample_data,
        })
    }
}

use crate::error::{PassiveResult, Result, Error};
use crate::math::Vec2;

impl Chunk {
    pub fn validate(&self, headers: usize) -> PassiveResult {
        if self.part_number as usize >= headers || self.part_number < 0 { // also triggers where part number > 0 in singlepart image
            return Err(Error::invalid("chunk data part number"));
        }

        Ok(())
        // TODO:
//        match self.block {
//            Block::ScanLine     (ref value) => value.validate(header),
//            Block::Tile         (ref value) => value.validate(header),
//            Block::DeepScanLine (ref value) => value.validate(header),
//            Block::DeepTile     (ref value) => value.validate(header),
//        }
    }

    pub fn write(&self, write: &mut impl Write, headers: &[Header]) -> PassiveResult {
        self.validate(headers.len())?;

        if headers.len() != 1 { self.part_number.write(write)?; }
        else { assert_eq!(self.part_number, 0); }

        match self.block {
            Block::ScanLine     (ref value) => value.write(write),
            Block::Tile         (ref value) => value.write(write),
            Block::DeepScanLine (ref value) => value.write(write),
            Block::DeepTile     (ref value) => value.write(write),
        }
    }

    pub fn read(read: &mut impl Read, meta_data: &MetaData) -> Result<Self> {
        let part_number = {
            if meta_data.requirements.is_multipart() { i32::read(read)? } // documentation says u64, but is i32
            else { 0_i32 } // use first header for single-part images
        };

        if part_number < 0 || part_number >= meta_data.headers.len() as i32 {
            return Err(Error::invalid("chunk data part number"));
        }

        let header = &meta_data.headers[part_number as usize];
        let max_block_byte_size = header.max_block_byte_size().min(std::u16::MAX as usize * 16);

        let chunk = Chunk {
            part_number,
            block: match header.blocks {
                // flat data
                Blocks::ScanLines if !header.deep => Block::ScanLine(ScanLineBlock::read(read, max_block_byte_size)?),
                Blocks::Tiles(_) if !header.deep     => Block::Tile(TileBlock::read(read, max_block_byte_size)?),

                // deep data
                Blocks::ScanLines   => Block::DeepScanLine(DeepScanLineBlock::read(read, max_block_byte_size)?),
                Blocks::Tiles(_)    => Block::DeepTile(DeepTileBlock::read(read, max_block_byte_size)?),
            },
        };

        chunk.validate(meta_data.headers.len())?;
        Ok(chunk)
    }
}

