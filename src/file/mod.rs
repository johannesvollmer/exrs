
//! The `file` module represents the file how it is laid out in memory.


pub mod chunks;
pub mod write;
pub mod read;


use ::smallvec::SmallVec;
use ::image::attributes::*;
use self::chunks::*;

// TODO put validation into own module
pub type Validity = Result<(), Invalid>;

#[derive(Debug, Clone, Copy)]
pub enum Invalid {
    Missing(Value),
    Combination(&'static [Value]),
    Content(Value, Required),
    Type(Required),
}

#[derive(Debug, Clone, Copy)]
pub enum Value {
    Attribute(&'static str),
    Version(&'static str),
    Chunk(&'static str),
    Type(&'static str),
    Part(&'static str),
    Enum(&'static str),
    Text,
}

#[derive(Debug, Clone, Copy)]
pub enum Required {
    Max(usize),
    Min(usize),
    Exact(&'static str),
    OneOf(&'static [&'static str]),
    Range {
        /// inclusive
        min: usize,

        /// inclusive
        max: usize
    },
}

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


impl MetaData {
    pub fn validate(&self) -> Validity {
        let tables = self.offset_tables.len();
        let headers = self.headers.len();

        if tables == 0 {
            return Err(Invalid::Missing(Value::Part("offset table")));
        }

        if headers == 0 {
            return Err(Invalid::Missing(Value::Part("header")));
        }

        if tables != headers {
            return Err(Invalid::Combination(&[
                Value::Part("headers"),
                Value::Part("offset tables"),
            ]));
        }

        let is_multi_part = headers != 1;
        if is_multi_part != self.version.has_multiple_parts {
            return Err(Invalid::Combination(&[
                Value::Version("multipart"),
                Value::Part("multipart"),
            ]));
        }

        Ok(())
    }
}


// TODO non-public fields?
#[derive(Debug, Clone)]
pub struct Header {
    /// Requires a null byte signalling the end of each attribute
    /// Contains custom attributes and required attributes
    pub attributes: SmallVec<[Attribute; 12]>,

    /// Cache required attribute indices of the attribute vector
    /// For faster access
    // TODO this only makes sense when decoding, and not for encoding
    pub indices: AttributeIndices,
}

/// Holds indices of the into header attributes
/// Indices will be overwritten in the order of the attributes in the file,
/// so that if multiple channel attributes exist, only the last one is referenced.
// TODO these always will be updated when a new attribute is inserted
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
    /// Note: Since the value of "maxSamplesPerPixel"
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

    /// this vector will contain all indices of chromaticity attributes
    pub chromaticities: SmallVec<[usize; 1]>,
}

impl Header {
    pub fn channels(&self) -> &ChannelList {
        self.attributes.get(self.indices.channels)
            .expect("invalid `channels` attribute index")
            .value.to_channel_list()
            .expect("check failed: `channels` attribute has wrong type")
    }

    pub fn kind(&self) -> Option<&ParsedText> {
        self.indices.kind.map(|kind|{
            self.attributes.get(kind)
                .expect("invalid `type` attribute index")
                .value.to_text()
                .expect("check failed: `type` attribute has wrong type")
        })
    }

    pub fn compression(&self) -> Compression {
        self.attributes.get(self.indices.compression)
            .expect("invalid `compression` attribute index")
            .value.to_compression()
            .expect("check failed: `compression` attribute has wrong type")
    }

    pub fn data_window(&self) -> I32Box2 {
        self.attributes.get(self.indices.data_window)
            .expect("invalid `dataWindow` attribute index")
            .value.to_i32_box_2()
            .expect("check failed: `dataWindow` attribute has wrong type")
    }

    pub fn tiles(&self) -> Option<TileDescription> {
        self.indices.tiles.map(|tiles|{
            self.attributes.get(tiles)
                .expect("invalid `tiles` attribute index")
                .value.to_tile_description()
                .expect("check failed: `tiles` attribute has wrong type")
        })
    }

    pub fn chunk_count(&self) -> Option<i32> {
        self.indices.chunk_count.map(|chunks|{
            self.attributes.get(chunks)
                .expect("invalid `chunks` attribute index")
                .value.to_i32()
                .expect("check failed: `chunks` attribute has wrong type")
        })
    }



    pub fn validate(&self, version: Version) -> Validity {
        if let Some(tiles) = self.indices.tiles {
            self.attributes.get(tiles)
                .expect("invalid tiles attribute index")
                .value.to_tile_description()?;
        }

        self.attributes.get(self.indices.channels)
            .expect("invalid channels attribute index")
            .value.to_channel_list()?;

        if let Some(kind) = self.indices.kind {
            self.attributes.get(kind)
                .expect("invalid kind attribute index")
                .value.to_text()?

                // sadly, "type" must be one of the specified texts
                // instead of being a plain enumeration
                .validate_kind()?;
        }

        if let Some(chunks) = self.indices.chunk_count {
            self.attributes.get(chunks)
                .expect("invalid chunk attribute index")
                .value.to_i32()?;
        }


        self.attributes.get(self.indices.compression)
            .expect("invalid compression attribute index")
            .value.to_compression()?;


        self.attributes.get(self.indices.data_window)
            .expect("invalid data_window attribute index")
            .value.to_i32_box_2()?;


        // TODO check all types..



        if version.has_multiple_parts {
            if self.indices.chunk_count.is_none() {
                return Err(Invalid::Missing(Value::Attribute("chunkCount (for multipart)")).into());
            }
            if self.indices.kind.is_none() {
                return Err(Invalid::Missing(Value::Attribute("type (for multipart)")).into());
            }
            if self.indices.name.is_none() {
                return Err(Invalid::Missing(Value::Attribute("name (for multipart)")).into());
            }
        }

        if version.has_deep_data {
            if self.indices.chunk_count.is_none() {
                return Err(Invalid::Missing(Value::Attribute("chunkCount (for deepdata)")).into());
            }
            if self.indices.kind.is_none() {
                return Err(Invalid::Missing(Value::Attribute("type (for deepdata)")).into());
            }
            if self.indices.name.is_none() {
                return Err(Invalid::Missing(Value::Attribute("name (for deepdata)")).into());
            }
            if self.indices.version.is_none() {
                return Err(Invalid::Missing(Value::Attribute("version (for deepdata)")).into());
            }
            if self.indices.max_samples_per_pixel.is_none() {
                return Err(Invalid::Missing(Value::Attribute("maxSamplesPerPixel (for deepdata)")).into());
            }

            let compression = self.compression(); // attribute is already checked
            if !compression.supports_deep_data() {
                return Err(Invalid::Content(
                    Value::Attribute("compression (for deepdata)"),
                    Required::OneOf(&["none", "rle", "zips", "zip"])
                ).into());
            }
        }

        if let Some(kind) = self.kind() {
            if kind.is_tile_kind() {
                if self.indices.tiles.is_none() {
                    return Err(Invalid::Missing(Value::Attribute("tiles (for tiledimage or deeptiles)")).into());
                }
            }

            // version-deepness and attribute-deepness must match
            if kind.is_deep_kind() != version.has_deep_data {
                return Err(Invalid::Content(
                    Value::Attribute("type"),
                    Required::OneOf(&["deepscanlines", "deeptiles"])
                ).into());
            }
        }

        Ok(())
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
    /// in c or bad c++ this might have been relevant (omg is he allowed to say that)
    pub has_long_names: bool,

    /// bit 11 "non-image bit"
    /// if true: at least one deep (thus non-reqular)
    pub has_deep_data: bool,

    /// bit 12
    /// if true: is multipart
    /// (end-of-header byte must always be included
    /// and part-number-fields must be added to chunks)
    pub has_multiple_parts: bool,
}

impl Version {
    pub fn validate(&self) -> Validity {
        match (
            self.is_single_tile, self.has_long_names,
            self.has_deep_data, self.file_format_version
        ) {
            // Single-part scan line. One normal scan line image.
            (false, false, false, _) => Ok(()),

            // Single-part tile. One normal tiled image.
            (true, false, false, _) => Ok(()),

            // Multi-part (new in 2.0).
            // Multiple normal images (scan line and/or tiled).
            (false, false, true, 2) => Ok(()),

            // Single-part deep data (new in 2.0).
            // One deep tile or deep scan line part
            (false, true, false, 2) => Ok(()),

            // Multi-part deep data (new in 2.0).
            // Multiple parts (any combination of:
            // tiles, scan lines, deep tiles and/or deep scan lines).
            (false, true, true, 2) => Ok(()),

            _ => Err(Invalid::Combination(&[
                Value::Version("is_single_tile"),
                Value::Version("has_long_names"),
                Value::Version("has_deep_data"),
                Value::Version("format_version"),
            ]))
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
