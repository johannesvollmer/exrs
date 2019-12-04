//use ::attributes::Compression;
use crate::file::meta::attributes::ParsedText;

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
pub enum Chunks {
    MultiPart(Vec<MultiPartChunk>),
    SinglePart(SinglePartChunks)
}

#[derive(Debug, Clone)]
pub struct MultiPartChunk {
    /// 0 indicates the chunk belongs to the part defined
    /// by the first header and the first chunk offset table
    /// PDF sais u64, but source code seems to be `int`
    pub part_number: i32,
    pub block: DynamicBlock,
}

#[derive(Debug, Clone)]
pub enum SinglePartChunks {
    /// type attribute “scanlineimage”
    ScanLine(Vec<ScanLineBlock>),

    /// type attribute “tiledimage”
    Tile(Vec<TileBlock>),

    /// type attribute “deepscanlines”
    DeepScanLine(Vec<DeepScanLineBlock>),

    /// type attribute “deeptiles”
    DeepTile(Vec<DeepTileBlock>),
}

/// Each block in a multipart file can have a different type
#[derive(Debug, Clone)]
pub enum DynamicBlock {
    /// type attribute “scanlineimage”
    ScanLine(ScanLineBlock),

    /// type attribute “tiledimage”
    Tile(TileBlock),

    /// type attribute “deepscanline”,
    // use box to reduce the size of this enum (which is stored inside an array)
    DeepScanLine(Box<DeepScanLineBlock>),

    /// type attribute “deeptile”
    // use box to reduce the size of this enum (which is stored inside an array)
    DeepTile(Box<DeepTileBlock>),
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
    pub tile_x: i32, pub tile_y: i32,
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


use crate::file::io::*;

impl TileCoordinates {
    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.tile_x.write(write)?;
        self.tile_y.write(write)?;
        self.level_x.write(write)?;
        self.level_y.write(write)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
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
const MAX_PIXEL_BYTES: usize = 1048576; // 2^20
use crate::file::meta::Header;

impl ScanLineBlock {
    pub fn validate(&self, header: &Header) -> Validity {
        if let &ParsedText::ScanLine = header.kind.as_ref().expect("check failed: header kind missing") {
            Ok(())

        } else {
            // TODO make these string literals constants!
            Err(Invalid::Content(Value::Attribute("type"), Required::Exact("scanlineimage")).into())
        }
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.y_coordinate.write(write)?;
        write_i32_sized_u8_array(write, &self.compressed_pixels)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let y_coordinate = i32::read(read)?;
        let compressed_pixels = read_i32_sized_u8_vec(read, MAX_PIXEL_BYTES)?; // TODO maximum scan line size can easily be calculated
        Ok(ScanLineBlock { y_coordinate, compressed_pixels })
    }

    /// reuses the already allocated pixel data buffer
    pub fn reuse_read<R: Read>(mut self, read: &mut R) -> ReadResult<Self> {
        self.y_coordinate = i32::read(read)?;

        let size = i32::read(read)?;
        self.compressed_pixels = reuse_read_u8_vec(
            // TODO maximum scan line size can easily be calculated
            read, self.compressed_pixels, size as usize, MAX_PIXEL_BYTES
        )?;

        Ok(self)
    }
}

impl TileBlock {
    pub fn validate(&self, header: &Header) -> Validity {
        if let &ParsedText::Tile = header.kind.as_ref().expect("check failed: header kind missing") {
            Ok(())

        } else {
            Err(Invalid::Content(Value::Attribute("type"), Required::Exact("tiledimage")).into())
        }
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.coordinates.write(write)?;
        write_i32_sized_u8_array(write, &self.compressed_pixels)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let coordinates = TileCoordinates::read(read)?;
        let compressed_pixels = read_i32_sized_u8_vec(read, MAX_PIXEL_BYTES)?; // TODO maximum scan line size can easily be calculated
        Ok(TileBlock { coordinates, compressed_pixels })
    }

    /// reuses the already allocated pixel data buffer
    pub fn reuse_read<R: Read>(mut self, read: &mut R) -> ReadResult<Self> {
        self.coordinates = TileCoordinates::read(read)?;

        let size = i32::read(read)?;
        self.compressed_pixels = reuse_read_u8_vec(
            // TODO maximum scan line size can easily be calculated
            read, self.compressed_pixels, size as usize, MAX_PIXEL_BYTES
        )?;

        Ok(self)
    }
}

impl DeepScanLineBlock {
    pub fn validate(&self, header: &Header) -> Validity {
        if let &ParsedText::DeepScanLine = header.kind.as_ref().expect("check failed: header kind missing") {
            Ok(())

        } else {
            Err(Invalid::Content(Value::Attribute("type"), Required::Exact("deepscanline")).into())
        }
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.y_coordinate.write(write)?;
        (self.compressed_pixel_offset_table.len() as u64).write(write)?;
        (self.compressed_sample_data.len() as u64).write(write)?; // TODO just guessed
        self.decompressed_sample_data_size.write(write)?;
        write_i8_array(write, &self.compressed_pixel_offset_table)?;
        write_u8_array(write, &self.compressed_sample_data)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let y_coordinate = i32::read(read)?;
        let compressed_pixel_offset_table_size = u64::read(read)?;
        let compressed_sample_data_size = u64::read(read)?;
        let decompressed_sample_data_size = u64::read(read)?;

        // TODO don't just panic-cast
        // doc said i32, try u8
        let compressed_pixel_offset_table = read_i8_vec(
            read, compressed_pixel_offset_table_size as usize, MAX_PIXEL_BYTES
        )?;

        let compressed_sample_data = read_u8_vec(
            read, compressed_sample_data_size as usize, MAX_PIXEL_BYTES
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
    pub fn validate(&self, header: &Header) -> Validity {
        if let &ParsedText::DeepTile = header.kind.as_ref().expect("check failed: header kind missing") {
            Ok(())

        } else {
            Err(Invalid::Content(Value::Attribute("type"), Required::Exact("deeptile")).into())
        }
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.coordinates.write(write)?;
        (self.compressed_pixel_offset_table.len() as u64).write(write)?;
        (self.compressed_sample_data.len() as u64).write(write)?; // TODO just guessed
        self.decompressed_sample_data_size.write(write)?;
        write_i8_array(write, &self.compressed_pixel_offset_table)?;
        write_u8_array(write, &self.compressed_sample_data)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let coordinates = TileCoordinates::read(read)?;
        let compressed_pixel_offset_table_size = u64::read(read)? as usize;
        let compressed_sample_data_size = u64::read(read)? as usize; // TODO u64 just guessed
        let decompressed_sample_data_size = u64::read(read)?;

        let compressed_pixel_offset_table = read_i8_vec(
            read, compressed_pixel_offset_table_size, MAX_PIXEL_BYTES
        )?;

        let compressed_sample_data = read_u8_vec(
            read, compressed_sample_data_size, MAX_PIXEL_BYTES
        )?;

        Ok(DeepTileBlock {
            coordinates,
            decompressed_sample_data_size,
            compressed_pixel_offset_table,
            compressed_sample_data,
        })
    }
}

use crate::file::validity::*;
use crate::file::meta::MetaData;

impl MultiPartChunk {
    pub fn write<W: Write>(&self, write: &mut W, meta_data: &MetaData) -> WriteResult {
        if self.part_number as usize >= meta_data.headers.len() {
            return Err(Invalid::Combination(&[
                Value::Part("header count"), Value::Chunk("part number")
            ]).into());
        }

        self.part_number.write(write)?;
        let header = &meta_data.headers[self.part_number as usize];

        match self.block {
            DynamicBlock::ScanLine    (ref value) => { value.validate(header)?; value.write(write) },
            DynamicBlock::Tile        (ref value) => { value.validate(header)?; value.write(write) },
            DynamicBlock::DeepScanLine(ref value) => { value.validate(header)?; value.write(write) },
            DynamicBlock::DeepTile    (ref value) => { value.validate(header)?; value.write(write) },
        }
    }

    /*pub fn write_all<W: Write>(chunks: &[MultiPartChunk], write: &mut W, meta_data: &MetaData) -> WriteResult {
        // TODO check if chunk number equals table offset len() sum
        for chunk in chunks {
            chunk.write(write, meta_data)?;
        }

        Ok(())
    }*/

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R, meta_data: &MetaData) -> ReadResult<Self> {
        // decode the index that tells us which header we need to analyze
        let part_number = i32::read(read)?; // documentation says u64, but is i32

        let header = &meta_data.headers.get(part_number as usize)
            .ok_or(Invalid::Content(Value::Chunk("part index"), Required::Range { min:0, max: meta_data.headers.len() }))?;

        let kind = header.kind.as_ref().expect("check failed: `multi_part_chunk` called without `type` attribute");
        kind.validate_kind()?;

        Ok(MultiPartChunk {
            part_number,
            block: match kind/*TODO .as_kind()? */ {
                ParsedText::ScanLine        => DynamicBlock::ScanLine(ScanLineBlock::read(read)?),
                ParsedText::Tile            => DynamicBlock::Tile(TileBlock::read(read)?),
                ParsedText::DeepScanLine    => DynamicBlock::DeepScanLine(Box::new(DeepScanLineBlock::read(read)?)),
                ParsedText::DeepTile        => DynamicBlock::DeepTile(Box::new(DeepTileBlock::read(read)?)),
                _ => panic!("check failed: `kind` is not a valid type string"),
            },
        })
    }
}

impl SinglePartChunks {
    pub fn write<W: Write>(&self, write: &mut W, meta_data: &MetaData) -> WriteResult {
        // single-part files have either scan lines or tiles,
        // but never deep scan lines or deep tiles
        assert!(!meta_data.requirements.has_deep_data, "single_part_chunks called with deep data");
        assert_eq!(meta_data.headers.len(), 1, "single_part_chunks called with multiple headers");
        assert_eq!(meta_data.offset_tables.len(), 1, "single_part_chunks called with multiple offset tables");

        let offset_table = &meta_data.offset_tables[0];

        match *self {
            SinglePartChunks::ScanLine(ref lines) => {
                println!("TODO sort chunks!");
                if offset_table.len() != lines.len() {
                    return Err(Invalid::Combination(&[
                        Value::Part("offset_table size"),
                        Value::Chunk("scanline chunk count"),
                    ]).into())
                }

                for line in lines {
                    line.write(write)?;
                }
                Ok(())
            },
            SinglePartChunks::Tile(ref tiles) => {
                println!("TODO sort chunks!");
                if offset_table.len() != tiles.len() {
                    return Err(Invalid::Combination(&[
                        Value::Part("offset_table size"),
                        Value::Chunk("tile chunk count"),
                    ]).into())
                }

                for tile in tiles {
                    tile.write(write)?;
                }
                Ok(())
            }
            SinglePartChunks::DeepScanLine(ref lines) => {
                println!("TODO sort chunks!");
                if offset_table.len() != lines.len() {
                    return Err(Invalid::Combination(&[
                        Value::Part("offset_table size"),
                        Value::Chunk("scanline chunk count"),
                    ]).into())
                }

                for line in lines {
                    line.write(write)?;
                }
                Ok(())
            },
            SinglePartChunks::DeepTile(ref tiles) => {
                println!("TODO sort chunks!");
                if offset_table.len() != tiles.len() {
                    return Err(Invalid::Combination(&[
                        Value::Part("offset_table size"),
                        Value::Chunk("tile chunk count"),
                    ]).into())
                }

                for tile in tiles {
                    tile.write(write)?;
                }
                Ok(())
            }
        }
    }

    pub fn read<R: Read>(read: &mut R, meta_data: &MetaData) -> ReadResult<Self> {
        assert_eq!(meta_data.headers.len(), 1, "single_part_chunks called with multiple headers");
        let header = &meta_data.headers[0];

        // TODO is there a better way to figure out if this image contains tiles?
        let is_tiled = header.tiles.is_some();
        let is_deep = meta_data.requirements.has_deep_data;

        assert_eq!(meta_data.offset_tables.len(), 1, "single_part_chunks called with multiple offset tables");
        let offset_table = &meta_data.offset_tables[0];
        let blocks = offset_table.len();


        Ok({
            if is_deep {
                if !is_tiled {
                    let mut scan_line_blocks = Vec::with_capacity(blocks);
                    for _ in 0..blocks {
                        scan_line_blocks.push(DeepScanLineBlock::read(read)?)
                    }

                    SinglePartChunks::DeepScanLine(scan_line_blocks)

                } else {
                    let mut tile_blocks = Vec::with_capacity(blocks);
                    for _ in 0..blocks {
                        tile_blocks.push(DeepTileBlock::read(read)?)
                    }

                    SinglePartChunks::DeepTile(tile_blocks)
                }

            } else {
                if !is_tiled {
                    let mut scan_line_blocks = Vec::with_capacity(blocks);
                    for _ in 0..blocks {
                        scan_line_blocks.push(ScanLineBlock::read(read)?)
                    }

                    SinglePartChunks::ScanLine(scan_line_blocks)

                } else {
                    let mut tile_blocks = Vec::with_capacity(blocks);
                    for _ in 0..blocks {
                        tile_blocks.push(TileBlock::read(read)?)
                    }

                    SinglePartChunks::Tile(tile_blocks)
                }
            }
        })
    }

    pub fn read_parallel<R: Read>(read: &mut R, meta_data: MetaData, sender: Sender<ChunkUpdate<DynamicBlock>>) {
        assert_eq!(meta_data.headers.len(), 1, "single_part_chunks called with multiple headers");

        // TODO is there a better way to figure out if this image contains tiles?
        let is_tiled = meta_data.headers[0].tiles.is_some();
        let is_deep = meta_data.requirements.has_deep_data;

        assert_eq!(meta_data.offset_tables.len(), 1, "single_part_chunks called with multiple offset tables");
        let blocks = meta_data.offset_tables[0].len();

        if is_deep {
            if !is_tiled {
                sender.send(ChunkUpdate::ExpectingChunks { additional: blocks }).unwrap();
                for _ in 0..blocks {
                    sender.send(ChunkUpdate::Chunk(
                        DeepScanLineBlock::read(read)
                            .map(|block| DynamicBlock::DeepScanLine(Box::new(block)))

                    )).unwrap();
                }

                sender.send(ChunkUpdate::Finished(meta_data)).unwrap();

            } else {
                sender.send(ChunkUpdate::ExpectingChunks { additional: blocks }).unwrap();
                for _ in 0..blocks {
                    sender.send(ChunkUpdate::Chunk(
                        DeepTileBlock::read(read)
                            .map(|block| DynamicBlock::DeepTile(Box::new(block)))

                    )).unwrap();
                }

                sender.send(ChunkUpdate::Finished(meta_data)).unwrap();
            }
        } else {
            if !is_tiled {
                sender.send(ChunkUpdate::ExpectingChunks { additional: blocks }).unwrap();
                for _ in 0..blocks {
                    sender.send(ChunkUpdate::Chunk(
                        ScanLineBlock::read(read)
                            .map(|block| DynamicBlock::ScanLine(block))

                    )).unwrap();
                }

                sender.send(ChunkUpdate::Finished(meta_data)).unwrap();

            } else {
                sender.send(ChunkUpdate::ExpectingChunks { additional: blocks }).unwrap();
                for _ in 0..blocks {
                    sender.send(ChunkUpdate::Chunk(
                        TileBlock::read(read)
                            .map(|block| DynamicBlock::Tile(block))

                    )).unwrap();
                }

                sender.send(ChunkUpdate::Finished(meta_data)).unwrap();
            }
        }
    }
}


use ::std::sync::mpsc::{Sender, Receiver};
use ::std::sync::mpsc;

pub enum ChunksReceiver {
    MultiPart(Receiver<ChunkUpdate<MultiPartChunk>>),
    SinglePart(Receiver<ChunkUpdate<DynamicBlock>>),
}

pub enum ChunkUpdate<T> {
    ExpectingChunks { additional: usize },
    Chunk(ReadResult<T>),
    Finished(MetaData),
}

impl Chunks {
    pub fn write<W: Write>(&self, write: &mut W, meta_data: &MetaData) -> WriteResult {
        // TODO check version.multiple and self::MultiPart
        match *self {
            Chunks::MultiPart(ref chunks) => {
                // TODO check chunk len == offset_table len sum
                for chunk in chunks {
                    chunk.write(write, meta_data)?
                }
                Ok(())
            },
            Chunks::SinglePart(ref chunks) => {
                chunks.write(write, meta_data)
            }
        }
    }

    pub fn read<R: Read>(read: &mut R, meta_data: &MetaData) -> ReadResult<Self> {
        Ok({
            if meta_data.requirements.has_multiple_parts {
                Chunks::MultiPart({
                    let mut chunks = Vec::new();
                    for offset_table in &meta_data.offset_tables {
                        chunks.reserve(offset_table.len());
                        for _ in 0..offset_table.len() {
                            chunks.push(MultiPartChunk::read(read, meta_data)?)
                        }
                    }

                    chunks
                })

            } else {
                Chunks::SinglePart(SinglePartChunks::read(read, meta_data)?)
            }
        })
    }

    pub fn read_parallel<R: Read + Send + 'static>(mut read: R, meta_data: MetaData) -> ChunksReceiver {
        if meta_data.requirements.has_multiple_parts {
            let (sender, receiver) = mpsc::channel();

            ::std::thread::spawn(move ||{
                for offset_table in &meta_data.offset_tables {
                    sender.send(ChunkUpdate::ExpectingChunks { additional: offset_table.len() }).unwrap();

                    for _ in 0..offset_table.len() {
                        sender.send(ChunkUpdate::Chunk(MultiPartChunk::read(&mut read, &meta_data))).unwrap();
                    }
                }

                sender.send(ChunkUpdate::Finished(meta_data)).unwrap();
            });

            ChunksReceiver::MultiPart(receiver)

        } else {
            let (sender, receiver) = mpsc::channel();
            // Chunks::SinglePart(SinglePartChunks::read(read, meta_data)?);

            ::std::thread::spawn(move ||{
                SinglePartChunks::read_parallel(&mut read, meta_data, sender);
            });

            ChunksReceiver::SinglePart(receiver)
        }
    }
}