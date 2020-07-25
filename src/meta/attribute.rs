
//! Contains all meta data attributes.
//! Each layer can have any number of [`Attribute`]s, including custom attributes.

use smallvec::SmallVec;


/// Contains one of all possible attributes.
/// Includes a variant for custom attributes.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {

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
    Matrix3x3(Matrix3x3),

    /// A 4x4 matrix of floats.
    Matrix4x4(Matrix4x4),

    /// 8-bit RGBA Preview of the image.
    Preview(Preview),

    /// An integer dividend and divisor.
    Rational(Rational),

    /// Deep or flat and tiled or scan line.
    BlockType(BlockType),

    /// List of texts.
    TextVector(Vec<Text>),

    /// How to tile up the image.
    TileDescription(TileDescription),

    /// Timepoint and more.
    TimeCode(TimeCode),

    /// A string of byte-chars.
    Text(Text),

    /// 64-bit float
    F64(f64),

    /// 32-bit float
    F32(f32),

    /// 32-bit signed integer
    I32(i32),

    /// 2D integer rectangle.
    IntegerBounds(IntegerBounds),

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
    },
}

/// A byte array with each byte being a char.
/// This is not UTF an must be constructed from a standard string.
// TODO is this ascii? use a rust ascii crate?
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Default)]
pub struct Text {
    bytes: TextBytes,
}

/// Contains time information.
// TODO use actual fields instead of bit fields and asseble bit-u32 on write
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
pub type DataWindow = IntegerBounds;

/// The integer rectangle limiting which part of the infinite 2D global space should be displayed.
pub type DisplayWindow = IntegerBounds;

/// An integer dividend and divisor, together forming a ratio.
pub type Rational = (i32, u32);

/// A float matrix with four rows and four columns.
pub type Matrix4x4 = [f32; 4*4];

/// A float matrix with three rows and three columns.
pub type Matrix3x3 = [f32; 3*3];

/// A rectangular section anywhere in 2D integer space.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct IntegerBounds {

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
    pub list: SmallVec<[ChannelInfo; 5]>,

    /// The number of bytes that one pixel in this image needs.
    // FIXME this needs to account for subsampling anywhere?
    pub bytes_per_pixel: usize, // FIXME only makes sense for flat images!

    /// The sample type of all channels, if all channels have the same type.
    pub uniform_sample_type: Option<SampleType>,
}

/// A single channel in an layer.
/// Does not contain the actual pixel data,
/// but instead merely describes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelInfo {

    /// One of "R", "G", or "B" most of the time.
    pub name: Text,

    /// U32, F16 or F32.
    pub sample_type: SampleType,

    /// This attribute only tells lossy compression methods
    /// whether this value should be quantized exponentially or linearly.
    ///
    /// Should be `false` for red, green, or blue channels.
    /// Should be `true` for hue, chroma, saturation, or alpha channels.
    pub quantize_linearly: bool,

    /// How many of the samples are skipped compared to the other channels in this layer.
    ///
    /// Can be used for chroma subsampling for manual lossy data compression.
    /// Values other than 1 are allowed only in flat, scan-line based images.
    /// If an image is deep or tiled, x and y sampling rates for all of its channels must be 1.
    pub sampling: Vec2<usize>,
}

/// What kind of pixels are in this channel.
#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub enum SampleType {

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
    /// In multi-core file writing, this option offers the best performance.
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

    /// Create a `Text` from a slice of bytes,
    /// without checking any of the bytes.
    pub fn from_slice_unchecked(text: &'static [u8]) -> Self {
        Self::from_bytes_unchecked(SmallVec::from_slice(text))
    }

    /// Create a `Text` from the specified bytes object,
    /// without checking any of the bytes.
    pub fn from_bytes_unchecked(bytes: TextBytes) -> Self {
        Text { bytes }
    }

    /// Check whether this string is valid, adjusting `long_names` if required.
    /// If `long_names` is not provided, text length will be entirely unchecked.
    pub fn validate(&self, null_terminated: bool, long_names: Option<&mut bool>) -> UnitResult {
        Self::validate_bytes(self.bytes(), null_terminated, long_names)
    }

    /// Check whether some bytes are valid, adjusting `long_names` if required.
    /// If `long_names` is not provided, text length will be entirely unchecked.
    pub fn validate_bytes(text: &[u8], null_terminated: bool, long_names: Option<&mut bool>) -> UnitResult {
        if null_terminated && text.is_empty() {
            return Err(Error::invalid("text must not be empty"));
        }

        if let Some(long) = long_names {
            if text.len() >= 256 { return Err(Error::invalid("text must not be longer than 255")); }
            if text.len() >= 32 { *long = true; }
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
        let mut bytes = smallvec![ u8::read(read)? ]; // null-terminated strings are always at least 1 byte

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

    /// Iterate over the individual chars in this text, similar to `String::chars()`.
    /// Does not do any heap-allocation but borrows from this instance instead.
    pub fn chars(&self) -> impl '_ + Iterator<Item = char> {
        self.bytes.iter().map(|&byte| byte as char)
    }

    /// Compare this `exr::Text` with a plain `&str`.
    pub fn eq(&self, string: &str) -> bool {
        string.chars().eq(self.chars())
    }

    /// Compare this `exr::Text` with a plain `&str` ignoring capitalization.
    pub fn eq_case_insensitive(&self, string: &str) -> bool {
        // this is technically not working for a "turkish i", but those cannot be encoded in exr files anyways
        let self_chars = self.chars().map(|char| char.to_ascii_lowercase());
        let string_chars = string.chars().flat_map(|ch| ch.to_lowercase());

        string_chars.eq(self_chars)
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
    pub fn new(channels: SmallVec<[ChannelInfo; 5]>) -> Self {
        let uniform_sample_type = {
            if let Some(first) = channels.first() {
                let has_uniform_types = channels.iter().skip(1)
                    .all(|chan| chan.sample_type == first.sample_type);

                if has_uniform_types { Some(first.sample_type) } else { None }
            }
            else { None }
        };

        ChannelList {
            bytes_per_pixel: channels.iter().map(|channel| channel.sample_type.bytes_per_sample()).sum(),
            list: channels, uniform_sample_type,
        }
    }
}

impl BlockType {

    /// The corresponding attribute type name literal
    const TYPE_NAME: &'static [u8] = type_names::TEXT;

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


impl IntegerBounds {

    /// Create a box with no size located at (0,0).
    pub fn zero() -> Self {
        Self::from_dimensions(Vec2(0, 0))
    }

    /// Create a box with a size starting at zero.
    pub fn from_dimensions(size: impl Into<Vec2<usize>>) -> Self {
        Self::new(Vec2(0,0), size)
    }

    /// Create a box with a size and an origin point.
    pub fn new(start: impl Into<Vec2<i32>>, size: impl Into<Vec2<usize>>) -> Self {
        Self { position: start.into(), size: size.into() }
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
    pub fn validate(&self, max: Option<Vec2<usize>>) -> UnitResult {
        if let Some(max) = max {
            if self.size.width() > max.width() || self.size.height() > max.height()  {
                return Err(Error::invalid("window attribute dimension value"));
            }
        }

        let max_int = i32::MAX as i64 / 2; // cannot go bigger than that ever

        let self_max = Vec2(
            self.position.x() as i64 + self.size.width() as i64,
            self.position.y() as i64 + self.size.height() as i64,
        );

        if self_max.x() >= max_int || self_max.y() >= max_int
            || self.position.x() as i64 <= -max_int
            || self.position.y() as i64 <= -max_int
        {
            return Err(Error::invalid("window size exceeding integer maximum"));
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
        let size = Vec2(max.x() + 1 - min.x(), max.y() + 1 - min.y()); // which is why we add 1
        let size = size.to_usize("box coordinates")?;

        Ok(IntegerBounds { position: min, size })
    }

    /// Create a new rectangle which is offset by the specified origin.
    pub fn with_origin(self, origin: Vec2<i32>) -> Self { // TODO rename to "move" or "translate"?
        IntegerBounds { position: self.position + origin, .. self }
    }

    /// Returns whether the specified rectangle is equal to or inside this rectangle.
    pub fn contains(self, subset: Self) -> bool {
           subset.position.x() >= self.position.x()
        && subset.position.y() >= self.position.y()
        && subset.end().x() <= self.end().x()
        && subset.end().y() <= self.end().y()
    }
}


impl FloatRect {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        4 * f32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        self.min.x().write(write)?;
        self.min.y().write(write)?;
        self.max.x().write(write)?;
        self.max.y().write(write)?;
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

impl SampleType {

    /// How many bytes a single sample takes up.
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            SampleType::F16 => f16::BYTE_SIZE,
            SampleType::F32 => f32::BYTE_SIZE,
            SampleType::U32 => u32::BYTE_SIZE,
        }
    }

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        i32::BYTE_SIZE
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        match *self {
            SampleType::U32 => 0_i32,
            SampleType::F16 => 1_i32,
            SampleType::F32 => 2_i32,
        }.write(write)?;

        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        // there's definitely going to be more than 255 different pixel types in the future
        Ok(match i32::read(read)? {
            0 => SampleType::U32,
            1 => SampleType::F16,
            2 => SampleType::F32,
            _ => return Err(Error::invalid("pixel type attribute value")),
        })
    }
}

impl ChannelInfo {

    /// Create a new channel with the specified properties and a sampling rate of (1,1).
    pub fn new(name: Text, sample_type: SampleType, quantize_linearly: bool) -> Self {
        Self { name, sample_type, quantize_linearly, sampling: Vec2(1, 1) }
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
            + SampleType::byte_size()
            + 1 // is_linear
            + 3 // reserved bytes
            + 2 * u32::BYTE_SIZE // sampling x, y
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        Text::write_null_terminated(&self.name, write)?;
        self.sample_type.write(write)?;

        match self.quantize_linearly {
            false => 0_u8,
            true  => 1_u8,
        }.write(write)?;

        i8::write_slice(write, &[0_i8, 0_i8, 0_i8])?;
        i32::write(usize_to_i32(self.sampling.x()), write)?;
        i32::write(usize_to_i32(self.sampling.y()), write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let name = Text::read_null_terminated(read, 256)?;
        let sample_type = SampleType::read(read)?;

        let is_linear = match u8::read(read)? {
            1 => true,
            0 => false,
            _ => return Err(Error::invalid("channel linearity attribute value")),
        };

        let mut reserved = [0_i8; 3];
        i8::read_slice(read, &mut reserved)?;

        let x_sampling = i32_to_usize(i32::read(read)?, "x channel sampling")?;
        let y_sampling = i32_to_usize(i32::read(read)?, "y channel sampling")?;

        Ok(ChannelInfo {
            name, sample_type,
            quantize_linearly: is_linear,
            sampling: Vec2(x_sampling, y_sampling),
        })
    }

    /// Validate this instance.
    pub fn validate(&self, allow_sampling: bool, data_window: IntegerBounds, strict: bool) -> UnitResult {
        self.name.validate(true, None)?; // TODO spec says this does not affect `requirements.long_names` but is that true?

        if self.sampling.x() == 0 || self.sampling.y() == 0 {
            return Err(Error::invalid("zero sampling factor"));
        }

        if strict && allow_sampling && self.sampling != Vec2(1,1) {
            return Err(Error::invalid("subsampling is only allowed in flat scan line images"));
        }

        if data_window.position.x() % self.sampling.x() as i32 != 0 || data_window.position.y() % self.sampling.y() as i32 != 0 {
            return Err(Error::invalid("channel sampling factor not dividing data window position"));
        }

        if data_window.size.x() % self.sampling.x() != 0 || data_window.size.y() % self.sampling.y() != 0 {
            return Err(Error::invalid("channel sampling factor not dividing data window size"));
        }

        if self.sampling != Vec2(1,1) {
            // TODO this must only be implemented in the crate::image module and child modules,
            //      should not be too difficult

            return Err(Error::unsupported("channel subsampling not supported yet"));
        }

        Ok(())
    }
}

impl ChannelList {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size(&self) -> usize {
        self.list.iter().map(ChannelInfo::byte_size).sum::<usize>() + sequence_end::byte_size()
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
            channels.push(ChannelInfo::read(read)?);
        }

        Ok(ChannelList::new(channels))
    }

    /// Check if channels are valid and sorted.
    pub fn validate(&self, allow_sampling: bool, data_window: IntegerBounds, strict: bool) -> UnitResult {
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
        self.red.x().write(write)?;
        self.red.y().write(write)?;

        self.green.x().write(write)?;
        self.green.y().write(write)?;

        self.blue.x().write(write)?;
        self.blue.y().write(write)?;

        self.white.x().write(write)?;
        self.white.y().write(write)?;
        Ok(())
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        Ok(Chromaticities {
            red: Vec2(f32::read(read)?, f32::read(read)?),
            green: Vec2(f32::read(read)?, f32::read(read)?),
            blue: Vec2(f32::read(read)?, f32::read(read)?),
            white: Vec2(f32::read(read)?, f32::read(read)?),
        })
    }
}

impl Compression {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize { u8::BYTE_SIZE }

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
            DWAA(_) => 8_u8,
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
            8 => DWAA(None),
            9 => DWAB,
            _ => return Err(Error::unsupported("unknown compression method")),
        })
    }
}

impl EnvironmentMap {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        u8::BYTE_SIZE
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
        u8::BYTE_SIZE
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
        u32::write(self.size.width() as u32, write)?;
        u32::write(self.size.height() as u32, write)?;

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
        if strict && (self.size.area() * 4 != self.pixel_data.len()) {
            return Err(Error::invalid("preview dimensions do not match content length"))
        }

        Ok(())
    }
}

impl ::std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(f, "Preview ({}x{} px)", self.size.width(), self.size.height())
    }
}

impl TileDescription {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        2 * u32::BYTE_SIZE + 1 // size x,y + (level mode + rounding mode)
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(&self, write: &mut W) -> UnitResult {
        u32::write(self.tile_size.width() as u32, write)?;
        u32::write(self.tile_size.height() as u32, write)?;

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
        let max = i32::MAX as i64 / 2;

        if self.tile_size.width() == 0 || self.tile_size.height() == 0
            || self.tile_size.width() as i64 >= max || self.tile_size.height() as i64 >= max
        {
            return Err(Error::invalid("tile size"))
        }

        Ok(())
    }
}


/// Number of bytes this attribute would consume in an exr file.
// TODO instead of pre calculating byte size, write to a tmp buffer whose length is inspected before actually writing?
pub fn byte_size(name: &Text, value: &AttributeValue) -> usize {
    name.null_terminated_byte_size()
        + value.kind_name().len() + sequence_end::byte_size()
        + i32::BYTE_SIZE // serialized byte size
        + value.byte_size()
}

/// Without validation, write this attribute to the byte stream.
pub fn write<W: Write>(name: &[u8], value: &AttributeValue, write: &mut W) -> UnitResult {
    Text::write_null_terminated_bytes(name, write)?;
    Text::write_null_terminated_bytes(value.kind_name(), write)?;
    i32::write(value.byte_size() as i32, write)?;
    value.write(write)
}

/// Read the attribute without validating. The result may be `Ok` even if this single attribute is invalid.
pub fn read(read: &mut PeekRead<impl Read>, max_size: usize) -> Result<(Text, Result<AttributeValue>)> {
    let name = Text::read_null_terminated(read, max_size)?;
    let kind = Text::read_null_terminated(read, max_size)?;
    let size = i32_to_usize(i32::read(read)?, "attribute size")?;
    let value = AttributeValue::read(read, kind, size)?;
    Ok((name, value))
}

/// Validate this attribute.
pub fn validate(name: &Text, value: &AttributeValue, long_names: &mut bool, allow_sampling: bool, data_window: IntegerBounds, strict: bool) -> UnitResult {
    name.validate(true, Some(long_names))?; // only name text has length restriction
    value.validate(allow_sampling, data_window, strict) // attribute value text length is never restricted
}


impl AttributeValue {

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size(&self) -> usize {
        use self::AttributeValue::*;

        match *self {
            IntegerBounds(_) => self::IntegerBounds::byte_size(),
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

            Matrix3x3(ref value) => value.len() * f32::BYTE_SIZE,
            Matrix4x4(ref value) => value.len() * f32::BYTE_SIZE,

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
        use self::AttributeValue::*;
        use self::type_names as ty;

        match *self {
            IntegerBounds(_) =>  ty::I32BOX2,
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
            Matrix3x3(_) =>  ty::F32MATRIX3X3,
            Matrix4x4(_) =>  ty::F32MATRIX4X4,
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
        use self::AttributeValue::*;
        match *self {
            IntegerBounds(value) => value.write(write)?,
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

            Matrix3x3(mut value) => f32::write_slice(write, &mut value)?,
            Matrix4x4(mut value) => f32::write_slice(write, &mut value)?,

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
    /// Returns `Ok(Ok(attribute))` for valid attributes.
    /// Returns `Ok(Err(Error))` for invalid attributes from a valid byte source.
    /// Returns `Err(Error)` for invalid byte sources, for example for invalid files.
    pub fn read(read: &mut PeekRead<impl Read>, kind: Text, byte_size: usize) -> Result<Result<Self>> {
        use self::AttributeValue::*;
        use self::type_names as ty;

        // always read bytes
        let attribute_bytes = u8::read_vec(read, byte_size, 128, None)?;
        // TODO no allocation for small attributes // : SmallVec<[u8; 64]> = smallvec![0; byte_size];

        let parse_attribute = move || {
            let reader = &mut attribute_bytes.as_slice();

            Ok(match kind.bytes.as_slice() {
                ty::I32BOX2 => IntegerBounds(self::IntegerBounds::read(reader)?),
                ty::F32BOX2 => FloatRect(self::FloatRect::read(reader)?),

                ty::I32 => I32(i32::read(reader)?),
                ty::F32 => F32(f32::read(reader)?),
                ty::F64 => F64(f64::read(reader)?),

                ty::RATIONAL => Rational({
                    let a = i32::read(reader)?;
                    let b = u32::read(reader)?;
                    (a, b)
                }),

                ty::TIME_CODE => TimeCode(self::TimeCode::read(reader)?),

                ty::I32VEC2 => IntVec2({
                    let a = i32::read(reader)?;
                    let b = i32::read(reader)?;
                    Vec2(a, b)
                }),

                ty::F32VEC2 => FloatVec2({
                    let a = f32::read(reader)?;
                    let b = f32::read(reader)?;
                    Vec2(a, b)
                }),

                ty::I32VEC3 => IntVec3({
                    let a = i32::read(reader)?;
                    let b = i32::read(reader)?;
                    let c = i32::read(reader)?;
                    (a, b, c)
                }),

                ty::F32VEC3 => FloatVec3({
                    let a = f32::read(reader)?;
                    let b = f32::read(reader)?;
                    let c = f32::read(reader)?;
                    (a, b, c)
                }),

                ty::CHANNEL_LIST    => ChannelList(self::ChannelList::read(&mut PeekRead::new(attribute_bytes.as_slice()))?),
                ty::CHROMATICITIES  => Chromaticities(self::Chromaticities::read(reader)?),
                ty::COMPRESSION     => Compression(self::Compression::read(reader)?),
                ty::ENVIRONMENT_MAP => EnvironmentMap(self::EnvironmentMap::read(reader)?),

                ty::KEY_CODE   => KeyCode(self::KeyCode::read(reader)?),
                ty::LINE_ORDER => LineOrder(self::LineOrder::read(reader)?),

                ty::F32MATRIX3X3 => Matrix3x3({
                    let mut result = [0.0_f32; 9];
                    f32::read_slice(reader, &mut result)?;
                    result
                }),

                ty::F32MATRIX4X4 => Matrix4x4({
                    let mut result = [0.0_f32; 16];
                    f32::read_slice(reader, &mut result)?;
                    result
                }),

                ty::PREVIEW     => Preview(self::Preview::read(reader)?),
                ty::TEXT        => Text(self::Text::read_sized(reader, byte_size)?),

                // the number of strings can be inferred from the total attribute size
                ty::TEXT_VECTOR => TextVector(self::Text::read_vec_of_i32_sized(
                    &mut PeekRead::new(attribute_bytes.as_slice()),
                    byte_size
                )?),

                ty::TILES       => TileDescription(self::TileDescription::read(reader)?),

                _ => Custom { kind: kind.clone(), bytes: attribute_bytes.clone() } // TODO no clone
            })
        };

        Ok(parse_attribute())
    }

    /// Validate this instance.
    pub fn validate(&self, allow_sampling: bool, data_window: IntegerBounds, strict: bool) -> UnitResult {
        use self::AttributeValue::*;

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


    /// Return `Ok(i32)` if this attribute is an i32.
    pub fn to_i32(&self) -> Result<i32> {
        match *self {
            AttributeValue::I32(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(f32)` if this attribute is an f32.
    pub fn to_f32(&self) -> Result<f32> {
        match *self {
            AttributeValue::F32(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(Text)` if this attribute is a text.
    pub fn into_text(self) -> Result<Text> {
        match self {
            AttributeValue::Text(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(Text)` if this attribute is a text.
    pub fn to_text(&self) -> Result<&Text> {
        match self {
            AttributeValue::Text(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(Chromaticities)` if this attribute is a chromaticities attribute.
    pub fn to_chromaticities(&self) -> Result<Chromaticities> {
        match *self {
            AttributeValue::Chromaticities(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    /// Return `Ok(TimeCode)` if this attribute is a time code.
    pub fn to_time_code(&self) -> Result<TimeCode> {
        match *self {
            AttributeValue::TimeCode(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }
}



/// Contains string literals identifying the type of an attribute.
pub mod type_names {
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
            (
                Text::from("greeting").unwrap(),
                AttributeValue::Text(Text::from("hello").unwrap()),
            ),
            (
                Text::from("age").unwrap(),
                AttributeValue::I32(923),
            ),
            (
                Text::from("leg count").unwrap(),
                AttributeValue::F64(9.114939599234),
            ),
            (
                Text::from("rabbit area").unwrap(),
                AttributeValue::FloatRect(FloatRect {
                    min: Vec2(23.4234, 345.23),
                    max: Vec2(68623.0, 3.12425926538),
                }),
            ),
            (
                Text::from("tests are difficult").unwrap(),
                AttributeValue::TextVector(vec![
                    Text::from("sdoifjpsdv").unwrap(),
                    Text::from("sdoifjpsdvxxxx").unwrap(),
                    Text::from("sdoifjasd").unwrap(),
                    Text::from("sdoifj").unwrap(),
                    Text::from("sdoifjddddddddasdasd").unwrap(),
                ]),
            ),
            (
                Text::from("what should we eat tonight").unwrap(),
                AttributeValue::Preview(Preview {
                    size: Vec2(10, 30),
                    pixel_data: vec![31; 10 * 30 * 4],
                }),
            ),
            (
                Text::from("leg count, again").unwrap(),
                AttributeValue::ChannelList(ChannelList::new(smallvec![
                        ChannelInfo {
                            name: Text::from("Green").unwrap(),
                            sample_type: SampleType::F16,
                            quantize_linearly: false,
                            sampling: Vec2(1,2)
                        },
                        ChannelInfo {
                            name: Text::from("Red").unwrap(),
                            sample_type: SampleType::F32,
                            quantize_linearly: true,
                            sampling: Vec2(1,2)
                        },
                        ChannelInfo {
                            name: Text::from("Purple").unwrap(),
                            sample_type: SampleType::U32,
                            quantize_linearly: false,
                            sampling: Vec2(0,0)
                        }
                    ],
                )),
            ),
        ];

        for (name, value) in &attributes {
            let mut bytes = Vec::new();
            super::write(name.bytes(), value, &mut bytes).unwrap();
            assert_eq!(super::byte_size(name, value), bytes.len(), "attribute.byte_size() for {:?}", (name, value));

            let new_attribute = super::read(&mut PeekRead::new(Cursor::new(bytes)), 300).unwrap();
            assert_eq!((name.clone(), value.clone()), (new_attribute.0, new_attribute.1.unwrap()), "attribute round trip");
        }


        {
            let (name, value) = (
                Text::from("asdkaspfokpaosdkfpaokswdpoakpsfokaposdkf").unwrap(),
                AttributeValue::I32(0),
            );

            let mut long_names = false;
            super::validate(&name, &value, &mut long_names, false, IntegerBounds::zero(), false).unwrap();
            assert!(long_names);
        }

        {
            let (name, value) = (
                Text::from("sdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfposdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfpo").unwrap(),
                AttributeValue::I32(0),
            );

            super::validate(&name, &value, &mut false, false, IntegerBounds::zero(), false).expect_err("name length check failed");
        }
    }
}