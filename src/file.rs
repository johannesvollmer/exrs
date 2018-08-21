
use ::smallvec::SmallVec;
use ::attributes::*;
use ::chunkdata::*;


//  The representation of 16-bit floating-point numbers is analogous to IEEE 754,
//  but with 5 exponent bits and 10 bits for the fraction
pub const MAGIC_NUMBER: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];


/// This is the raw data of the file, obtained by minimal processing.
/// It closely resembles the actual file layout and supports all openEXR features natively.
/// Converting this to a boring rgb array requires more processing and loses information,
/// which should always be optional.
#[derive(Debug, Clone)]
pub struct File {
    version: Version,
    headers: Headers,

    offset_table: OffsetTable,
    data: Chunks,
}




#[derive(Debug, Clone)]
pub enum Headers {
    SinglePart(Header),

    /// separate header for each part, requires a null byte signalling the end of each header
    MultiPart(SmallVec<[Header; 3]>),
}

#[derive(Debug, Clone)]
pub struct Header {
    /// requires a null byte signalling the end of each attribute
    attributes: SmallVec<[Attribute; 12]>
}


#[derive(Debug, Clone)]
pub struct Version {
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