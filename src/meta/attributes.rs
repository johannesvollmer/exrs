
//! Contains all meta data attributes.
//! Each layer can have any number of [`Attribute`]s, including custom attributes.

use smallvec::SmallVec;

/// A named attribute value.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {

    /// Identifier of the attribute.
    pub name: Text,

    /// Content of the attribute.
    pub value: AnyValue,
}

/// Contains one of all possible attributes.
/// Includes a variant for custom attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum AnyValue {

    /// Channel meta data.
    ChannelList(ChannelList),

    /// Color space definition.
    Chromaticities(Chromaticities),

    /// Compression method of this layer.
    Compression(Compression),

    /// This image is an environment map.
    EnvironmentMap(EnvironmentMap),

    /// Film roll information.
    KeyCode(KeyCode),

    /// Order of the bocks in the file.
    LineOrder(LineOrder),

    /// A 3x3 matrix of floats.
    F32Matrix3x3([f32; 9]),

    /// A 4x4 matrix of floats.
    F32Matrix4x4([f32; 16]),

    /// 8-bit RGBA Preview of the image.
    Preview(Preview),

    /// A number a divided by number b.
    Rational((i32, u32)),

    /// Deep or flat and tiled or scan line.
    BlockType(BlockType),

    /// List of texts.
    TextVector(Vec<Text>),

    /// How to tile up the image.
    TileDescription(TileDescription),

    /// Timepoint and more.
    TimeCode(TimeCode),

    /// ASCII String.
    Text(Text),

    /// 64-bit float
    F64(f64),

    /// 32-bit float
    F32(f32),

    /// 32-bit signed integer
    I32(i32),

    /// 2D integer rectangle.
    IntRect(IntRect),

    /// 2D float rectangle.
    FloatRect(FloatRect),

    /// 2D integer vector.
    IntVec2(Vec2<i32>),

    /// 2D float vector.
    FloatVec2(Vec2<f32>),

    /// 3D integer vector.
    IntVec3((i32, i32, i32)),

    /// 3D float vector.
    FloatVec3((f32, f32, f32)),

    /// A custom attribute.
    /// Contains the type name of this value.
    Custom {

        /// The name of the type this attribute is an instance of.
        kind: Text,

        /// The value, stored in little-endian byte order, of the value.
        /// Use the `exr::io::Data` trait to extract binary values from this vector.
        bytes: Vec<u8>
    }
}

/// A byte array with each byte being a char.
/// This is not UTF an must be constructed from a standard string.
// TODO is this ascii? use a rust ascii crate?
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Text {
    bytes: TextBytes,
}

/// Contains time information.
// TODO enable conversion to rust time
#[derive(Copy, Debug, Clone, Eq, PartialEq, Hash)]
pub struct TimeCode {
    time_and_flags: u32,
    user_data: u32,
}

/// layer type, specifies block type and deepness.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BlockType {

    /// Corresponds to the string value `scanlineimage`.
    ScanLine,

    /// Corresponds to the string value `tiledimage`.
    Tile,

    /// Corresponds to the string value `deepscanline`.
    DeepScanLine,

    /// Corresponds to the string value `deeptile`.
    DeepTile,
}

/// The string literals used to represent a `BlockType` in a file.
pub mod block_type_strings {

    /// Type attribute text value of flat scan lines
    pub const SCAN_LINE: &'static [u8] = b"scanlineimage";

    /// Type attribute text value of flat tiles
    pub const TILE: &'static [u8] = b"tiledimage";

    /// Type attribute text value of deep scan lines
    pub const DEEP_SCAN_LINE: &'static [u8] = b"deepscanline";

    /// Type attribute text value of deep tiles
    pub const DEEP_TILE: &'static [u8] = b"deeptile";
}


pub use crate::compression::Compression;

/// The integer rectangle describing where an layer is placed on the infinite 2D global space.
pub type DataWindow = IntRect;

/// The integer rectangle limiting part of the infinite 2D global space should be displayed.
pub type DisplayWindow = IntRect;

/// A rectangular section anywhere in 2D integer space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntRect {

    /// The bottom left corner of this rectangle.
    /// The `Box2I32` includes this pixel if the size is not zero.
    pub position: Vec2<i32>,

    /// How many pixels to include in this `Box2I32`.
    /// Does not include the actual boundary, just like `Vec::len()`.
    pub size: Vec2<usize>,
}

/// A rectangular section anywhere in 2D float space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatRect {
    min: Vec2<f32>,
    max: Vec2<f32>
}

/// A List of channels. Channels must be sorted alphabetically.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelList {

    /// The channels in this list.
    pub list: SmallVec<[Channel; 5]>,

    /// The number of bytes that one pixel in this image needs.
    // FIXME this needs to account for subsampling anywhere?
    pub bytes_per_pixel: usize, // FIXME only makes sense for flat images!
}

/// A single channel in an layer.
/// Does not contain the actual pixel data,
/// but instead merely describes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Channel {

    /// One of "R", "G", or "B" most of the time.
    pub name: Text,

    /// U32, F16 or F32.
    pub pixel_type: PixelType,

    /// Are the samples in this channel in a linear space or not?
    pub is_linear: bool,

    /// How many of the samples are skipped compared to the other channels in this layer.
    ///
    /// Can be used for chroma subsampling for manual lossy data compression.
    /// Values other than 1 are allowed only in flat, scan-line based images.
    /// If an image is deep or tiled, x and y sampling rates for all of its channels must be 1.
    pub sampling: Vec2<usize>,
}

/// What kind of pixels are in this channel.
#[derive(Clone, Debug, Eq, PartialEq, Copy)]
pub enum PixelType {

    /// This channel contains 32-bit unsigned int values.
    U32,

    /// This channel contains 16-bit float values.
    F16,

    /// This channel contains 32-bit float values.
    F32,
}

/// The color space of the pixels.
///
/// If a file doesn't have a chromaticities attribute, display software
/// should assume that the file's primaries and the white point match `Rec. ITU-R BT.709-3`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chromaticities {

    /// "Red" location on the CIE XY chromaticity diagram.
    pub red: Vec2<f32>,

    /// "Green" location on the CIE XY chromaticity diagram.
    pub green: Vec2<f32>,

    /// "Blue" location on the CIE XY chromaticity diagram.
    pub blue: Vec2<f32>,

    /// "White" location on the CIE XY chromaticity diagram.
    pub white: Vec2<f32>
}

/// If this attribute is present, it describes
/// how this texture should be projected onto an environment.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EnvironmentMap {

    /// This image is an environment map projected like a world map.
    LatitudeLongitude,

    /// This image contains the six sides of a cube.
    Cube,
}

/// Uniquely identifies a motion picture film frame.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct KeyCode {

    /// Identifies a film manufacturer.
    pub film_manufacturer_code: i32,

    /// Identifies a film type.
    pub film_type: i32,

    /// Specifies the film roll prefix.
    pub film_roll_prefix: i32,

    /// Specifies the film count.
    pub count: i32,

    /// Specifies the perforation offset.
    pub perforation_offset: i32,

    /// Specifies the perforation count of each single frame.
    pub perforations_per_frame: i32,

    /// Specifies the perforation count of each single film.
    pub perforations_per_count: i32,
}

/// In what order the `Block`s of pixel data appear in a file.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LineOrder {

    /// The blocks in the file are ordered in descending rows from left to right.
    /// When compressing in parallel, this option requires potentially large amounts of memory.
    /// In that case, use `LineOrder::Unspecified` for best performance.
    Increasing,

    /// The blocks in the file are ordered in ascending rows from right to left.
    /// When compressing in parallel, this option requires potentially large amounts of memory.
    /// In that case, use `LineOrder::Unspecified` for best performance.
    Decreasing,

    /// The blocks are not ordered in a specific way inside the file.
    /// In multicore file writing, this option offers the best performance.
    Unspecified,
}

/// A small `rgba` image of `i8` values that approximates the real exr image.
// TODO is this linear?
#[derive(Clone, Eq, PartialEq)]
pub struct Preview {

    /// The dimensions of the preview image.
    pub size: Vec2<usize>,

    /// An array with a length of 4 × width × height.
    /// The pixels are stored in `LineOrder::Increasing`.
    /// Each pixel consists of the four `u8` values red, green, blue, alpha.
    pub pixel_data: Vec<i8>,
}

/// Describes how the layer is divided into tiles.
/// Specifies the size of each tile in the image
/// and whether this image contains multiple resolution levels.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TileDescription {

    /// The size of each tile.
    /// Stays the same number of pixels across all levels.
    pub tile_size: Vec2<usize>,

    /// Whether to also store smaller versions of the image.
    pub level_mode: LevelMode,

    /// Whether to round up or down when calculating Mip/Rip levels.
    pub rounding_mode: RoundingMode,
}

/// Whether to also store increasingly smaller versions of the original image.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LevelMode {

    /// Only a single level.
    Singular,

    /// Levels with a similar aspect ratio.
    MipMap,

    /// Levels with all possible aspect ratios.
    RipMap,
}


/// The raw bytes that make up a string in an exr file.
/// Each `u8` is a single char.
// will mostly be "R", "G", "B" or "deepscanlineimage"
type TextBytes = SmallVec<[u8; 24]>;



use crate::io::*;
use crate::meta::sequence_end;
use crate::error::*;
use crate::math::{RoundingMode, Vec2};
use half::f16;
use std::convert::{TryFrom};


fn invalid_type() -> Error {
    Error::invalid("attribute type mismatch")
}


impl Text {

    /// The internal ASCII bytes this text is made of.
    pub fn bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Create a `Text` from an `str` reference.
    /// Returns `None` if this string contains unsupported chars.
    pub fn from(str: impl AsRef<str>) -> Option<Self> {
        let vec : Option<TextBytes> = str.as_ref().chars()
            .map(|character| u8::try_from(character as u64).ok())
            .collect();

        vec.map(Self::from_bytes_unchecked)
    }

    /// Create a `Text` from the specified bytes object,
    /// without checking any of the bytes.
    pub fn from_bytes_unchecked(bytes: TextBytes) -> Self {
        Text { bytes }
    }

    /// Check whether this string is valid, considering the maximum text length.
    pub fn validate(&self, null_terminated: bool, long_names: Option<bool>) -> UnitResult {
        Self::validate_bytes(self.bytes(), null_terminated, long_names)
    }

    /// Check whether some bytes are valid, considering the maximum text length.
    pub fn validate_bytes(text: &[u8], null_terminated: bool, long_names: Option<bool>) -> UnitResult {
        if null_terminated && text.is_empty() {
            return Err(Error::invalid("text must not be empty"));
        }

        if let Some(long) = long_names {
            if long && text.len() >= 256 { return Err(Error::invalid("text must not be longer than 255")); }
            if !long && text.len() >= 32 { return Err(Error::invalid("text must not be longer than 31")); }
        }

        Ok(())
    }

    /// The byte count this string would occupy if it were encoded as a null-terminated string.
    pub fn null_terminated_byte_size(&self) -> usize {
        self.bytes.len() + sequence_end::byte_size()
    }

    /// The byte count this string would occupy if it were encoded as a size-prefixed string.
    pub fn i32_sized_byte_size(&self) -> usize {
        self.bytes.len() + i32::BYTE_SIZE
    }

    /// Write the length of a string and then the contents with that length.
    pub fn write_i32_sized<W: Write>(&self, write: &mut W) -> UnitResult {
        debug_assert!(self.validate( false, None).is_ok(), "text size bug");
        i32::write(usize_to_i32(self.bytes.len()), write)?;
        Self::write_unsized_bytes(self.bytes.as_slice(), write)
    }

    /// Without validation, write this instance to the byte stream.
    fn write_unsized_bytes<W: Write>(bytes: &[u8], write: &mut W) -> UnitResult {
        u8::write_slice(write, bytes)?;
        Ok(())
    }

    /// Read the length of a string and then the contents with that length.
    pub fn read_i32_sized<R: Read>(read: &mut R, max_size: usize) -> Result<Self> {
        let size = i32_to_usize(i32::read(read)?, "vector size")?;
        Ok(Text::from_bytes_unchecked(SmallVec::from_vec(u8::read_vec(read, size, 1024, Some(max_size))?)))
    }

    /// Read the contents with that length.
    pub fn read_sized<R: Read>(read: &mut R, size: usize) -> Result<Self> {
        const SMALL_SIZE: usize  = 24;

        // for small strings, read into small vec without heap allocation
        if size <= SMALL_SIZE {
            let mut buffer = [0_u8; SMALL_SIZE];
            let data = &mut buffer[..size];

            read.read_exact(data)?;
            Ok(Text::from_bytes_unchecked(SmallVec::from_slice(data)))
        }

        // for large strings, read a dynamic vec of arbitrary size
        else {
            Ok(Text::from_bytes_unchecked(SmallVec::from_vec(u8::read_vec(read, size, 1024, None)?)))
        }
    }

    /// Write the string contents and a null-terminator.
    pub fn write_null_terminated<W: Write>(&self, write: &mut W) -> UnitResult {
        Self::write_null_terminated_bytes(self.bytes(), write)
    }

    /// Write the string contents and a null-terminator.
    fn write_null_terminated_bytes<W: Write>(bytes: &[u8], write: &mut W) -> UnitResult {
        debug_assert!(!bytes.is_empty(), "text is empty bug"); // required to avoid mixup with "sequece_end"

        Text::write_unsized_bytes(bytes, write)?;
        sequence_end::write(write)?;
        Ok(())
    }

    /// Read a string until the null-terminator is found. Then skips the null-terminator.
    pub fn read_null_terminated<R: Read>(read: &mut R, max_len: usize) -> Result<Self> {
        let mut bytes = SmallVec::new();

        loop {
            match u8::read(read)? {
                0 => break,
                non_terminator => bytes.push(non_terminator),
            }

            if bytes.len() > max_len {
                return Err(Error::invalid("text too long"))
            }
        }

        Ok(Text { bytes })
    }

    /// Allows any text length since it is only used for attribute values,
    /// but not attribute names, attribute type names, or channel names.
    fn read_vec_of_i32_sized(
        read: &mut PeekRead<impl Read>,
        total_byte_size: usize
    ) -> Result<Vec<Text>>
    {
        let mut result = Vec::with_capacity(2);

        // length of the text-vector can be inferred from attribute size
        let mut processed_bytes = 0;

        while processed_bytes < total_byte_size {
            let text = Text::read_i32_sized(read, total_byte_size)?;
            processed_bytes += ::std::mem::size_of::<i32>(); // size i32 of the text
            processed_bytes += text.bytes.len();
            result.push(text);
        }

        // the expected byte size did not match the actual text byte size
        if processed_bytes != total_byte_size {
            return Err(Error::invalid("text array byte size"))
        }

        Ok(result)
    }

    /// Allows any text length since it is only used for attribute values,
    /// but not attribute names, attribute type names, or channel names.
    fn write_vec_of_i32_sized_texts<W: Write>(write: &mut W, texts: &[Text]) -> UnitResult {
        // length of the text-vector can be inferred from attribute size
        for text in texts {
            text.write_i32_sized(write)?;
        }

        Ok(())
    }
}

impl Into<String> for Text {
    fn into(self) -> String {
        self.to_string()
    }
}

impl<'s> TryFrom<&'s str> for Text {
    type Error = &'static str;

    fn try_from(value: &'s str) -> std::result::Result<Self, Self::Error> {
        Text::from(value).ok_or("exr text does not support unicode characters")
    }
}


impl ::std::fmt::Debug for Text {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "exr::Text(\"{}\")", self)
    }
}

// automatically implements to_string for us
impl ::std::fmt::Display for Text {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        use std::fmt::Write;

        for &byte in self.bytes.iter() {
            f.write_char(byte as char)?;
        }

        Ok(())
    }
}


impl ChannelList {

    /// Does not validate channel order.
    pub fn new(channels: SmallVec<[Channel; 5]>) -> Self {
        ChannelList {
            bytes_per_pixel: channels.iter().map(|channel| channel.pixel_type.bytes_per_sample()).sum(),
            list: channels,
        }
    }
}

impl BlockType {

    /// The corresponding attribute type name literal
    const TYPE_NAME: &'static [u8] = attribute_type_names::TEXT;

    /// Return a `BlockType` object from the specified attribute text value.
    pub fn parse(text: Text) -> Result<Self> {
        match text.bytes() {
            block_type_strings::SCAN_LINE => Ok(BlockType::ScanLine),
            block_type_strings::TILE => Ok(BlockType::Tile),

            block_type_strings::DEEP_SCAN_LINE => Ok(BlockType::DeepScanLine),
            block_type_strings::DEEP_TILE => Ok(BlockType::DeepTile),

            _ => Err(Error::invalid("block type attribute value")),
        }
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write(&self, write: &mut impl Write) -> UnitResult {
        u8::write_slice(write, self.to_text_bytes())?;
        Ok(())
    }

    /// Returns the raw attribute text value this type is represented by in a file.
    pub fn to_text_bytes(&self) -> &[u8] {
        match self {
            BlockType::ScanLine => block_type_strings::SCAN_LINE,
            BlockType::Tile => block_type_strings::TILE,
            BlockType::DeepScanLine => block_type_strings::DEEP_SCAN_LINE,
            BlockType::DeepTile => block_type_strings::DEEP_TILE,
        }
    }

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size(&self) -> usize {
        self.to_text_bytes().len()
    }
}


impl IntRect {

    /// Create a box with no size located at (0,0).
    pub fn zero() -> Self {
        Self::from_dimensions(Vec2(0, 0))
    }

    /// Create a box with a size starting at zero.
    pub fn from_dimensions(size: Vec2<usize>) -> Self {
        Self::new(Vec2(0,0), size)
    }

    /// Create a box with a size and an origin point.
    pub fn new(start: Vec2<i32>, size: Vec2<usize>) -> Self {
        Self { position: start, size }
    }

    /// Returns the top-right coordinate of the rectangle.
    /// The row and column described by this vector are not included in the rectangle,
    /// just like `Vec::len()`.
    pub fn end(self) -> Vec2<i32> {
        self.position + self.size.to_i32() // larger than max int32 is panic
    }

    /// Returns the maximum coordinate that a value in this rectangle may have.
    pub fn max(self) -> Vec2<i32> {
        self.end() - Vec2(1,1)
    }

    /// Validate this instance.
    pub fn validate(&self, max: Vec2<usize>) -> UnitResult {
        if self.size.0 > max.0 || self.size.1 > max.1  {
            return Err(Error::invalid("window attribute dimension value"));
        }

        Ok(())
    }

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        4 * i32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        let Vec2(x_min, y_min) = self.position;
        let Vec2(x_max, y_max) = self.max();

        x_min.write(write)?;
        y_min.write(write)?;
        x_max.write(write)?;
        y_max.write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let x_min = i32::read(read)?;
        let y_min = i32::read(read)?;
        let x_max = i32::read(read)?;
        let y_max = i32::read(read)?;

        let min = Vec2(x_min.min(x_max), y_min.min(y_max));
        let max  = Vec2(x_min.max(x_max), y_min.max(y_max)); // these are inclusive!
        let size = Vec2(max.0 + 1 - min.0, max.1 + 1 - min.1); // which is why we add 1
        let size = size.to_usize("box coordinates")?;

        Ok(IntRect { position: min, size })
    }

    /// Create a new rectangle which is offset by the specified origin.
    pub fn with_origin(self, origin: Vec2<i32>) -> Self {
        IntRect { position: self.position + origin, .. self }
    }
}


impl FloatRect {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        4 * f32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        self.min.0.write(write)?;
        self.min.1.write(write)?;
        self.max.0.write(write)?;
        self.max.1.write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let x_min = f32::read(read)?;
        let y_min = f32::read(read)?;
        let x_max = f32::read(read)?;
        let y_max = f32::read(read)?;

        Ok(FloatRect {
            min: Vec2(x_min, y_min),
            max: Vec2(x_max, y_max)
        })
    }
}

impl PixelType {

    /// How many bytes a single sample takes up.
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            PixelType::F16 => f16::BYTE_SIZE,
            PixelType::F32 => f32::BYTE_SIZE,
            PixelType::U32 => u32::BYTE_SIZE,
        }
    }

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        i32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        match *self {
            PixelType::U32 => 0_i32,
            PixelType::F16 => 1_i32,
            PixelType::F32 => 2_i32,
        }.write(write)?;

        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        // there's definitely going to be more than 255 different pixel types in the future
        Ok(match i32::read(read)? {
            0 => PixelType::U32,
            1 => PixelType::F16,
            2 => PixelType::F32,
            _ => return Err(Error::invalid("pixel type attribute value")),
        })
    }
}

impl Channel {

    /// Create a new channel with the specified properties and a sampling rate of (1,1).
    pub fn new(name: Text, pixel_type: PixelType, is_linear: bool) -> Self {
        Self { name, pixel_type, is_linear, sampling: Vec2(1, 1) }
    }

    /// The count of pixels this channel contains, respecting subsampling.
    // FIXME this must be used everywhere
    pub fn subsampled_pixels(&self, dimensions: Vec2<usize>) -> usize {
        self.subsampled_resolution(dimensions).area()
    }

    /// The resolution pf this channel, respecting subsampling.
    pub fn subsampled_resolution(&self, dimensions: Vec2<usize>) -> Vec2<usize> {
        dimensions / self.sampling
    }

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size(&self) -> usize {
        self.name.null_terminated_byte_size()
            + PixelType::byte_size()
            + 1 // is_linear
            + 3 // reserved bytes
            + 2 * u32::BYTE_SIZE // sampling x, y
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        Text::write_null_terminated(&self.name, write)?;
        self.pixel_type.write(write)?;

        match self.is_linear {
            false => 0_u8,
            true  => 1_u8,
        }.write(write)?;

        i8::write_slice(write, &[0_i8, 0_i8, 0_i8])?;
        i32::write(usize_to_i32(self.sampling.0), write)?;
        i32::write(usize_to_i32(self.sampling.1), write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let name = Text::read_null_terminated(read, 256)?;
        let pixel_type = PixelType::read(read)?;

        let is_linear = match u8::read(read)? {
            1 => true,
            0 => false,
            _ => return Err(Error::invalid("channel linearity attribute value")),
        };

        let mut reserved = [0_i8; 3];
        i8::read_slice(read, &mut reserved)?;

        let x_sampling = i32_to_usize(i32::read(read)?, "x channel sampling")?;
        let y_sampling = i32_to_usize(i32::read(read)?, "y channel sampling")?;

        Ok(Channel {
            name, pixel_type, is_linear,
            sampling: Vec2(x_sampling, y_sampling),
        })
    }

    /// Validate this instance.
    pub fn validate(&self, allow_sampling: bool, data_window: IntRect, strict: bool) -> UnitResult {
        if self.sampling.0 == 0 || self.sampling.1 == 0 {
            return Err(Error::invalid("zero sampling factor"));
        }

        if strict && allow_sampling && self.sampling != Vec2(1,1) {
            return Err(Error::invalid("sub sampling is only allowed in flat scan line images"));
        }

        if data_window.position.0 % self.sampling.0 as i32 != 0 || data_window.position.1 % self.sampling.1 as i32 != 0 {
            return Err(Error::invalid("channel sampling factor not dividing data window position"));
        }

        if data_window.size.0 % self.sampling.0 != 0 || data_window.size.1 % self.sampling.1 != 0 {
            return Err(Error::invalid("channel sampling factor not dividing data window size"));
        }

        if self.sampling != Vec2(1,1) {
            // TODO this must only be implemented in the crate::image module and child modules,
            //      should not be too difficult

            return Err(Error::unsupported("channel sub sampling not supported yet"));
        }

        Ok(())
    }
}

impl ChannelList {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size(&self) -> usize {
        // FIXME this needs to account for subsampling anywhere?
        self.list.iter().map(Channel::byte_size).sum::<usize>() + sequence_end::byte_size()
    }

    /// Without validation, write this instance to the byte stream.
    /// Assumes channels are sorted alphabetically and all values are validated.
    pub fn write(&self, write: &mut impl Write) -> UnitResult {
        for channel in &self.list {
            channel.write(write)?;
        }

        sequence_end::write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read(read: &mut PeekRead<impl Read>) -> Result<Self> {
        let mut channels = SmallVec::new();
        while !sequence_end::has_come(read)? {
            channels.push(Channel::read(read)?);
        }

        Ok(ChannelList::new(channels))
    }

    /// Check if channels are valid and sorted.
    pub fn validate(&self, allow_sampling: bool, data_window: IntRect, strict: bool) -> UnitResult {
        let mut iter = self.list.iter().map(|chan| chan.validate(allow_sampling, data_window, strict).map(|_| &chan.name));
        let mut previous = iter.next().ok_or(Error::invalid("at least one channel is required"))??;

        for result in iter {
            let value = result?;
            if strict && previous == value { return Err(Error::invalid("channel names are not unique")); }
            else if previous > value { return Err(Error::invalid("channel names are not sorted alphabetically")); }
            else { previous = value; }
        }

        Ok(())
    }
}

impl TimeCode {

    /// Number of bytes this would consume in an exr file.
    pub const BYTE_SIZE: usize = 2 * u32::BYTE_SIZE;

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        self.time_and_flags.write(write)?;
        self.user_data.write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let time_and_flags = u32::read(read)?;
        let user_data = u32::read(read)?;
        Ok(Self { time_and_flags, user_data })
    }
}

impl Chromaticities {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        8 * f32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        self.red.0.write(write)?;
        self.red.1.write(write)?;

        self.green.0.write(write)?;
        self.green.1.write(write)?;

        self.blue.0.write(write)?;
        self.blue.1.write(write)?;

        self.white.0.write(write)?;
        self.white.1.write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        Ok(Chromaticities {
            red: Vec2(f32::read(read)?, f32::read(read)?), // TODO does this respect struct init order?
            green: Vec2(f32::read(read)?, f32::read(read)?),
            blue: Vec2(f32::read(read)?, f32::read(read)?),
            white: Vec2(f32::read(read)?, f32::read(read)?),
        })
    }
}

impl Compression {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        1
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(self, write: &mut W) -> UnitResult {
        use self::Compression::*;
        match self {
            Uncompressed => 0_u8,
            RLE => 1_u8,
            ZIP1 => 2_u8,
            ZIP16 => 3_u8,
            PIZ => 4_u8,
            PXR24 => 5_u8,
            B44 => 6_u8,
            B44A => 7_u8,
            DWAA => 8_u8,
            DWAB => 9_u8,
        }.write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        use self::Compression::*;
        Ok(match u8::read(read)? {
            0 => Uncompressed,
            1 => RLE,
            2 => ZIP1,
            3 => ZIP16,
            4 => PIZ,
            5 => PXR24,
            6 => B44,
            7 => B44A,
            8 => DWAA,
            9 => DWAB,
            _ => return Err(Error::unsupported("unknown compression method")),
        })
    }
}

impl EnvironmentMap {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        u32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(self, write: &mut W) -> UnitResult {
        use self::EnvironmentMap::*;
        match self {
            LatitudeLongitude => 0_u8,
            Cube => 1_u8
        }.write(write)?;

        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        use self::EnvironmentMap::*;
        Ok(match u8::read(read)? {
            0 => LatitudeLongitude,
            1 => Cube,
            _ => return Err(Error::invalid("environment map attribute value")),
        })
    }
}

impl KeyCode {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        6 * i32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        self.film_manufacturer_code.write(write)?;
        self.film_type.write(write)?;
        self.film_roll_prefix.write(write)?;
        self.count.write(write)?;
        self.perforation_offset.write(write)?;
        self.perforations_per_count.write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        Ok(KeyCode {
            film_manufacturer_code: i32::read(read)?,
            film_type: i32::read(read)?,
            film_roll_prefix: i32::read(read)?,
            count: i32::read(read)?,
            perforation_offset: i32::read(read)?,
            perforations_per_frame: i32::read(read)?,
            perforations_per_count: i32::read(read)?,
        })
    }
}

impl LineOrder {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        u32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(self, write: &mut W) -> UnitResult {
        use self::LineOrder::*;
        match self {
            Increasing => 0_u8,
            Decreasing => 1_u8,
            Unspecified => 2_u8,
        }.write(write)?;

        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        use self::LineOrder::*;
        Ok(match u8::read(read)? {
            0 => Increasing,
            1 => Decreasing,
            2 => Unspecified,
            _ => return Err(Error::invalid("line order attribute value")),
        })
    }
}

impl Preview {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size(&self) -> usize {
        2 * u32::BYTE_SIZE + self.pixel_data.len()
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        u32::write(self.size.0 as u32, write)?;
        u32::write(self.size.1 as u32, write)?;

        i8::write_slice(write, &self.pixel_data)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let components_per_pixel = 4;
        let width = u32::read(read)? as usize;
        let height = u32::read(read)? as usize;

        let pixel_data = i8::read_vec(read, width * height * components_per_pixel, 1024*1024*4, None)?;

        let preview = Preview {
            size: Vec2(width, height),
            pixel_data,
        };

        Ok(preview)
    }

    /// Validate this instance.
    pub fn validate(&self, strict: bool) -> UnitResult {
        if strict && (self.size.0 * self.size.1 * 4 != self.pixel_data.len()) {
            return Err(Error::invalid("preview dimensions do not match content length"))
        }

        Ok(())
    }
}

impl ::std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "Preview ({}x{} px)", self.size.0, self.size.1)
    }
}

impl TileDescription {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        2 * u32::BYTE_SIZE + 1 // size x,y + (level mode + rounding mode)
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        u32::write(self.tile_size.0 as u32, write)?;
        u32::write(self.tile_size.1 as u32, write)?;

        let level_mode = match self.level_mode {
            LevelMode::Singular => 0_u8,
            LevelMode::MipMap => 1_u8,
            LevelMode::RipMap => 2_u8,
        };

        let rounding_mode = match self.rounding_mode {
            RoundingMode::Down => 0_u8,
            RoundingMode::Up => 1_u8,
        };

        let mode: u8 = level_mode + (rounding_mode * 16);
        mode.write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let x_size = u32::read(read)? as usize;
        let y_size = u32::read(read)? as usize;

        let mode = u8::read(read)?;

        // wow you really saved that one byte here
        // mode = level_mode + (rounding_mode * 16)
        let level_mode = mode & 0b00001111; // wow that works
        let rounding_mode = mode >> 4; // wow that works

        let level_mode = match level_mode {
            0 => LevelMode::Singular,
            1 => LevelMode::MipMap,
            2 => LevelMode::RipMap,
            _ => return Err(Error::invalid("tile description level mode")),
        };

        let rounding_mode = match rounding_mode {
            0 => RoundingMode::Down,
            1 => RoundingMode::Up,
            _ => return Err(Error::invalid("tile description rounding mode")),
        };

        Ok(TileDescription { tile_size: Vec2(x_size, y_size), level_mode, rounding_mode, })
    }

    /// Validate this instance.
    pub fn validate(&self) -> UnitResult {
        let max = std::i32::MAX as i64 / 2;

        if self.tile_size.0 == 0 || self.tile_size.1 == 0 || self.tile_size.0 as i64 > max || self.tile_size.1 as i64 > max  {
            return Err(Error::invalid("tile size"))
        }

        Ok(())
    }
}

impl Attribute {

    /// Create a new attribute from name and value.
    pub fn new(name: Text, value: AnyValue) -> Self {
        Self { name, value }
    }

    /// Create a new attribute from a predefined byte slice and value.
    pub fn predefined(name: &'static [u8], value: AnyValue) -> Self {
        Self { name: Text::from_bytes_unchecked(SmallVec::from_slice(name)), value }
    }


    /// Number of bytes this would consume in an exr file.
    // TODO instead of pre calculating byte size, write to a tmp buffer whose length is inspected before actually writing?
    pub fn byte_size(&self) -> usize {
        self.name.null_terminated_byte_size()
            + self.value.kind_name().len() + sequence_end::byte_size()
            + i32::BYTE_SIZE // serialized byte size
            + self.value.byte_size()
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        self.name.write_null_terminated(write)?;
        Text::write_null_terminated_bytes(self.value.kind_name(), write)?;
        i32::write(self.value.byte_size() as i32, write)?;
        self.value.write(write)
    }

    /// Read the value without validating.
    pub fn read(read: &mut PeekRead<impl Read>, max_size: usize) -> Result<Self> {
        let name = Text::read_null_terminated(read, max_size)?;
        let kind = Text::read_null_terminated(read, max_size)?;
        let size = i32_to_usize(i32::read(read)?, "attribute size")?;
        let value = AnyValue::read(read, kind, size)?;
        Ok(Attribute { name, value, })
    }

    /// Validate this instance.
    pub fn validate(&self, long_names: bool, allow_sampling: bool, data_window: IntRect, strict: bool) -> UnitResult {
        self.name.validate(true, Some(long_names))?; // only name text has length restriction
        self.value.validate(allow_sampling, data_window, strict) // attribute value text length is never restricted
    }
}



impl AnyValue {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size(&self) -> usize {
        use self::AnyValue::*;

        match *self {
            IntRect(_) => self::IntRect::byte_size(),
            FloatRect(_) => self::FloatRect::byte_size(),

            I32(_) => i32::BYTE_SIZE,
            F32(_) => f32::BYTE_SIZE,
            F64(_) => f64::BYTE_SIZE,

            Rational(_) => { i32::BYTE_SIZE + u32::BYTE_SIZE },
            TimeCode(_) => self::TimeCode::BYTE_SIZE,

            IntVec2(_) => { 2 * i32::BYTE_SIZE },
            FloatVec2(_) => { 2 * f32::BYTE_SIZE },
            IntVec3(_) => { 3 * i32::BYTE_SIZE },
            FloatVec3(_) => { 3 * f32::BYTE_SIZE },

            ChannelList(ref channels) => channels.byte_size(),
            Chromaticities(_) => self::Chromaticities::byte_size(),
            Compression(_) => self::Compression::byte_size(),
            EnvironmentMap(_) => self::EnvironmentMap::byte_size(),

            KeyCode(_) => self::KeyCode::byte_size(),
            LineOrder(_) => self::LineOrder::byte_size(),

            F32Matrix3x3(ref value) => value.len() * f32::BYTE_SIZE,
            F32Matrix4x4(ref value) => value.len() * f32::BYTE_SIZE,

            Preview(ref value) => value.byte_size(),

            // attribute value texts never have limited size.
            // also, don't serialize size, as it can be inferred from attribute size
            Text(ref value) => value.bytes.len(),

            TextVector(ref value) => value.iter().map(self::Text::i32_sized_byte_size).sum(),
            TileDescription(_) => self::TileDescription::byte_size(),
            Custom { ref bytes, .. } => bytes.len(),
            BlockType(ref kind) => kind.byte_size()
        }
    }

    /// The exr name string of the type that an attribute can have.
    pub fn kind_name(&self) -> &[u8] {
        use self::AnyValue::*;
        use self::attribute_type_names as ty;

        match *self {
            IntRect(_) =>  ty::I32BOX2,
            FloatRect(_) =>  ty::F32BOX2,
            I32(_) =>  ty::I32,
            F32(_) =>  ty::F32,
            F64(_) =>  ty::F64,
            Rational(_) => ty::RATIONAL,
            TimeCode(_) => ty::TIME_CODE,
            IntVec2(_) => ty::I32VEC2,
            FloatVec2(_) => ty::F32VEC2,
            IntVec3(_) => ty::I32VEC3,
            FloatVec3(_) => ty::F32VEC3,
            ChannelList(_) =>  ty::CHANNEL_LIST,
            Chromaticities(_) =>  ty::CHROMATICITIES,
            Compression(_) =>  ty::COMPRESSION,
            EnvironmentMap(_) =>  ty::ENVIRONMENT_MAP,
            KeyCode(_) =>  ty::KEY_CODE,
            LineOrder(_) =>  ty::LINE_ORDER,
            F32Matrix3x3(_) =>  ty::F32MATRIX3X3,
            F32Matrix4x4(_) =>  ty::F32MATRIX4X4,
            Preview(_) =>  ty::PREVIEW,
            Text(_) =>  ty::TEXT,
            TextVector(_) =>  ty::TEXT_VECTOR,
            TileDescription(_) =>  ty::TILES,
            Custom { ref kind, .. } => &kind.bytes,
            BlockType(_) => super::BlockType::TYPE_NAME,
        }
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        use self::AnyValue::*;
        match *self {
            IntRect(value) => value.write(write)?,
            FloatRect(value) => value.write(write)?,

            I32(value) => value.write(write)?,
            F32(value) => value.write(write)?,
            F64(value) => value.write(write)?,

            Rational((a, b)) => { a.write(write)?; b.write(write)?; },
            TimeCode(codes) => { codes.write(write)?; },

            IntVec2(Vec2(x, y)) => { x.write(write)?; y.write(write)?; },
            FloatVec2(Vec2(x, y)) => { x.write(write)?; y.write(write)?; },
            IntVec3((x, y, z)) => { x.write(write)?; y.write(write)?; z.write(write)?; },
            FloatVec3((x, y, z)) => { x.write(write)?; y.write(write)?; z.write(write)?; },

            ChannelList(ref channels) => channels.write(write)?,
            Chromaticities(ref value) => value.write(write)?,
            Compression(value) => value.write(write)?,
            EnvironmentMap(value) => value.write(write)?,

            KeyCode(value) => value.write(write)?,
            LineOrder(value) => value.write(write)?,

            F32Matrix3x3(mut value) => f32::write_slice(write, &mut value)?,
            F32Matrix4x4(mut value) => f32::write_slice(write, &mut value)?,

            Preview(ref value) => { value.write(write)?; },

            // attribute value texts never have limited size.
            // also, don't serialize size, as it can be inferred from attribute size
            Text(ref value) => u8::write_slice(write, value.bytes.as_slice())?,

            TextVector(ref value) => self::Text::write_vec_of_i32_sized_texts(write, value)?,
            TileDescription(ref value) => value.write(write)?,
            Custom { ref bytes, .. } => u8::write_slice(write, &bytes)?, // write.write(&bytes).map(|_| ()),
            BlockType(kind) => kind.write(write)?
        };

        Ok(())
    }

    /// Read the value without validating.
    pub fn read(read: &mut PeekRead<impl Read>, kind: Text, byte_size: usize) -> Result<Self> {
        use self::AnyValue::*;
        use self::attribute_type_names as ty;

        Ok(match kind.bytes.as_slice() {
            ty::I32BOX2 => IntRect(self::IntRect::read(read)?),
            ty::F32BOX2 => FloatRect(self::FloatRect::read(read)?),

            ty::I32 => I32(i32::read(read)?),
            ty::F32 => F32(f32::read(read)?),
            ty::F64 => F64(f64::read(read)?),

            ty::RATIONAL => Rational({
                let a = i32::read(read)?;
                let b = u32::read(read)?;
                (a, b)
            }),

            ty::TIME_CODE => TimeCode(self::TimeCode::read(read)?),

            ty::I32VEC2 => IntVec2({
                let a = i32::read(read)?;
                let b = i32::read(read)?;
                Vec2(a, b)
            }),

            ty::F32VEC2 => FloatVec2({
                let a = f32::read(read)?;
                let b = f32::read(read)?;
                Vec2(a, b)
            }),

            ty::I32VEC3 => IntVec3({
                let a = i32::read(read)?;
                let b = i32::read(read)?;
                let c = i32::read(read)?;
                (a, b, c)
            }),

            ty::F32VEC3 => FloatVec3({
                let a = f32::read(read)?;
                let b = f32::read(read)?;
                let c = f32::read(read)?;
                (a, b, c)
            }),

            ty::CHANNEL_LIST    => ChannelList(self::ChannelList::read(read)?),
            ty::CHROMATICITIES  => Chromaticities(self::Chromaticities::read(read)?),
            ty::COMPRESSION     => Compression(self::Compression::read(read)?),
            ty::ENVIRONMENT_MAP => EnvironmentMap(self::EnvironmentMap::read(read)?),

            ty::KEY_CODE   => KeyCode(self::KeyCode::read(read)?),
            ty::LINE_ORDER => LineOrder(self::LineOrder::read(read)?),

            ty::F32MATRIX3X3 => F32Matrix3x3({
                let mut result = [0.0_f32; 9];
                f32::read_slice(read, &mut result)?;
                result
            }),

            ty::F32MATRIX4X4 => F32Matrix4x4({
                let mut result = [0.0_f32; 16];
                f32::read_slice(read, &mut result)?;
                result
            }),

            ty::PREVIEW     => Preview(self::Preview::read(read)?),
            ty::TEXT        => Text(self::Text::read_sized(read, byte_size)?),

            // the number of strings can be inferred from the total attribute size
            ty::TEXT_VECTOR => TextVector(self::Text::read_vec_of_i32_sized(read, byte_size)?),

            ty::TILES       => TileDescription(self::TileDescription::read(read)?),

            _ => {
                let mut bytes = vec![0_u8; byte_size];
                u8::read_slice(read, &mut bytes)?;
                Custom { kind, bytes }
            }
        })
    }

    /// Validate this instance.
    pub fn validate(&self, allow_sampling: bool, data_window: IntRect, strict: bool) -> UnitResult {
        use self::AnyValue::*;

        match *self {
            ChannelList(ref channels) => channels.validate(allow_sampling, data_window, strict)?,
            TileDescription(ref value) => value.validate()?,
            Preview(ref value) => value.validate(strict)?,

            TextVector(ref vec) => if strict && vec.is_empty() {
                return Err(Error::invalid("text vector may not be empty"))
            },

            _ => {}
        };

        Ok(())
    }

    /// Return `Ok(TileDescription)` if this attribute is a tile description.
    pub fn to_tile_description(&self) -> Result<TileDescription> {
        match *self {
            AnyValue::TileDescription(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(TimeCode)` if this attribute is a time code.
    pub fn to_time_code(&self) -> Result<TimeCode> {
        match *self {
            AnyValue::TimeCode(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(Preview)` if this attribute is an image preview.
    pub fn to_preview(&self) -> Result<&Preview> {
        match self {
            AnyValue::Preview(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(i32)` if this attribute is an i32.
    pub fn to_i32(&self) -> Result<i32> {
        match *self {
            AnyValue::I32(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(f32)` if this attribute is an f32.
    pub fn to_f32(&self) -> Result<f32> {
        match *self {
            AnyValue::F32(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(IntRect)` if this attribute is a integer rectangle.
    pub fn to_i32_box_2(&self) -> Result<IntRect> {
        match *self {
            AnyValue::IntRect(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(f32)` if this attribute is a 2d vector of f32 numbers.
    pub fn to_f32_vec_2(&self) -> Result<Vec2<f32>> {
        match *self {
            AnyValue::FloatVec2(vec) => Ok(vec),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(LineOrder)` if this attribute is a line order.
    pub fn to_line_order(&self) -> Result<LineOrder> {
        match *self {
            AnyValue::LineOrder(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(Compression)` if this attribute is a compression.
    pub fn to_compression(&self) -> Result<Compression> {
        match *self {
            AnyValue::Compression(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(Text)` if this attribute is a text.
    pub fn into_text(self) -> Result<Text> {
        match self {
            AnyValue::Text(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(BlockType)` if this attribute is a block type.
    pub fn into_block_type(self) -> Result<BlockType> {
        match self {
            AnyValue::BlockType(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(ChannelList)` if this attribute is a channel list.
    pub fn into_channel_list(self) -> Result<ChannelList> {
        match self {
            AnyValue::ChannelList(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(Chromaticities)` if this attribute is a chromaticities attribute.
    pub fn to_chromaticities(&self) -> Result<Chromaticities> {
        match *self {
            AnyValue::Chromaticities(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }
}

/// Contains string literals identifying the type of an attribute.
pub mod attribute_type_names {
    macro_rules! define_attribute_type_names {
        ( $($name: ident : $value: expr),* ) => {
            $(
                /// The byte-string name of this attribute type as it appears in an exr file.
                pub const $name: &'static [u8] = $value;
            )*
        };
    }

    define_attribute_type_names! {
        I32BOX2:        b"box2i",
        F32BOX2:        b"box2f",
        I32:            b"int",
        F32:            b"float",
        F64:            b"double",
        RATIONAL:       b"rational",
        TIME_CODE:      b"timecode",
        I32VEC2:        b"v2i",
        F32VEC2:        b"v2f",
        I32VEC3:        b"v3i",
        F32VEC3:        b"v3f",
        CHANNEL_LIST:   b"chlist",
        CHROMATICITIES: b"chromaticities",
        COMPRESSION:    b"compression",
        ENVIRONMENT_MAP:b"envmap",
        KEY_CODE:       b"keycode",
        LINE_ORDER:     b"lineOrder",
        F32MATRIX3X3:   b"m33f",
        F32MATRIX4X4:   b"m44f",
        PREVIEW:        b"preview",
        TEXT:           b"string",
        TEXT_VECTOR:    b"stringvector",
        TILES:          b"tiledesc"
    }
}

/// Collection of required attribute names
pub mod required_attribute_names {
    macro_rules! define_required_attribute_names {
        ( $($name: ident : $value: expr),* ) => {
            $(
                /// The byte-string name of this required attribute as it appears in an exr file.
                pub const $name: &'static [u8] = $value;
            )*
        };
    }

    define_required_attribute_names! {
        TILES: b"tiles",
        NAME: b"name",
        BLOCK_TYPE: b"type",
        DEEP_DATA_VERSION: b"version",
        CHUNKS: b"chunkCount",
        MAX_SAMPLES: b"maxSamplesPerPixel",
        CHANNELS: b"channels",
        COMPRESSION: b"compression",
        DATA_WINDOW: b"dataWindow",
        DISPLAY_WINDOW: b"displayWindow",
        LINE_ORDER: b"lineOrder",
        PIXEL_ASPECT: b"pixelAspectRatio",
        WINDOW_CENTER: b"screenWindowCenter",
        WINDOW_WIDTH: b"screenWindowWidth"
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use ::std::io::Cursor;

    #[test]
    fn text_ord() {
        for _ in 0..1024 {
            let text1 = Text::from_bytes_unchecked((0..4).map(|_| rand::random::<u8>()).collect());
            let text2 = Text::from_bytes_unchecked((0..4).map(|_| rand::random::<u8>()).collect());

            assert_eq!(text1.to_string().cmp(&text2.to_string()), text1.cmp(&text2), "in text {:?} vs {:?}", text1, text2);
        }
    }

    #[test]
    fn rounding_up(){
        let round_up = RoundingMode::Up;
        assert_eq!(round_up.divide(10, 10), 1, "divide equal");
        assert_eq!(round_up.divide(10, 2), 5, "divide even");
        assert_eq!(round_up.divide(10, 5), 2, "divide even");

        assert_eq!(round_up.divide(8, 5), 2, "round up");
        assert_eq!(round_up.divide(10, 3), 4, "round up");
        assert_eq!(round_up.divide(100, 50), 2, "divide even");
        assert_eq!(round_up.divide(100, 49), 3, "round up");
    }

    #[test]
    fn rounding_down(){
        let round_down = RoundingMode::Down;
        assert_eq!(round_down.divide(8, 5), 1, "round down");
        assert_eq!(round_down.divide(10, 3), 3, "round down");
        assert_eq!(round_down.divide(100, 50), 2, "divide even");
        assert_eq!(round_down.divide(100, 49), 2, "round down");
        assert_eq!(round_down.divide(100, 51), 1, "round down");
    }

    #[test]
    fn tile_description_write_read_roundtrip(){
        let tiles = [
            TileDescription {
                tile_size: Vec2(31, 7),
                level_mode: LevelMode::MipMap,
                rounding_mode: RoundingMode::Down,
            },

            TileDescription {
                tile_size: Vec2(0, 0),
                level_mode: LevelMode::Singular,
                rounding_mode: RoundingMode::Up,
            },

            TileDescription {
                tile_size: Vec2(4294967294, 4294967295),
                level_mode: LevelMode::RipMap,
                rounding_mode: RoundingMode::Down,
            },
        ];

        for tile in &tiles {
            let mut bytes = Vec::new();
            tile.write(&mut bytes).unwrap();

            let new_tile = TileDescription::read(&mut Cursor::new(bytes)).unwrap();
            assert_eq!(*tile, new_tile, "tile round trip");
        }
    }

    #[test]
    fn attribute_write_read_roundtrip_and_byte_size(){
        let attributes = [
            Attribute {
                name: Text::from("greeting").unwrap(),
                value: AnyValue::Text(Text::from("hello").unwrap()),
            },
            Attribute {
                name: Text::from("age").unwrap(),
                value: AnyValue::I32(923),
            },
            Attribute {
                name: Text::from("leg count").unwrap(),
                value: AnyValue::F64(9.114939599234),
            },
            Attribute {
                name: Text::from("rabbit area").unwrap(),
                value: AnyValue::FloatRect(FloatRect {
                    min: Vec2(23.4234, 345.23),
                    max: Vec2(68623.0, 3.12425926538),
                }),
            },
            Attribute {
                name: Text::from("tests are difficult").unwrap(),
                value: AnyValue::TextVector(vec![
                    Text::from("sdoifjpsdv").unwrap(),
                    Text::from("sdoifjpsdvxxxx").unwrap(),
                    Text::from("sdoifjasd").unwrap(),
                    Text::from("sdoifj").unwrap(),
                    Text::from("sdoifjddddddddasdasd").unwrap(),
                ]),
            },
            Attribute {
                name: Text::from("what should we eat tonight").unwrap(),
                value: AnyValue::Preview(Preview {
                    size: Vec2(10, 30),
                    pixel_data: vec![31; 10 * 30 * 4],
                }),
            },
            Attribute {
                name: Text::from("leg count, again").unwrap(),
                value: AnyValue::ChannelList(ChannelList {
                    list: smallvec![
                        Channel {
                            name: Text::from("Green").unwrap(),
                            pixel_type: PixelType::F16,
                            is_linear: false,
                            sampling: Vec2(1,2)
                        },
                        Channel {
                            name: Text::from("Red").unwrap(),
                            pixel_type: PixelType::F32,
                            is_linear: true,
                            sampling: Vec2(1,2)
                        },
                        Channel {
                            name: Text::from("Purple").unwrap(),
                            pixel_type: PixelType::U32,
                            is_linear: false,
                            sampling: Vec2(0,0)
                        }
                    ],
                    bytes_per_pixel: 10
                }),
            },
        ];

        for attribute in &attributes {
            let mut bytes = Vec::new();
            attribute.write(&mut bytes).unwrap();
            assert_eq!(attribute.byte_size(), bytes.len(), "attribute.byte_size() for {:?}", attribute);

            let new_attribute = Attribute::read(&mut PeekRead::new(Cursor::new(bytes)), 300).unwrap();
            assert_eq!(*attribute, new_attribute, "attribute round trip");
        }


        {
            let too_large_named = Attribute {
                name: Text::from("asdkaspfokpaosdkfpaokswdpoakpsfokaposdkf").unwrap(),
                value: AnyValue::I32(0),
            };

            too_large_named.validate(false, false, IntRect::zero(), false).expect_err("name length check failed");
        }

        {
            let way_too_large_named = Attribute {
                name: Text::from("sdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfpo").unwrap(),
                value: AnyValue::I32(0),
            };

            way_too_large_named.validate(true, false, IntRect::zero(), false).expect_err("name length check failed");
        }
    }
}