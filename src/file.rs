
use ::smallvec::SmallVec;
use ::attributes::*;
use ::blocks::*;


//  The representation of 16-bit floating-point numbers is analogous to IEEE 754,
//  but with 5 exponent bits and 10 bits for the fraction
pub const MAGIC_NUMBER: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];


/// This is the raw data of the file,
/// which can be obtained from a byte stream with minimal processing overhead
/// or written to a byte stream with minimal processing overhead.
///
/// It closely resembles the actual file layout and supports all openEXR features natively.
/// Converting this from or to a boring RGBA array requires more processing and loses information,
/// which is thus optional
#[derive(Debug, Clone)]
pub struct RawImage {
    pub meta_data: MetaData,
    pub chunks: Chunks,
}

#[derive(Debug, Clone)]
pub struct MetaData {
    pub version: Version,

    /// separate header for each part, requires a null byte signalling the end of each header
    pub headers: Headers,

    /// one table per header
    pub offset_tables: OffsetTables,
}

pub type Headers = SmallVec<[Header; 3]>;
pub type OffsetTables = SmallVec<[OffsetTable; 3]>;


// TODO non-public fields?
#[derive(Debug, Clone)]
pub struct Header {
    /// requires a null byte signalling the end of each attribute
    /// contains custom attributes and required attributes
    pub attributes: SmallVec<[Attribute; 12]>,

    /// cache required attribute indices
    pub indices: AttributeIndices,
}

/// Holds indices into header attributes
#[derive(Debug, Clone)]
pub struct AttributeIndices {
    pub channels: usize,
    pub compression: usize,
    pub data_window: usize,
    pub display_window: usize,
    pub line_order: usize,
    pub pixel_aspect: usize,
    pub screen_window_center: usize,
    pub screen_window_width: usize,

    /// TileDescription: size of the tiles and the number of resolution levels in the file
    /// Required for parts of type tiledimage and deeptile
    pub tiles: Option<usize>,

    /// Required if either the multipart bit (12) or the non-image bit (11) is set
    pub name: Option<usize>,

    /// Required if either the multipart bit (12) or the non-image bit (11) is set.
    /// Set to one of: scanlineimage, tiledimage, deepscanline, or deeptile.
    /// Note: This value must agree with the version field's tile bit (9) and non-image (deep data) bit (11) settings
    /// required for deep data. when deep data, Must be set to deepscanline or deeptile
    pub kind: Option<usize>,

    /// This document describes version 1 data for all
    /// part types. version is required for deep data (deepscanline and deeptile) parts.
    /// If not specified for other parts, assume version=1
    /// required for deep data: Should be set to 1 . It will be changed if the format is updated
    pub version: Option<usize>,

    /// Required if either the multipart bit (12) or the non-image bit (11) is set
    pub chunk_count: Option<usize>,

    /// Required for deep data (deepscanline and deeptile) parts.
    /// Note: Since the value of maxSamplesPerPixel
    /// maybe be unknown at the time of opening the
    /// file, the value “ -1 ” is written to the file to
    /// indicate an unknown value. When the file is
    /// closed, this will be overwritten with the correct
    /// value.
    /// If file writing does not complete
    /// correctly due to an error, the value -1 will
    /// remain. In this case, the value must be derived
    /// by decoding each chunk in the part
    pub max_samples_per_pixel: Option<usize>,
}

impl Header {
    pub fn is_valid(&self, _version: Version) -> bool {

        true
        // TODO check if the header has all required attributes
    }
}


// TODO use immutable accessors and private fields?
#[derive(Debug, Clone, Copy)]
pub struct Version {
    /// is currently 2
    pub file_format_version: u8,

    /// bit 9
    /// if true: single-part tiles (bits 11 and 12 must be 0).
    /// if false and 11 and 12 are false: single-part scan-line.
    pub is_single_tile: bool,

    /// bit 10
    /// if true: maximum name length is 255,
    /// else: 31 bytes for attribute names, attribute type names, and channel names
    pub has_long_names: bool,

    /// bit 11
    /// if true: at least one deep (thus non-reqular)
    pub has_deep_data: bool,

    /// bit 12
    /// if true: is multipart
    /// (end-of-header byte must always be included
    /// and part-number-fields must be added to chunks)
    pub has_multiple_parts: bool,
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




/*

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

pub const TILE_ATTRIBUTE: (&'static str, &'static str) = (
    "tiles", "tiledesc"
);

// TODO standard OpenEXR attributes and optional attributes such as preview images, see the OpenEXR File Layout document
pub const REQUIRED_MULTIPART_ATTRIBUTES: [(&'static str, &'static str); 5] = [
    ("name", "string"),
    ("type", "string"),
    ("version", "int"),
    ("chunkCount", "box2i"),
    TILE_ATTRIBUTE,
];

// TODO standard OpenEXR attributes and optional attributes such as preview images, see the OpenEXR File Layout document
pub const REQUIRED_DEEP_DATA_ATTRIBUTES: [(&'static str, &'static str); 4] = [
    // Required for parts of type tiledimage and deeptile
    TILE_ATTRIBUTE,
    ("maxSamplesPerPixel", "int"),
    ("version", "int"),
    ("type", "string"),
];*/
