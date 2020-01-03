
//use ::attributes::Compression;
use crate::meta::attributes::Kind;

// TODO
// INCREASING_Y The tiles for each level are stored in a contiguous block. The levels are
//ordered like this:
//where
//if the file's level mode is RIPMAP_LEVELS, or
//if the level mode is MIPMAP_LEVELS, or
//if the level mode is ONE_LEVEL.
//In each level, the tiles are stored in the following order:
//where and are the number of tiles in the x and y direction respectively,
//for that particular level.
// SEE PAGE 14 IN TECHNICAL INTRODUCTION


#[derive(Debug, Clone)]
pub struct Chunk {
    /// 0 indicates the chunk belongs to the part defined
    /// by the first header and the first chunk offset table
    /// PDF sais u64, but source code seems to be `int`
    pub part_number: i32,
    pub block: Block,
}

/// Each block in a multipart file can have a different type
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
#[derive(Debug, Clone, Copy)]
pub struct TileCoordinates {
    pub tile_x: i32, pub tile_y: i32, // TODO make this u32
    pub level_x: i32, pub level_y: i32,
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
    // TODO validate levels >= 0

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.tile_x.write(write)?;
        self.tile_y.write(write)?;
        self.level_x.write(write)?;
        self.level_y.write(write)?;
        Ok(())
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read(read: &mut impl Read) -> Result<Self> {
        Ok(TileCoordinates {
            tile_x: i32::read(read)?,
            tile_y: i32::read(read)?,
            level_x: i32::read(read)?,
            level_y: i32::read(read)?,
        })
    }
}



/// If a block length greater than this number is decoded,
/// it will not try to allocate that much memory, but instead consider
/// that decoding the block length has gone wrong
use crate::meta::{OffsetTables, Headers, Header};

impl ScanLineBlock {
    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.y_coordinate.write(write)?;
        u8::write_i32_sized_slice(write, &self.compressed_pixels)?;
        Ok(())
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read(read: &mut impl Read, max_block_byte_size: usize) -> Result<Self> {
        let y_coordinate = i32::read(read)?;
        let compressed_pixels = u8::read_i32_sized_vec(read, max_block_byte_size)?;
        Ok(ScanLineBlock { y_coordinate, compressed_pixels })
    }
}

impl TileBlock {
    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.coordinates.write(write)?;
        u8::write_i32_sized_slice(write, &self.compressed_pixels)?;
        Ok(())
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read(read: &mut impl Read, max_block_byte_size: usize) -> Result<Self> {
        let coordinates = TileCoordinates::read(read)?;
        let compressed_pixels = u8::read_i32_sized_vec(read, max_block_byte_size)?; // TODO maximum scan line size can easily be calculated
        Ok(TileBlock { coordinates, compressed_pixels })
    }

    /*pub fn reuse_read<R: Read>(mut self, read: &mut R) -> Result<Self> {
        self.coordinates = TileCoordinates::read(read)?;

        let size = i32::read(read)?;
        self.compressed_pixels = reuse_read_u8_vec(
            // TODO maximum scan line size can easily be calculated
            read, self.compressed_pixels, size as usize, MAX_PIXEL_BYTES
        )?;

        Ok(self)
    }*/
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
            read, compressed_pixel_offset_table_size as usize, max_block_byte_size
        )?;

        let compressed_sample_data = u8::read_vec(
            read, compressed_sample_data_size as usize, max_block_byte_size
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

    pub fn read(read: &mut impl Read, max_block_byte_size: usize) -> Result<Self> {
        let coordinates = TileCoordinates::read(read)?;
        let compressed_pixel_offset_table_size = u64::read(read)? as usize;
        let compressed_sample_data_size = u64::read(read)? as usize; // TODO u64 just guessed
        let decompressed_sample_data_size = u64::read(read)?;

        let compressed_pixel_offset_table = i8::read_vec(
            read, compressed_pixel_offset_table_size, max_block_byte_size
        )?;

        let compressed_sample_data = u8::read_vec(
            read, compressed_sample_data_size, max_block_byte_size
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

impl Chunk {
    pub fn validate(&self, is_multipart: bool, headers: usize) -> PassiveResult {
        if self.part_number as usize >= headers {
            return Err(Error::invalid("chunk data part number"));
        }

        if !is_multipart && self.part_number != 0 {
            unimplemented!()
            // Err(...)
        }

        // TODO
        Ok(())
//        match self.block {
//            Block::ScanLine     (ref value) => value.validate(header),
//            Block::Tile         (ref value) => value.validate(header),
//            Block::DeepScanLine (ref value) => value.validate(header),
//            Block::DeepTile     (ref value) => value.validate(header),
//        }
    }

    pub fn write(&self, write: &mut impl Write, is_multipart: bool, headers: &[Header]) -> PassiveResult {
        self.validate(is_multipart, headers.len())?;

        if is_multipart { self.part_number.write(write)?; }
        else { assert_eq!(self.part_number, 0); }

//        let header = &headers[self.part_number as usize];

        match self.block {
            Block::ScanLine     (ref value) => value.write(write),
            Block::Tile         (ref value) => value.write(write),
            Block::DeepScanLine (ref value) => value.write(write),
            Block::DeepTile     (ref value) => value.write(write),
        }
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read(read: &mut impl Read, is_multipart: bool, headers: &[Header]) -> Result<Self> {
        let part_number = {
            if is_multipart { i32::read(read)? } // documentation says u64, but is i32
            else { 0_i32 } // first header for single-part images
        };

        if part_number < 0 || part_number >= headers.len() as i32 {
            return Err(Error::invalid("chunk data part number"));
        }

        let header = &headers[part_number as usize];
        let kind = header.kind.unwrap_or(Kind::ScanLine); // TODO is this how it works?
        let max_block_byte_size = header.max_block_byte_size();

        let chunk = Chunk {
            part_number,
            block: match kind {
                Kind::ScanLine        => Block::ScanLine(ScanLineBlock::read(read, max_block_byte_size)?),
                Kind::Tile            => Block::Tile(TileBlock::read(read, max_block_byte_size)?),
                Kind::DeepScanLine    => Block::DeepScanLine(DeepScanLineBlock::read(read, max_block_byte_size)?),
                Kind::DeepTile        => Block::DeepTile(DeepTileBlock::read(read, max_block_byte_size)?),
            },
        };

        chunk.validate(is_multipart, headers.len())?;
        Ok(chunk)
    }
}

pub struct ChunkReader<'h, R: Read> {
    headers: &'h Headers,
    remaining_chunk_count: usize,
    multipart: bool,
    read: R,
}

impl<'h, R:Read> ChunkReader<'h, R> {
    pub fn new(read: R, multipart: bool, headers: &'h Headers, tables: &OffsetTables) -> Self {
        ChunkReader {
            remaining_chunk_count: tables.iter().map(Vec::len).sum(),
            headers, read, multipart
        }
    }
}

impl<'h, R: Read> Iterator for ChunkReader<'h, R> {
    type Item = Result<Chunk>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_chunk_count > 0 {
            self.remaining_chunk_count -= 1;
            Some(Chunk::read(&mut self.read, self.multipart, self.headers.as_slice()))
        }
        else {
            None
        }
    }
}












