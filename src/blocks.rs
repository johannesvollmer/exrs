use ::attributes::Compression;
use ::smallvec::SmallVec;

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
#[derive(Debug, Clone)]
pub struct OffsetTable {
    /// one per chunk, relative to file-start (!) in bytes
    offsets: SmallVec<[u64; 64]>, // TODO consider smaller stack size
}

#[derive(Debug, Clone)]
pub enum Chunks {
    /// type attribute “scanlineimage”
    ScanLine(Vec<Chunk<ScanLineBlock>>),

    /// type attribute “tiledimage”
    Tiled(Vec<Chunk<TileBlock>>),

    /// type attribute “deepscanline”,
    DeepScanLine(Vec<Chunk<DeepScanLineBlock>>),

    /// type attribute “deeptile”
    DeepTile(Vec<Chunk<DeepTileBlock>>),
}

pub fn compression_scan_lines_per_block(compression: Compression) -> usize {
    use self::Compression::*;
    match compression {
        None | RLE   | ZIPSingle    => 1,
        ZIP  | PXR24                => 16,
        PIZ  | B44   | B44A         => 32,
    }
}

#[derive(Debug, Clone)]
pub struct ScanLineBlock {
    /// only for multi part files.
    /// 0 indicates the chunk belongs to the part defined
    /// by the first header and the first chunk offset table
    part_number: Option<u64>,
    /// The block's y coordinate is equal to the pixel space y
    /// coordinate of the top scan line in the block.
    /// The top scan line block in the image is aligned with the top edge
    /// of the data window (that is, the y coordinate of the top scan line block
    /// is equal to the data window's minimum y)
    y_coordinate: i32,
    pixels: FlatPixelData,
}

#[derive(Debug, Clone)]
pub struct TileBlock {
    /// only for multi part files.
    /// 0 indicates the chunk belongs to the part defined
    /// by the first header and the first chunk offset table
    part_number: Option<u64>,
    tile_coordinates: TileCoordinates,
    pixels: FlatPixelData,
}

/// indicates the tile's position and resolution level
#[derive(Debug, Clone)]
pub struct TileCoordinates {
    tile_x: i32, tile_y: i32,
    level_x: i32, level_y: i32,
}

/// Deep scan line images are indicated by a type attribute of “deepscanline”.
/// Each chunk of deep scan line data is a single scan line of data.
#[derive(Debug, Clone)]
pub struct DeepScanLineBlock {
    /// only for multi part files.
    /// 0 indicates the chunk belongs to the part defined
    /// by the first header and the first chunk offset table
    part_number: Option<u64>,
    y_coordinate: i32,
    packed_pixel_offset_table_size: i32,
    packed_sample_data_size: i32,
    unpacked_sample_data_size: u64,
    compressed_pixel_offset_table: Vec<i32>,
    compressed_sample_data: Vec<u8>,
}

/// Tiled images are indicated by a type attribute of “deeptile”.
/// Each chunk of deep tile data is a single tile
#[derive(Debug, Clone)]
pub struct DeepTileBlock {
    /// only for multi part files.
    /// 0 indicates the chunk belongs to the part defined
    /// by the first header and the first chunk offset table
    part_number: Option<u64>,
    tile_coordinates: TileCoordinates,
    packed_pixel_offset_table_size: i32,
    packed_sample_data_size: i32,
    unpacked_sample_data_size: u64,

    /// When decompressed, the unpacked chunk consists of the
    /// channel data stored in a non-interleaved fashion
    /// Exception: For ZIP_COMPRESSION only there will be
    /// up to 16 scanlines in the packed sample data block
    compressed_sample_data: Vec<u8>,

    /// The pixel offset table is a list of int s, one for each column within the dataWindow.
    /// Each entry n in the table indicates the total number of samples required
    /// to store the pixel in n as well as all pixels to the left of it.
    /// Thus, the first samples stored in each channel of the pixel data are for
    /// the pixel in column 0, which contains table[1] samples.
    /// Each channel contains table[width-1] samples in total
    compressed_pixel_offset_table: Vec<i32>,
}

pub fn compression_supports_deep_data(compression: Compression) -> bool {
    use self::Compression::*;
    match compression {
        None | RLE | ZIPSingle | ZIP => true,
        _ => false,
    }
}

/// encoded as i32-size followed by u8 sequence
#[derive(Debug, Clone)]
pub enum FlatPixelData {
    Compressed(Vec<u8>),
    Decompressed(Vec<u8>)
}

impl FlatPixelData {
    pub fn try_decompress(&mut self, method: Compression){

    }

    pub fn decompressed(self, method: Compression) -> FlatPixelData {
        Ok({
            if let FlatPixelData::Compressed(compressed) = *self {
                FlatPixelData::Decompressed(::compress::decompress(method, compressed, None)?)

            } else {
                self
            }
        })
    }
}