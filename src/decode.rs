
use ::std::io::Read;
use ::std::io::Cursor;
use ::byteorder::{LittleEndian, ReadBytesExt};
use ::bit_field::BitField;
use ::attributes::*;


//  The representation of 16-bit floating-point numbers is analogous to IEEE 754,
//  but with 5 exponent bits and 10 bits for the fraction


pub type DecodeResult<T> = Result<T, DecodeErr>;

#[derive(Debug)]
pub enum DecodeErr {
    NotEXR,
    Invalid(&'static str),

    IO(::std::io::Error),
    NotSupported(&'static str),
}


pub struct Image {
    version: Version,
    headers: Headers,
    offset_table: OffsetTable,
    data: Chunks,
}


#[derive(Debug)]
struct Version {
    /// is currently 2
    file_format_version: u8,

    /// bit 9
    /// if true: single-part tiles (bits 11 and 12 must be 0).
    /// if false and 11 and 12 are false: single-part scan-line.
    is_single_tile: bool,

    /// bit 10
    /// if true: maximum name length is 255,
    /// else: 31 bytes for attribute names, attribute type names, and channel names
    has_long_names: bool,

    /// bit 11
    /// if true: at least one deep (thus non-reqular)
    has_deep_data: bool,

    /// bit 12
    /// if true: is multipart
    /// (end-of-header byte must always be included
    /// and part-number-fields must be added to chunks)
    has_multiple_parts: bool,
}

impl Version {
    pub fn is_valid(&self) -> bool {
        match (
            self.is_single_tile, self.has_long_names,
            self.has_deep_data, self.file_format_version
        ) {
            // Single-part scan line. One normal scan line image.
            (false, false, false, _) => true,

            // Single-part tile. One normal tiled image.
            (true, false, false, _) => true,

            // Multi-part (new in 2.0).
            // Multiple normal images (scan line and/or tiled).
            (false, false, true, 2) => true,

            // Single-part deep data (new in 2.0).
            // One deep tile or deep scan line part
            (false, true, false, 2) => true,

            // Multi-part deep data (new in 2.0).
            // Multiple parts (any combination of:
            // tiles, scan lines, deep tiles and/or deep scan lines).
            (false, true, true, 2) => true,

            _ => false
        }
    }
}

enum Headers {
    SinglePart(Header),

    /// separate header for each part and a null byte signalling the end of the header
    MultiPart(Vec<Header>), // TODO use small vec
}

struct Header {
    attributes: Vec<Attributes>
}

pub const REQUIRED_ATTRIBUTES: [(&'static str, &'static str); 8] = [
    ("channels", "chlist"),
    ("compression", "compression"),
    ("dataWindow", "box2i"),
    ("displayWindow", "box2i"),
    ("lineOrder", "lineOrder"),
    ("pixelAspectRatio", "float"),
    ("screenWindowCenter", "v2f"),
    ("screenWindowWidth", "float"),
];

/// size of the tiles and the number of resolution levels in the file
pub const TILE_ATTRIBUTE: (&'static str, &'static str) = (
    "tiles", "tiledesc"
);

// TODO standard OpenEXR attributes and optional attributes such as preview images, see the OpenEXR File Layout document
pub const REQUIRED_MULTIPART_ATTRIBUTES: [(&'static str, &'static str); 5] = [
    // Required if either the multipart bit (12) or the non-image bit (11) is set
    ("name", "string"),

    // Required if either the multipart bit (12) or the non-image bit (11) is set.
    // Set to one of: scanlineimage, tiledimage, deepscanline, or deeptile.
    // Note: This value must agree with the version field's tile bit (9) and non-image (deep data) bit (11) settings
    ("type", "string"),

    // This document describes version 1 data for all
    // part types. version is required for deep data (deepscanline and deeptile) parts.
    // If not specified for other parts, assume version=1
    ("version", "int"),

    // Required if either the multipart bit (12) or the non-image bit (11) is set
    ("chunkCount", "box2i"),

    // Required for parts of type tiledimage and deeptile
    TILE_ATTRIBUTE,
];

// TODO standard OpenEXR attributes and optional attributes such as preview images, see the OpenEXR File Layout document
pub const REQUIRED_DEEP_DATA_ATTRIBUTES: [(&'static str, &'static str); 4] = [
    // Required for parts of type tiledimage and deeptile
    TILE_ATTRIBUTE,

    // Required for deep data (deepscanline and deeptile) parts.
    // Note: Since the value of maxSamplesPerPixel
    // maybe be unknown at the time of opening the
    // file, the value “ -1 ” is written to the file to
    // indicate an unknown value. When the file is
    // closed, this will be overwritten with the correct
    // value.
    // If file writing does not complete
    // correctly due to an error, the value -1 will
    // remain. In this case, the value must be derived
    // by decoding each chunk in the part
    ("maxSamplesPerPixel", "int"),

    // Should be set to 1 . It will be changed if the format is updated
    ("version", "int"),

    // Must be set to deepscanline or deeptile
    ("type", "string"),
];



struct Attribute {
    name: LimitedText,
    kind: LimitedText,

    /// size in bytes of the attribute value
    size: i32,

    value: AttributeValue,
}




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
struct OffsetTable {
    /// one per chunk, relative to file (!) start in bytes
    offsets: Vec<Offset>, // TODO use smallvec?
}

pub type Offset = u64;


struct Chunk<T> {
    /// only for multi part files.
    /// 0 indicates the chunk belongs to the part defined
    /// by the first header and the first chunk offset table
    part_number: Option<u64>,
    data: T,
}

enum Chunks {
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
    use Compression::*;
    match compression {
        NONE | RLE | ZIPS   => 1,
        ZIP | PXR24         => 16,
        PIZ | B44 | B44A    => 32,
    }
}

struct ScanLineBlock {
    /// The block's y coordinate is equal to the pixel space y
    /// coordinate of the top scan line in the block.
    /// The top scan line block in the image is aligned with the top edge
    /// of the data window (that is, the y coordinate of the top scan line block
    /// is equal to the data window's minimum y)
    y_coordinate: i32,
    pixels: PixelData,
}

struct TileBlock {
    tile_coordinates: TileCoordinates,
    pixels: PixelData,
}

/// indicates the tile's position and resolution level
struct TileCoordinates {
    tile_x: i32, tile_y: i32,
    level_x: i32, level_y: i32,
}

/// Deep scan line images are indicated by a type attribute of “deepscanline”.
/// Each chunk of deep scan line data is a single scan line of data.
struct DeepScanLineBlock {
    y_coordinate: i32,
    packed_pixel_offset_table_size: i32,
    packed_sample_data_size: i32,
    unpacked_sample_data_size: u64,
    compressed_pixel_offset_table: Vec<i32>,
    compressed_sample_data: Vec<u8>,
}

/// Tiled images are indicated by a type attribute of “deeptile”.
/// Each chunk of deep tile data is a single tile
struct DeepTileBlock {
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

fn compression_supports_deep_data(compression: Compression) -> bool {
    use Compression::*;
    match compression {
        NONE | RLE | ZIPS | ZIP => true,
        _ => false,
    }
}

struct PixelData {
    size: i32,
    data: Vec<u8>,
}

/// null-terminated text strings.
/// max 31 bytes long (if bit 10 is set to 0),
/// or max 255 bytes long (if bit 10 is set to 1).
enum LimitedText {
    /// vector does not include null terminator
    Short(SmallVec<[u8; 31]>),

    /// vector does not include null terminator
    /// rust will automatically use pointers and heap if this is too large
    Long(SmallVec<[u8; 255]>),
}

impl LimitedText {
    /// panics if value is too long
    pub fn short(str_value: &str) -> Self {
        let str_bytes = str_value.as_bytes();

        if str_bytes.len() > 31 {
            panic!("text too long, greater than 31 bytes");
        }

        LimitedText::Short(SmallVec::from_slice(str_bytes))
    }
}


impl Image {
    pub const MAGIC_NUMBER: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];


    fn identify_exr<R: Read>(read: &mut R) -> DecodeResult<bool> {
        let mut magic_num = [0; 4];

        read.read_exact(&mut magic_num)
            .map_err(|io_err| DecodeErr::IO(io_err))?;

        Ok(magic_num == Self::MAGIC_NUMBER)
    }

    fn version_from_byte_stream<R: ReadBytesExt>(read: &mut R) -> DecodeResult<Version> {
        let version = read.read_u8()
            .map_err(|io| DecodeErr::IO(io))?;

        let flags: u32 = read.read_u24::<LittleEndian>()
            .map_err(|io| DecodeErr::IO(io))?;

        // the u32 will have zeroes appended at the beginning,
        // so indexing at 9 should give us the first bit of the u24
        let is_single_tile = flags.get_bit(9);
        let has_long_names = flags.get_bit(10);
        let has_deep_data = flags.get_bit(11);
        let has_multiple_parts = flags.get_bit(12);

        println!("version flags: {:#026b}", flags);

        // remaining bits are reserved and should be 0
        Ok(Version {
            file_format_version: version,
            is_single_tile, has_long_names,
            has_deep_data, has_multiple_parts,
        })
    }

    fn header_from_byte_stream<R: Read>(read: &mut R) -> DecodeResult<Header> {
        Err(DecodeErr::NotSupported("header"))
    }

    #[must_use]
    pub fn read_file(path: &str) -> DecodeResult<Self> {
        let file = ::std::fs::File::open(path)
            .map_err(|io| DecodeErr::IO(io))?;

        Self::read(&mut ::std::io::BufReader::new(file))
    }

    #[must_use]
    pub fn read<R: Read>(read: &mut R) -> DecodeResult<Self> {
        if !Self::identify_exr(read)? {
            return Err(DecodeErr::NotEXR);
        }

        println!("hurray, magic number works");

        let version = Self::version_from_byte_stream(read)?;
        println!("version: {:?}", version);

        if !version.is_valid() {
            return Err(DecodeErr::Invalid("version values are contradictory"))
        }


        let header = Self::header_from_byte_stream(read)?;


        Err(DecodeErr::NotSupported("anything"))

//        version: u32,
//        header: u32,
    }

}