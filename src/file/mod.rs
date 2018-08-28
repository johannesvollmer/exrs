
//! The `file` module represents the file how it is laid out in memory.


pub mod attributes;
pub mod chunks;
pub mod compress;


use ::smallvec::SmallVec;
use ::file::attributes::*;
use self::chunks::*;
use ::std::io::{Read, Write};
use self::io::Data;







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
    // ### required attributes: ###
    pub channels: Option<usize>,
    pub compression: Option<usize>,
    pub data_window: Option<usize>,
    pub display_window: Option<usize>,
    pub line_order: Option<usize>,
    pub pixel_aspect: Option<usize>,
    pub screen_window_center: Option<usize>,
    pub screen_window_width: Option<usize>,

    // ### optional attributes: ###

    /// TileDescription: size of the tiles and the number of resolution levels in the file
    /// Required for parts of type tiledimage and deeptile
    pub tiles: Option<usize>,

    /// The name of the `Part` which contains this Header.
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

    /// Required if either the multipart bit (12) or the deep-data bit (11) is set
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
    pub chromaticities: SmallVec<[usize; 3]>,
}




pub type WriteResult = ::std::result::Result<(), WriteError>;

#[derive(Debug)]
pub enum WriteError {
    CompressionError(compress::Error),
    IoError(::std::io::Error),
    Invalid(Invalid),
}



pub type ReadResult<T> = ::std::result::Result<T, ReadError>;

// TODO implement Display for all errors
#[derive(Debug)]
pub enum ReadError {
    NotEXR,
    Invalid(Invalid),
    UnknownAttributeType { bytes_to_skip: u32 },

    IoError(::std::io::Error),
    CompressionError(compress::Error),
}

pub mod validity {
    // TODO put validation into own module
    pub type Validity = Result<(), Invalid>;

    #[derive(Debug, Clone, Copy)]
    pub enum Invalid {
        Missing(Value),
        NotSupported(&'static str),
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
}



use self::validity::*;

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


        self.version.validate()?;
        for header in &self.headers {
            header.validate(self.version)?;
        }

        Ok(())
    }
}

impl Header {
    pub fn channels(&self) -> &ChannelList {
        self.attributes.get(self.indices.channels.expect("`channels` attribute index missing"))
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
        self.attributes.get(self.indices.compression.expect("`compression` attribute index missing"))
            .expect("invalid `compression` attribute index")
            .value.to_compression()
            .expect("check failed: `compression` attribute has wrong type")
    }

    pub fn data_window(&self) -> I32Box2 {
        self.attributes.get(self.indices.data_window.expect("`dataWindow` attribute index missing"))
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
        let compression = self.indices.compression
            .ok_or(Invalid::Missing(Value::Attribute("compression")))?;

        self.attributes.get(compression)
            .expect("invalid compression attribute index")
            .value.to_compression()?;


        let data_window = self.indices.data_window
            .ok_or(Invalid::Missing(Value::Attribute("dataWindow")))?;

        self.attributes.get(data_window)
            .expect("invalid data_window attribute index")
            .value.to_i32_box_2()?;


        let channels = self.indices.channels
            .ok_or(Invalid::Missing(Value::Attribute("channels")))?;

        self.attributes.get(channels).expect("invalid channels attribute index")
            .value.to_channel_list()?;


        if let Some(tiles) = self.indices.tiles {
            self.attributes.get(tiles)
                .expect("invalid tiles attribute index")
                .value.to_tile_description()?;
        }

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

        if let Some(version) = self.indices.version {
            let version = self.attributes.get(version)
                .expect("invalid version attribute index")
                .value.to_i32()?;

            if version != 1 {
                return Err(Invalid::NotSupported("deep data version other than 1"));
            }
        }


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

            // make maxSamplesPerPixel optional because some files don't have it
            /*if self.indices.max_samples_per_pixel.is_none() {
                return Err(Invalid::Missing(Value::Attribute("maxSamplesPerPixel (for deepdata)")).into());
            }*/

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
    pub fn byte_size(self) -> usize {
        0_u32.byte_size()
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        use ::bit_field::BitField;

        let version_and_flags = u32::read(read)?;

        // take the 8 least significant bits, they contain the file format version number
        let version = (version_and_flags & 0x000F) as u8;

        // the 24 most significant bits are treated as a set of boolean flags
        let is_single_tile = version_and_flags.get_bit(9);
        let has_long_names = version_and_flags.get_bit(10);
        let has_deep_data = version_and_flags.get_bit(11);
        let has_multiple_parts = version_and_flags.get_bit(12);

        // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0
        // if a file has any of these bits set to 1, it means this file contains
        // a feature that we don't support
        let unknown_flags = version_and_flags >> 13; // all flags excluding the 12 bits we already parsed

        if unknown_flags != 0 { // TODO test if this correctly detects unsupported files
            return Err(Invalid::NotSupported("version flags").into());
        }

        let version = Version {
            file_format_version: version,
            is_single_tile, has_long_names,
            has_deep_data, has_multiple_parts,
        };

        version.validate()?;
        Ok(version)
    }

    pub fn write<W: Write>(self, write: &mut W) -> WriteResult {
        use ::bit_field::BitField;

        self.validate()?;

        // the 8 least significant bits contain the file format version number
        // and the flags are set to 0
        let mut version_and_flags = self.file_format_version as u32;

        // the 24 most significant bits are treated as a set of boolean flags
        version_and_flags.set_bit(9, self.is_single_tile);
        version_and_flags.set_bit(10, self.has_long_names);
        version_and_flags.set_bit(11, self.has_deep_data);
        version_and_flags.set_bit(12, self.has_multiple_parts);
        // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0

        version_and_flags.write(write)
    }

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


/// Enable using the `?` operator on io::Result
impl From<::std::io::Error> for ReadError {
    fn from(io_err: ::std::io::Error) -> Self {
        panic!("give me that nice stack trace like you always do: {}", io_err); // TODO remove
        // ReadError::IoError(io_err)
    }
}

/// Enable using the `?` operator on compress::Result
impl From<compress::Error> for ReadError {
    fn from(compress_err: compress::Error) -> Self {
        ReadError::CompressionError(compress_err)
    }
}

/// Enable using the `?` operator on Validity
impl From<Invalid> for ReadError {
    fn from(err: Invalid) -> Self {
        ReadError::Invalid(err)
    }
}


/// enable using the `?` operator on io errors
impl From<::std::io::Error> for WriteError {
    fn from(err: ::std::io::Error) -> Self {
        WriteError::IoError(err)
    }
}

/// Enable using the `?` operator on Validity
impl From<Invalid> for WriteError {
    fn from(err: Invalid) -> Self {
        WriteError::Invalid(err)
    }
}


pub mod io {
    pub use ::std::io::{Read, Write, Seek, SeekFrom};
    pub use ::seek_bufread::BufReader as SeekBufRead;
    pub use super::{WriteResult, ReadResult, WriteError, ReadError};
    use ::byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt, ByteOrder};

    // will be inlined
    /// extension trait for primitive types like numbers and arrays
    pub trait Data: Sized {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult;
        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self>;

        // TODO make static
        fn byte_size(self) -> usize { ::std::mem::size_of::<Self>() }
    }

    impl Data for u8 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_u8(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_u8().map_err(ReadError::from)
        }
    }

    impl Data for u32 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_u32::<LittleEndian>(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_u32::<LittleEndian>().map_err(ReadError::from)
        }
    }

    impl Data for u64 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_u64::<LittleEndian>(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_u64::<LittleEndian>().map_err(ReadError::from)
        }
    }

    impl Data for i64 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_i64::<LittleEndian>(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_i64::<LittleEndian>().map_err(ReadError::from)
        }
    }

    impl Data for i8 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_i8(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_i8().map_err(ReadError::from)
        }
    }

    impl Data for i32 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_i32::<LittleEndian>(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_i32::<LittleEndian>().map_err(ReadError::from)
        }
    }

    impl Data for f32 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_f32::<LittleEndian>(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_f32::<LittleEndian>().map_err(ReadError::from)
        }
    }

    impl Data for f64 {
        fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
            write.write_f64::<LittleEndian>(self).map_err(WriteError::from)
        }

        fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
            read.read_f64::<LittleEndian>().map_err(ReadError::from)
        }
    }




    // TODO make these instance functions?

    pub fn write_u8_array<W: Write>(write: &mut W, bytes: &[u8]) -> WriteResult {
        write.write_all(bytes).map_err(WriteError::from)
    }

    pub fn write_i32_sized_u8_array<W: Write>(write: &mut W, bytes: &[u8]) -> WriteResult {
        (bytes.len() as i32).write(write)?;
        write_u8_array(write, bytes)
    }

    // TODO test
    pub fn write_f32_array<W: WriteBytesExt>(write: &mut W, array: &mut [f32]) -> WriteResult {
        LittleEndian::from_slice_f32(array); // convert data to little endian
        write_u8_array(write, unsafe {
            ::std::slice::from_raw_parts(
                array.as_ptr() as *const u8,
                array.len() * ::std::mem::size_of::<f32>()
            )
        })
    }

    // TODO test
    pub fn write_i32_array<W: Write>(write: &mut W, array: &mut [i32]) -> WriteResult {
        LittleEndian::from_slice_i32(array); // convert data to little endian
        write_u8_array(write, unsafe {
            ::std::slice::from_raw_parts(
                array.as_ptr() as *const u8,
                array.len() * ::std::mem::size_of::<i32>()
            )
        })
    }

    // TODO test
    pub fn write_u64_array<W: Write>(write: &mut W, array: &mut [u64]) -> WriteResult {
        LittleEndian::from_slice_u64(array); // convert data to little endian
        write_u8_array(write, unsafe {
            ::std::slice::from_raw_parts(
                array.as_ptr() as *const u8,
                array.len() * ::std::mem::size_of::<u64>()
            )
        })
    }

    // TODO test
    pub fn write_i8_array<W: Write>(write: &mut W, array: &[i8]) -> WriteResult {
        // single bytes don't need shuffling to little endian
        // reinterpret the i8 array as bytes, in order to write it
        write_u8_array(write, unsafe {
            ::std::slice::from_raw_parts(
                array.as_ptr() as *const u8,
                array.len()
            )
        })
    }


    pub fn read_u8_array<R: Read>(read: &mut R, array: &mut [u8]) -> ReadResult<()> {
        read.read_exact(array).map_err(ReadError::from)
    }

    // TODO test
    pub fn read_i8_array<R: Read>(read: &mut R, array: &mut [i8]) -> ReadResult<()> {
        let as_u8 = unsafe {
            ::std::slice::from_raw_parts_mut(
                array.as_mut_ptr() as *mut u8,
                array.len()
            )
        };

        read.read_exact(as_u8).map_err(ReadError::from)
    }

    pub fn read_f32_array<R: ReadBytesExt>(read: &mut R, array: &mut [f32]) -> ReadResult<()> {
        read.read_f32_into::<LittleEndian>(array).map_err(ReadError::from)
    }

    pub fn read_i32_vec<R: ReadBytesExt>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<i32>> {
        if data_size < estimated_max {
            let mut data = vec![0; data_size];
            read.read_i32_into::<LittleEndian>(&mut data)?;
            data.shrink_to_fit();
            Ok(data)

        } else {
            println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

            // be careful for suspiciously large data,
            // as reading the pixel_data_size could have gone wrong
            // (read byte by byte to avoid allocating too much memory at once,
            // assuming that it will fail soon, when the file ends)
            let mut data = vec![0; estimated_max];
            read.read_i32_into::<LittleEndian>(&mut data)?;

            for _ in estimated_max..data_size {
                data.push(i32::read(read)?);
            }

            data.shrink_to_fit();
            Ok(data)
        }
    }

    pub fn read_u64_vec<R: ReadBytesExt>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<u64>> {
        if data_size < estimated_max {
            let mut data = vec![0; data_size];
            read.read_u64_into::<LittleEndian>(&mut data)?;
            data.shrink_to_fit();
            Ok(data)

        } else {
            println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

            // be careful for suspiciously large data,
            // as reading the pixel_data_size could have gone wrong
            // (read byte by byte to avoid allocating too much memory at once,
            // assuming that it will fail soon, when the file ends)
            let mut data = vec![0; estimated_max];
            read.read_u64_into::<LittleEndian>(&mut data)?;

            for _ in estimated_max..data_size {
                data.push(u64::read(read)?);
            }

            data.shrink_to_fit();
            Ok(data)
        }
    }

    pub fn read_i8_vec<R: Read>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<i8>> {
        if data_size < estimated_max {
            let mut data = vec![0; data_size];
            read_i8_array(read, &mut data)?;
            data.shrink_to_fit();
            Ok(data)

        } else {
            println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

            // be careful for suspiciously large data,
            // as reading the pixel_data_size could have gone wrong
            // (read byte by byte to avoid allocating too much memory at once,
            // assuming that it will fail soon, when the file ends)
            let mut data = vec![0; estimated_max];
            read_i8_array(read, &mut data)?;

            for _ in estimated_max..data_size {
                data.push(i8::read(read)?);
            }

            data.shrink_to_fit();
            Ok(data)
        }
    }

    /// reuses the allocated buffer, does not shrink to fit
    pub fn reuse_read_u8_vec<R: Read>(read: &mut R, mut data: Vec<u8>, data_size: usize, estimated_max: usize) -> ReadResult<Vec<u8>> {
        if data_size < estimated_max {
            data.resize(data_size, 0);
            read_u8_array(read, &mut data)?;
            Ok(data)

        } else {
            println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

            // be careful for suspiciously large data,
            // as reading the pixel_data_size could have gone wrong
            // (read byte by byte to avoid allocating too much memory at once,
            // assuming that it will fail soon, when the file ends)
            data.resize(estimated_max, 0);
            read.read_exact(&mut data)?;

            for _ in estimated_max..data_size {
                data.push(u8::read(read)?);
            }

            Ok(data)
        }
    }

    pub fn read_u8_vec<R: Read>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<u8>> {
        if data_size < estimated_max {
            let mut data = vec![0; data_size];
            read_u8_array(read, &mut data)?;
            data.shrink_to_fit();
            Ok(data)

        } else {
            println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

            // be careful for suspiciously large data,
            // as reading the pixel_data_size could have gone wrong
            // (read byte by byte to avoid allocating too much memory at once,
            // assuming that it will fail soon, when the file ends)
            let mut data = vec![0; estimated_max];
            read.read_exact(&mut data)?;

            for _ in estimated_max..data_size {
                data.push(u8::read(read)?);
            }

            data.shrink_to_fit();
            Ok(data)
        }
    }

    pub fn read_i32_sized_u8_vec<R: Read>(read: &mut R, estimated_max: usize) -> ReadResult<Vec<u8>> {
        let data_size = i32::read(read)? as usize;
        read_u8_vec(read, data_size, estimated_max)
    }





    pub struct MagicNumber;
    impl MagicNumber {
        pub const BYTES: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];
    }

    impl MagicNumber {
        pub fn write<W: Write>(write: &mut W) -> WriteResult {
            write_u8_array(write, &Self::BYTES)
        }

        pub fn is_exr<R: Read>(read: &mut R) -> ReadResult<bool> {
            let mut magic_num = [0; 4];
            read_u8_array(read, &mut magic_num)?;
            Ok(magic_num == Self::BYTES)
        }

        pub fn validate_exr<R: Read>(read: &mut R) -> ReadResult<()> {
            if Self::is_exr(read)? {
                Ok(())

            } else {
                Err(ReadError::NotEXR)
            }
        }
    }


    pub struct SequenceEnd;
    impl SequenceEnd {
        pub fn byte_size() -> usize {
            1
        }

        pub fn write<W: Write>(write: &mut W) -> WriteResult {
            0_u8.write(write)
        }

        pub fn has_come<R: Read + Seek>(read: &mut R) -> ReadResult<bool> {
            if u8::read(read)? == 0 {
                Ok(true)

            } else {
                // go back that wasted byte because its not 0
                // TODO benchmark peeking the buffer performance
                read.seek(SeekFrom::Current(-1))?;
                Ok(false)
            }
        }
    }


    use super::*;

    impl Header {
        pub fn write_all<W: Write>(headers: &Headers, write: &mut W, version: Version) -> WriteResult {
            let has_multiple_headers = headers.len() != 1;
            if headers.is_empty() || version.has_multiple_parts != has_multiple_headers {
                // TODO return combination?
                return Err(Invalid::Content(Value::Part("headers count"), Required::Exact("1")).into());
            }

            for header in headers {
                debug_assert!(header.validate(version).is_ok(), "check failed: header invalid");

                for attrib in &header.attributes {
                    attrib.write(write, version.has_long_names)?;
                }
                SequenceEnd::write(write)?;

            }
            SequenceEnd::write(write)?;

            Ok(())
        }


        pub fn read_all<R: Read + Seek>(read: &mut R, version: Version) -> ReadResult<Headers> {
            Ok({
                if !version.has_multiple_parts {
                    SmallVec::from_elem(Header::read(read, version)?, 1)

                } else {
                    let mut headers = SmallVec::new();
                    while !SequenceEnd::has_come(read)? {
                        headers.push(Header::read(read, version)?);
                    }

                    headers
                }
            })
        }

        pub fn read<R: Read + Seek>(read: &mut R, format_version: Version) -> ReadResult<Self> {
            let mut attributes = SmallVec::new();

            // these required attributes will be Some(usize) when encountered while parsing
            let mut tiles = None;
            let mut name = None;
            let mut kind = None;
            let mut version = None;
            let mut chunk_count = None;
            let mut max_samples_per_pixel = None;
            let mut channels = None;
            let mut compression = None;
            let mut data_window = None;
            let mut display_window = None;
            let mut line_order = None;
            let mut pixel_aspect = None;
            let mut screen_window_center = None;
            let mut screen_window_width = None;
            let mut chromaticities = SmallVec::new();

            while !SequenceEnd::has_come(read)? {
                match Attribute::read(read) {
                    // skip unknown attribute values
                    Err(ReadError::UnknownAttributeType { bytes_to_skip }) => {
                        read.seek(SeekFrom::Current(bytes_to_skip as i64))?;
                    },

                    Err(other_error) => return Err(other_error),

                    Ok(attribute) => {
                        // save index when a required attribute is encountered
                        let index = attributes.len();

                        // TODO replace these literals with constants
                        use ::file::attributes::required::*;
                        match attribute.name.bytes.as_slice() {
                            TILES => tiles = Some(index),
                            NAME => name = Some(index),
                            TYPE => kind = Some(index),
                            VERSION => version = Some(index),
                            CHUNKS => chunk_count = Some(index),
                            MAX_SAMPLES => max_samples_per_pixel = Some(index),
                            CHANNELS => channels = Some(index),
                            COMPRESSION => compression = Some(index),
                            DATA_WINDOW => data_window = Some(index),
                            DISPLAY_WINDOW => display_window = Some(index),
                            LINE_ORDER => line_order = Some(index),
                            PIXEL_ASPECT => pixel_aspect = Some(index),
                            WINDOW_CENTER => screen_window_center = Some(index),
                            WINDOW_WIDTH => screen_window_width = Some(index),
                            _ => {},
                        }

                        if attribute.value.to_chromaticities().is_ok() {
                            chromaticities.push(index);
                        }

                        attributes.push(attribute)
                    }
                }
            }

            let header = Header {
                attributes,
                indices: AttributeIndices {
                    channels, compression, data_window,
                    display_window, line_order, pixel_aspect,
                    screen_window_center, screen_window_width,
                    chromaticities,

                    tiles,
                    name, kind,
                    version, chunk_count,
                    max_samples_per_pixel,
                },
            };

//            println!("{:#?}", header);
            header.validate(format_version)?;
            Ok(header)
        }
    }

    // TODO make instance fn
    pub fn read_offset_table<R: Seek + Read>(
        read: &mut R, version: Version, header: &Header
    ) -> ReadResult<OffsetTable>
    {
        let entry_count: u32 = {
            if let Some(chunk_count) = header.chunk_count() {
                chunk_count as u32 // TODO will this panic on negative number / invalid data?

            } else {
                debug_assert!(
                    !version.has_multiple_parts,
                    "Multi-Part header does not have chunkCount, should have been checked"
                );

                // If not multipart and the chunkCount is not present,
                // the number of entries in the chunk table is computed
                // using the dataWindow and tileDesc attributes and the compression format
                let compression = header.compression();
                let data_window = header.data_window();
                data_window.validate()?;

                let (data_width, data_height) = data_window.dimensions();

                if let Some(tiles) = header.tiles() {
                    let round = tiles.rounding_mode;
                    let (tile_width, tile_height) = tiles.dimensions();

                    // calculations inspired by
                    // https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp

                    let level_count = |full_res: u32| {
                        round.log2(full_res + 1) + 1
                    };

                    let level_size = |full_res: u32, level_index: u32| {
                        round.divide(full_res + 1, 1 << level_index).max(1)
                    };

                    fn tile_count(full_res: u32, tile_size: u32) -> u32 {
                        // round up, because if the image is not evenly divisible by the tiles,
                        // we add another tile at the end (which is only partially used)
                        RoundingMode::Up.divide(full_res, tile_size)
                    }

                    use ::file::attributes::LevelMode::*;
                    match tiles.level_mode {
                        One => {
                            tile_count(data_width, tile_width) * tile_count(data_height, tile_height)
                        },

                        MipMap => {
                            // sum all tiles per level
                            // note: as levels shrink, tiles stay the same pixel size.
                            // so at lower levels, tiles cover up a bigger are of the smaller image
                            (0..level_count(data_width.max(data_height))).map(|level_index|{
                                let tile_count_x = tile_count(level_size(data_width, level_index), tile_width);
                                let tile_count_y = tile_count(level_size(data_height, level_index), tile_height);
                                tile_count_x * tile_count_y
                            }).sum()
                        },

                        RipMap => {
                            // TODO test this
                            (0..level_count(data_width)).map(|level_x_index|{
                                (0..level_count(data_height)).map(|level_y_index| {
                                    let tile_count_x = tile_count(level_size(data_width, level_x_index), tile_width);
                                    let tile_count_y = tile_count(level_size(data_height, level_y_index), tile_height);
                                    tile_count_x * tile_count_y
                                }).sum::<u32>()
                            }).sum()
                        }
                    }

                } else { // scanlines
                    let lines_per_block = compression.scan_lines_per_block() as u32;
                    (data_height + lines_per_block) / lines_per_block
                }
            }
        };

        read_u64_vec(read, entry_count as usize, ::std::u16::MAX as usize)
    }

    fn read_offset_tables<R: Seek + Read>(
        read: &mut R, version: Version, headers: &Headers,
    ) -> ReadResult<OffsetTables>
    {
        let mut tables = SmallVec::new();

        for i in 0..headers.len() {
            // one offset table for each header
            tables.push(read_offset_table(read, version, &headers[i])?);
        }

        Ok(tables)
    }

    pub fn write_offset_tables<W: Write>(write: &mut W, tables: &OffsetTables) -> WriteResult {
        for table in tables {
            write_u64_array(write, &mut table.clone())?; // TODO without clone at least on little endian machines
        }

        Ok(())
    }


    impl MetaData {
        pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
            self.validate()?;
            self.version.write(write)?;
            Header::write_all(&self.headers, write, self.version)?;

            println!("calculate tables???");
            write_offset_tables(write, &self.offset_tables)
        }

        pub fn read<R: Read + Seek>(read: &mut R) -> ReadResult<Self> {
            let version = Version::read(read)?;
            let headers = Header::read_all(read, version)?;
            let offset_tables = read_offset_tables(read, version, &headers)?;

            // TODO check if supporting version 2 implies supporting version 1
            Ok(MetaData { version, headers, offset_tables })
        }
    }


    #[must_use]
    pub fn read_file(path: &::std::path::Path) -> ReadResult<RawImage> {
        read(::std::fs::File::open(path)?)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn read<R: Read + Seek>(unbuffered: R) -> ReadResult<RawImage> {
        read_seekable_buffer(&mut SeekBufRead::new(unbuffered))
    }

    #[must_use]
    pub fn read_seekable_buffer<R: Read + Seek>(read: &mut SeekBufRead<R>) -> ReadResult<RawImage> {
        MagicNumber::validate_exr(read)?;
        let meta_data = MetaData::read(read)?;
        let chunks = Chunks::read(read, &meta_data)?;
        Ok(::file::RawImage { meta_data, chunks, })
    }





    #[must_use]
    pub fn write_file(path: &str, image: &RawImage) -> WriteResult {
        write(&mut ::std::fs::File::open(path)?, image)
    }

    #[must_use]
    pub fn write<W: Write>(write: &mut W, image: &RawImage) -> WriteResult {
        MagicNumber::write(write)?;
        image.meta_data.write(write)?;
        image.chunks.write(write, &image.meta_data)
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
