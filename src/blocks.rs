use ::attributes::Compression;

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
    pub part_number: u64,
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
    Tiled(TileBlock),

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
    pub pixels: FlatPixelData,
}

#[derive(Debug, Clone)]
pub struct TileBlock {
    pub tile_coordinates: TileCoordinates,
    pub pixels: FlatPixelData,
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
    pub packed_pixel_offset_table_size: i32,
    pub packed_sample_data_size: i32,
    pub unpacked_sample_data_size: u64,
    pub compressed_pixel_offset_table: Vec<i32>,
    pub compressed_sample_data: Vec<u8>,
}

/// Tiled images are indicated by a type attribute of “deeptile”.
/// Each chunk of deep tile data is a single tile
#[derive(Debug, Clone)]
pub struct DeepTileBlock {
    pub tile_coordinates: TileCoordinates,
    pub packed_pixel_offset_table_size: i32,
    pub packed_sample_data_size: i32,
    pub unpacked_sample_data_size: u64,

    /// When decompressed, the unpacked chunk consists of the
    /// channel data stored in a non-interleaved fashion
    /// Exception: For ZIP_COMPRESSION only there will be
    /// up to 16 scanlines in the packed sample data block
    pub compressed_sample_data: Vec<u8>,

    /// The pixel offset table is a list of int s, one for each column within the dataWindow.
    /// Each entry n in the table indicates the total number of samples required
    /// to store the pixel in n as well as all pixels to the left of it.
    /// Thus, the first samples stored in each channel of the pixel data are for
    /// the pixel in column 0, which contains table[1] samples.
    /// Each channel contains table[width-1] samples in total
    pub compressed_pixel_offset_table: Vec<i32>,
}


/// encoded as i32-size followed by u8 sequence
#[derive(Debug, Clone)]
pub enum FlatPixelData {
    Compressed(Vec<u8>),
    Decompressed(Vec<u8>)
}
