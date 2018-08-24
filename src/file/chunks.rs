//use ::attributes::Compression;

/// For scan line blocks, the line offset table is a sequence of scan line offsets,
/// with one offset per scan line block. In the table, scan line offsets are
/// ordered according to increasing scan line y coordinates
///
/// For tiles, the offset table is a sequence of tile offsets, one offset per tile.
/// In the table, scan line offsets are sorted the same way as tiles in IncreasingY order
///
/// For multi-part files, each part defined in the header component has a corresponding chunk offset table
///
/// If the multipart (12) bit is unset and the chunkCount is not present, the number of entries in the
/// chunk table is computed using the dataWindow and tileDesc attributes and the compression format.
/// 2. If the multipart (12) bit is set, the header must contain a chunkCount attribute (which indicates the
/// size of the table and the number of chunks).
///
///
/// one per chunk, relative to file-start (!) in bytes
pub type OffsetTable = Vec<u64>;

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
    pub block: MultiPartBlock,
}

#[derive(Debug, Clone)]
pub enum SinglePartChunks {
    /// type attribute “scanlineimage”
    ScanLine(Vec<ScanLineBlock>),

    /// type attribute “tiledimage”
    Tile(Vec<TileBlock>),

    // FIXME check if this needs to support deep data
}

/// Each block in a multipart file can have a different type
#[derive(Debug, Clone)]
pub enum MultiPartBlock {
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
#[derive(Debug, Clone)]
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
    pub compressed_pixel_offset_table: Vec<i32>,
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
    pub compressed_pixel_offset_table: Vec<i32>,

    /// When decompressed, the unpacked chunk consists of the
    /// channel data stored in a non-interleaved fashion
    /// Exception: For ZIP_COMPRESSION only there will be
    /// up to 16 scanlines in the packed sample data block
    pub compressed_sample_data: Vec<u8>,
}


/// encoded as i32-size followed by u8 sequence
#[derive(Debug, Clone)]
pub enum FlatPixelData {
    Compressed(Vec<u8>),

    /// When decompressed, the unpacked chunk consists of
    /// the channel data stored in a non-interleaved fashion
    Decompressed(Vec<u8>)
}


use ::file::io::*;

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
use ::file::Header;

impl ScanLineBlock {
    pub fn validate(&self, header: &Header) -> Validity {
        if let &ParsedText::ScanLine = header.kind().expect("check failed: header kind missing") {
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
}

impl TileBlock {
    pub fn validate(&self, header: &Header) -> Validity {
        if let &ParsedText::Tile = header.kind().expect("check failed: header kind missing") {
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
}

impl DeepScanLineBlock {
    pub fn validate(&self, header: &Header) -> Validity {
        if let &ParsedText::DeepScanLine = header.kind().expect("check failed: header kind missing") {
            Ok(())

        } else {
            Err(Invalid::Content(Value::Attribute("type"), Required::Exact("deepscanline")).into())
        }
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.y_coordinate.write(write)?;
        (self.compressed_pixel_offset_table.len() as i32).write(write)?;
        (self.compressed_sample_data.len() as u64).write(write)?; // TODO just guessed
        self.decompressed_sample_data_size.write(write)?;
        write_i32_array(write, &self.compressed_pixel_offset_table)?;
        write_u8_array(write, &self.compressed_sample_data)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let y_coordinate = i32::read(read)?;
        let compressed_pixel_offset_table_size = i32::read(read)? as usize;
        let compressed_sample_data_size = u64::read(read)? as usize; // TODO u64 just guessed
        let decompressed_sample_data_size = u64::read(read)?;

        let compressed_pixel_offset_table = read_i32_vec(
            read, compressed_pixel_offset_table_size, MAX_PIXEL_BYTES
        )?;

        let compressed_sample_data = read_u8_vec(
            read, compressed_sample_data_size, MAX_PIXEL_BYTES
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
        if let &ParsedText::DeepTile = header.kind().expect("check failed: header kind missing") {
            Ok(())

        } else {
            Err(Invalid::Content(Value::Attribute("type"), Required::Exact("deeptile")).into())
        }
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.coordinates.write(write)?;
        (self.compressed_pixel_offset_table.len() as i32).write(write)?;
        (self.compressed_sample_data.len() as u64).write(write)?; // TODO just guessed
        self.decompressed_sample_data_size.write(write)?;
        write_i32_array(write, &self.compressed_pixel_offset_table)?;
        write_u8_array(write, &self.compressed_sample_data)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let coordinates = TileCoordinates::read(read)?;
        let compressed_pixel_offset_table_size = i32::read(read)? as usize;
        let compressed_sample_data_size = u64::read(read)? as usize; // TODO u64 just guessed
        let decompressed_sample_data_size = u64::read(read)?;

        let compressed_pixel_offset_table = read_i32_vec(
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

use ::file::validity::*;
use ::file::MetaData;
use ::file::attributes::ParsedText;

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
            MultiPartBlock::ScanLine    (ref value) => { value.validate(header)?; value.write(write) },
            MultiPartBlock::Tile        (ref value) => { value.validate(header)?; value.write(write) },
            MultiPartBlock::DeepScanLine(ref value) => { value.validate(header)?; value.write(write) },
            MultiPartBlock::DeepTile    (ref value) => { value.validate(header)?; value.write(write) },
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

        let kind_index = header.indices.kind.expect("check failed: `multi_part_chunk` called without `type` attribute");
        let kind = &header.attributes[kind_index].value.to_text()?;
        kind.validate_kind()?;

        Ok(MultiPartChunk {
            part_number,
            block: match kind/*TODO .as_kind()? */ {
                ParsedText::ScanLine        => MultiPartBlock::ScanLine(ScanLineBlock::read(read)?),
                ParsedText::Tile            => MultiPartBlock::Tile(TileBlock::read(read)?),
                ParsedText::DeepScanLine    => MultiPartBlock::DeepScanLine(Box::new(DeepScanLineBlock::read(read)?)),
                ParsedText::DeepTile        => MultiPartBlock::DeepTile(Box::new(DeepTileBlock::read(read)?)),
                _ => panic!("check failed: `kind` is not a valid type string"),
            },
        })
    }
}

impl SinglePartChunks {
    pub fn write<W: Write>(&self, write: &mut W, meta_data: &MetaData) -> WriteResult {
        // single-part files have either scan lines or tiles,
        // but never deep scan lines or deep tiles
        assert!(!meta_data.version.has_deep_data, "single_part_chunks called with deep data");
        assert_eq!(meta_data.headers.len(), 1, "single_part_chunks called with multiple headers");
        assert_eq!(meta_data.offset_tables.len(), 1, "single_part_chunks called with multiple offset tables");

        let offset_table = &meta_data.offset_tables[0];

        match *self {
            SinglePartChunks::ScanLine(ref lines) => {
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
        // single-part files have either scan lines or tiles,
        // but never deep scan lines or deep tiles
        assert!(!meta_data.version.has_deep_data, "single_part_chunks called with deep data");

        assert_eq!(meta_data.headers.len(), 1, "single_part_chunks called with multiple headers");
        let header = &meta_data.headers[0];

        assert_eq!(meta_data.offset_tables.len(), 1, "single_part_chunks called with multiple offset tables");
        let offset_table = &meta_data.offset_tables[0];

        // TODO is there a better way to figure out if this image contains tiles?
        let is_tile_image = header.tiles().is_some();
        let blocks = offset_table.len();

        Ok(if !is_tile_image {
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
        })
    }
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

    // TODO can read be immutable?
    pub fn read<R: Read>(read: &mut R, meta_data: &MetaData) -> ReadResult<Self> {
        Ok({
            if meta_data.version.has_multiple_parts {
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
}