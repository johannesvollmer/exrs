use smallvec::SmallVec;
use crate::error::validity::*;

/// null-terminated text strings.
/// max 31 bytes long (if bit 10 is set to 0),
/// or max 255 bytes long (if bit 10 is set to 1).
/// must be at least 1 byte (to avoid confusion with null-terminators)
// TODO non public fields?
#[derive(Clone, Eq, PartialEq)]
pub struct Text {
    /// vector does not include null terminator
    pub bytes: SmallVec<[u8; 16]>,
}



#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: Text,

    /// kind can be inferred from value
    /// size in bytes can be inferred from value
    pub value: AttributeValue,
}


// TODO custom attribute
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    I32Box2(I32Box2),
    F32Box2(F32Box2),
    ChannelList(ChannelList),
    Chromaticities(Chromaticities),
    Compression(Compression),
    F64(f64),
    EnvironmentMap(EnvironmentMap),
    F32(f32),
    I32(i32),
    KeyCode(KeyCode),
    LineOrder(LineOrder),
    F32Matrix3x3([f32; 9]),
    F32Matrix4x4([f32; 16]),
    Preview(Preview),
    Rational(i32, u32),

    /// i32 of byte-length followed by u8 content
    Text(Text),

    /// image kind, one of the strings specified in `Kind`
    Kind(Kind),

    /// the number of strings can be inferred from the total attribute size
    TextVector(Vec<Text>),

    TileDescription(TileDescription),

    // TODO enable conversion to rust time
    TimeCode(u32, u32),

    I32Vec2(i32, i32),
    F32Vec2(f32, f32),
    I32Vec3(i32, i32, i32),
    F32Vec3(f32, f32, f32),

    Custom { kind: Text, bytes: Vec<u8> }
}

// FIXME this should be a simple Kind enum and use Text everywhere else!

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Kind {
    /// "scanlineimage"
    ScanLine,

    /// "tiledimage"
    Tile,

    /// "deepscanline"
    DeepScanLine,

    /// "deeptile"
    DeepTile,
}

pub mod kind {
    pub const SCAN_LINE: &'static [u8] = b"scanlineimage";
    pub const TILE: &'static [u8] = b"tiledimage";

    pub const DEEP_SCAN_LINE: &'static [u8] = b"deepscanline";
    pub const DEEP_TILE: &'static [u8] = b"deeptile";
}


pub use crate::file::data::compression::Compression;

pub type DataWindow = I32Box2;
pub type DisplayWindow = I32Box2;

/// all limits are inclusive, so when calculating dimensions, +1 must be added
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I32Box2 {
    pub x_min: i32, pub y_min: i32,
    pub x_max: i32, pub y_max: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct F32Box2 {
    pub x_min: f32, pub y_min: f32,
    pub x_max: f32, pub y_max: f32,
}

/// followed by a null byte
/// sorted alphabetically?
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelList {
    pub list: SmallVec<[Channel; 5]>,
    pub bytes_per_pixel: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Channel {
    /// zero terminated, 1 to 255 bytes
    pub name: Text,

    /// is a i32 in file
    pub pixel_type: PixelType,

    pub is_linear: bool,

    /// three signed chars, should be zero
    pub reserved: [i8; 3],

    /// can be used for chroma-subsampling
    /// other than 1 are allowed only in flat, scan-line based images.
    /// If deep or tiled, x and y sampling rates for all of its channels must be 1.
    // TODO include in header validation!
    pub x_sampling: i32,

    /// can be used for chroma-subsampling
    /// other than 1 are allowed only in flat, scan-line based images.
    /// If deep or tiled, x and y sampling rates for all of its channels must be 1.
    // TODO include in header validation!
    pub y_sampling: i32,
}

#[derive(Clone, Debug, Eq, PartialEq, Copy)]
pub enum PixelType {
    U32, F16, F32,
}

/// If a file doesn't have a chromaticities attribute, display software
/// should assume that the file's primaries and the white point match Rec. ITU-R BT.709-3:
//CIE x, y
//red
//0.6400, 0.3300
//green 0.3000, 0.6000
//blue
//0.1500, 0.0600
//white 0.3127, 0.3290
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chromaticities {
    pub red_x: f32,     pub red_y: f32,
    pub green_x: f32,   pub green_y: f32,
    pub blue_x: f32,    pub blue_y: f32,
    pub white_x: f32,   pub white_y: f32
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum EnvironmentMap {
    LatitudeLongitude,
    Cube,
}

/// uniquely identifies a motion picture film frame
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct KeyCode {
    pub film_manufacturer_code: i32,
    pub film_type: i32,
    pub film_roll_prefix: i32,

    pub count: i32,

    pub perforation_offset: i32,
    pub perforations_per_frame: i32,
    pub perforations_per_count: i32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LineOrder {
    IncreasingY,
    DecreasingY,
    RandomY,
}

#[derive(Clone, Eq, PartialEq)]
pub struct Preview {
    pub width: u32,
    pub height: u32,

    /// 4 × width × height bytes,
    /// Scan lines are stored top to bottom; within a scan line pixels are stored from left
    /// to right. A pixel consists of four unsigned chars, R, G, B, A
    pub pixel_data: Vec<i8>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TileDescription {
    pub x_size: u32, pub y_size: u32,
    pub level_mode: LevelMode,
    pub rounding_mode: RoundingMode,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LevelMode {
    Singular, MipMap, RipMap,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RoundingMode {
    Down, Up,
}


use crate::file::io::*;
use crate::file::io;
use std::cmp::Ordering;
use crate::error::{ReadResult, WriteResult, ReadError};

impl Text {
    pub fn from_str(str_value: &str) -> Self {
        debug_assert_eq!(
            str_value.bytes().len(), str_value.chars().count(),
            "only single-byte chars supported by open exr" // TODO is this true?
        );

        Text { bytes: SmallVec::from_slice(str_value.as_bytes()) }
    }

    pub fn from_bytes(bytes: SmallVec<[u8; 16]>) -> Self {
        Text { bytes }
    }

    /// panics if value is too long (31 bytes max)
    pub fn from_str_32(str_value: &str) -> Self {
        assert!(str_value.as_bytes().len() < 32, "max text length is 31");
        Self::from_str(str_value)
    }

    /// panics if value is too long (31 bytes max)
    pub fn from_str_256(str_value: &str) -> Self {
        assert!(str_value.as_bytes().len() < 256, "max text length is 255");
        Self::from_str(str_value)
    }

    pub fn to_string(&self) -> String {
        self.bytes.iter()
            .map(|&byte| byte as char)
            .collect() // TODO is this ascii and can be treated as utf-8?
    }

    pub fn validate(&self, long_names: Option<bool>) -> Validity {
        Self::validate_bytes(self.bytes.as_slice(), long_names)
    }

    pub fn validate_bytes(text: &[u8], long_names: Option<bool>) -> Validity {
        let is_valid = !text.is_empty() && match long_names {
            Some(false) => text.len() < 32,
            Some(true) => text.len() < 256,
            None => true,
        };

        if is_valid { Ok(()) } else {
            if text.is_empty() {
                Err(Invalid::Content(Value::Text, Required::Min(1)).into())
            } else if long_names.unwrap() {
                Err(Invalid::Content(Value::Text, Required::Max(255)).into())
            } else {
                Err(Invalid::Content(Value::Text, Required::Max(31)).into())
            }
        }
    }


    pub fn null_terminated_byte_size(&self) -> usize {
        self.bytes.len() + SequenceEnd::byte_size()
    }

    pub fn i32_sized_byte_size(&self) -> usize {
        self.bytes.len() + 0_i32.byte_size()
    }

    pub fn write_i32_sized<W: Write>(&self, write: &mut W, long_names: Option<bool>) -> WriteResult {
        (self.bytes.len() as i32).write(write)?;
        Self::write_unsized_bytes(self.bytes.as_slice(), write, long_names)
    }

    pub fn write_unsized_bytes<W: Write>(bytes: &[u8], write: &mut W, long_names: Option<bool>) -> WriteResult {
        Text::validate_bytes(bytes, long_names)?;
        io::write_u8_array(write, bytes)
    }

    pub fn read_i32_sized<R: Read>(read: &mut R) -> ReadResult<Self> {
        let size = i32::read(read)? as usize;
        Text::read_sized(read, size)
    }

    pub fn read_sized<R: Read>(read: &mut R, size: usize) -> ReadResult<Self> {
        // TODO read into small vec without heap
        Ok(Text::from_bytes(SmallVec::from_vec(read_u8_vec(read, size, 1024)?)))
    }

    pub fn write_null_terminated<W: Write>(&self, write: &mut W, long_names: Option<bool>) -> WriteResult {
        Self::write_unsized_bytes(self.bytes.as_slice(), write, long_names)?;
        io::SequenceEnd::write(write)
    }

    pub fn write_null_terminated_bytes<W: Write>(bytes: &[u8], write: &mut W, long_names: Option<bool>) -> WriteResult {
        Text::write_unsized_bytes(bytes, write, long_names)?;
        io::SequenceEnd::write(write)
    }

    pub fn read_null_terminated<R: Read>(read: &mut R) -> ReadResult<Self> {
        let mut bytes = SmallVec::new();

        loop {
            match u8::read(read)? {
                0 => break,
                non_terminator => bytes.push(non_terminator),
            }
        }

        Ok(Text { bytes })
    }

    fn read_vec_of_i32_sized(
        read: &mut PeekRead<impl Read>, attribute_value_byte_size: u32
    ) -> ReadResult<Vec<Text>>
    {
        let mut result = Vec::with_capacity(2);

        // length of the text-vector can be inferred from attribute size
        let mut processed_bytes = 0;

        while processed_bytes < attribute_value_byte_size {
            let text = Text::read_i32_sized(read)?;
            processed_bytes += ::std::mem::size_of::<i32>() as u32; // size i32 of the text
            processed_bytes += text.bytes.len() as u32;
            result.push(text);
        }

        debug_assert_eq!(processed_bytes, attribute_value_byte_size, "text lengths did not match attribute size");
        Ok(result)
    }

    /// allows any text length since it is only used for attribute values,
    /// but not attribute names, attribute type names, or channel names
    fn write_vec_of_i32_sized_texts<W: Write>(write: &mut W, texts: &[Text]) -> WriteResult {
        // length of the text-vector can be inferred from attribute size
        for text in texts {
            text.write_i32_sized(write, None)?;
        }
        Ok(())
    }

}

impl ::std::fmt::Debug for Text {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "\"{}\"", self.to_string())
    }
}


impl ChannelList {
    pub fn new(mut channels: SmallVec<[Channel; 5]>) -> Self {
        channels.sort_by(|a, b| a.name.cmp(&b.name));

        ChannelList {
            bytes_per_pixel: channels.iter().map(|channel| channel.pixel_type.bytes_per_sample()).sum(),
            list: channels,
        }
    }
}

impl Kind {
    const TYPE_NAME: &'static [u8] = attribute_type_names::TEXT;

    pub fn parse(text: Text) -> ReadResult<Self> {
        match text.bytes.as_slice() {
            kind::SCAN_LINE => Ok(Kind::ScanLine),
            kind::TILE => Ok(Kind::Tile),
            kind::DEEP_SCAN_LINE => Ok(Kind::DeepScanLine),
            kind::DEEP_TILE => Ok(Kind::DeepTile),
            _ => Err(ReadError::Invalid(Invalid::Content(
                Value::Attribute("type"),
                Required::OneOf(&["", "", "", ""])
            ))),
        }
    }

    pub fn write(&self, write: &mut impl Write) -> WriteResult {
        write_u8_array(write, self.to_text_bytes())
    }

    pub fn to_text_bytes(&self) -> &[u8] {
        match self {
            Kind::ScanLine => kind::SCAN_LINE,
            Kind::Tile => kind::TILE,
            Kind::DeepScanLine => kind::DEEP_SCAN_LINE,
            Kind::DeepTile => kind::DEEP_TILE,
        }
    }

    pub fn byte_size(&self) -> usize {
        self.to_text_bytes().len()
    }
}


impl I32Box2 {
    pub fn validate(&self) -> Validity {
        if self.x_min > self.x_max || self.y_min > self.y_max {
            Err(Invalid::Combination(&[
                Value::Attribute("box2i min"),
                Value::Attribute("box2i max")
            ]))
        } else {
            Ok(())
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (
            // see technical introduction p. 1
            (self.x_max - self.x_min) as u32 + 1, // TODO checked_sub
            (self.y_max - self.y_min) as u32 + 1,
        )
    }

    pub fn byte_size(&self) -> usize {
        4 * self.x_min.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        // validate?
        self.x_min.write(write)?;
        self.y_min.write(write)?;
        self.x_max.write(write)?;
        self.y_max.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        Ok(I32Box2 {
            x_min: i32::read(read)?,
            y_min: i32::read(read)?,
            x_max: i32::read(read)?,
            y_max: i32::read(read)?,
        })
    }
}

impl F32Box2 {
    pub fn byte_size(&self) -> usize {
        4 * self.x_min.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.x_min.write(write)?;
        self.y_min.write(write)?;
        self.x_max.write(write)?;
        self.y_max.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        Ok(F32Box2 {
            x_min: f32::read(read)?,
            y_min: f32::read(read)?,
            x_max: f32::read(read)?,
            y_max: f32::read(read)?,
        })
    }
}

impl PixelType {
    pub fn bytes_per_sample(&self) -> u32 {
        match self {
            PixelType::F16 => 2, // TODO use mem::sizeof
            PixelType::F32 => 4, // TODO use mem::sizeof
            PixelType::U32 => 4, // TODO use mem::sizeof
        }
    }

    pub fn byte_size(&self) -> usize {
        0_i32.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        match *self {
            PixelType::U32 => 0_i32,
            PixelType::F16 => 1_i32,
            PixelType::F32 => 2_i32,
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        // there's definitely going to be more than 255 different pixel types
        // in the future, when exr is still used
        Ok(match i32::read(read)? {
            0 => PixelType::U32,
            1 => PixelType::F16,
            2 => PixelType::F32,
            _ => return Err(Invalid::Content(
                Value::Enum("pixelType"),
                Required::Range{ min: 0, max: 2 }
            ).into())
        })
    }
}

impl Channel {
    pub fn subsampled_pixels(&self, dimensions: (u32, u32)) -> u32 {
        let (width, height) = self.subsampled_resolution(dimensions);
        width * height
    }

    pub fn subsampled_resolution(&self, dimensions: (u32, u32)) -> (u32, u32) {
        (
            dimensions.0 / self.x_sampling as u32,
            dimensions.1 / self.y_sampling as u32,
        )
    }

    pub fn byte_size(&self) -> usize {
        self.name.null_terminated_byte_size()
            + self.pixel_type.byte_size()
            + 1 // is_linear
            + self.reserved.len()
            + self.x_sampling.byte_size()
            + self.y_sampling.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> WriteResult {
        Text::write_null_terminated(&self.name, write, Some(long_names))?;
        self.pixel_type.write(write)?;

        match self.is_linear {
            false => 0_u8,
            true  => 1_u8,
        }.write(write)?;

        write_i8_array(write, &self.reserved)?;
        self.x_sampling.write(write)?;
        self.y_sampling.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let name = Text::read_null_terminated(read)?;
        let pixel_type = PixelType::read(read)?;

        let is_linear = match u8::read(read)? {
            1 => true,
            0 => false,
            _ => return Err(Invalid::Content(
                Value::Enum("pLinear"),
                Required::Range{ min: 0, max: 1 }
            ).into())
        };

        let mut reserved = [0; 3];
        read_i8_array(read, &mut reserved)?;

        let x_sampling = i32::read(read)?;
        let y_sampling = i32::read(read)?;

        Ok(Channel {
            name, pixel_type, is_linear,
            reserved, x_sampling, y_sampling,
        })
    }

    pub fn list_byte_size(channels: &ChannelList) -> usize {
        channels.list.iter().map(Channel::byte_size).sum::<usize>() + SequenceEnd::byte_size()
    }

    pub fn write_all<W: Write>(channels: &ChannelList, write: &mut W, long_names: bool) -> WriteResult {
        // FIXME validate if channel names are sorted alphabetically

        for channel in &channels.list {
            channel.write(write, long_names)?;
        }

        SequenceEnd::write(write)
    }

    pub fn read_all(read: &mut PeekRead<impl Read>) -> ReadResult<ChannelList> {
        let mut channels = SmallVec::new();
        while !SequenceEnd::has_come(read)? {
            channels.push(Channel::read(read)?);
        }

        Ok(ChannelList::new(channels))
    }
}

impl Chromaticities {
    pub fn byte_size(&self) -> usize {
        8 * self.red_x.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.red_x.write(write)?;
        self.red_y.write(write)?;
        self.green_x.write(write)?;
        self.green_y.write(write)?;
        self.blue_x.write(write)?;
        self.blue_y.write(write)?;
        self.white_x.write(write)?;
        self.white_y.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        Ok(Chromaticities {
            red_x: f32::read(read)?,
            red_y: f32::read(read)?,
            green_x: f32::read(read)?,
            green_y: f32::read(read)?,
            blue_x: f32::read(read)?,
            blue_y: f32::read(read)?,
            white_x: f32::read(read)?,
            white_y: f32::read(read)?,
        })
    }
}

impl Compression {
    pub fn byte_size(&self) -> usize {
        0_u8.byte_size()
    }

    pub fn write<W: Write>(self, write: &mut W) -> WriteResult {
        use self::Compression::*;
        match self {
            None => 0_u8,
            RLE => 1_u8,
            ZIP1 => 2_u8,
            ZIP16 => 3_u8,
            PIZ => 4_u8,
            PXR24 => 5_u8,
            B44 => 6_u8,
            B44A => 7_u8,
            DWAA => 8_u8,
            DWAB => 9_u8,
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        use self::Compression::*;
        Ok(match u8::read(read)? {
            0 => None,
            1 => RLE,
            2 => ZIP1,
            3 => ZIP16,
            4 => PIZ,
            5 => PXR24,
            6 => B44,
            7 => B44A,
            8 => DWAA,
            9 => DWAB,
            _ => return Err(Invalid::Content(
                Value::Enum("compression"),
                Required::Range { min: 0, max: 9 }
            ).into()),
        })
    }
}

impl EnvironmentMap {
    pub fn byte_size(&self) -> usize {
        0_u32.byte_size()
    }

    pub fn write<W: Write>(self, write: &mut W) -> WriteResult {
        use self::EnvironmentMap::*;
        match self {
            LatitudeLongitude => 0_u8,
            Cube => 1_u8
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        use self::EnvironmentMap::*;
        Ok(match u8::read(read)? {
            0 => LatitudeLongitude,
            1 => Cube,

            _ => return Err(Invalid::Content(
                Value::Enum("envmap"),
                Required::Range { min: 0, max: 1 }
            ).into()),
        })
    }
}

impl KeyCode {
    pub fn byte_size(&self) -> usize {
        6 * self.film_manufacturer_code.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.film_manufacturer_code.write(write)?;
        self.film_type.write(write)?;
        self.film_roll_prefix.write(write)?;
        self.count.write(write)?;
        self.perforation_offset.write(write)?;
        self.perforations_per_count.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
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
    pub fn byte_size(&self) -> usize {
        0_u32.byte_size()
    }

    pub fn write<W: Write>(self, write: &mut W) -> WriteResult {
        use self::LineOrder::*;
        match self {
            IncreasingY => 0_u8,
            DecreasingY => 1_u8,
            RandomY => 2_u8,
        }.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        use self::LineOrder::*;
        Ok(match u8::read(read)? {
            0 => IncreasingY,
            1 => DecreasingY,
            2 => RandomY,
            _ => return Err(Invalid::Content(
                Value::Enum("lineOrder"),
                Required::Range { min: 0, max: 2 }
            ).into()),
        })
    }
}

impl Preview {
    pub fn validate(&self) -> Validity {
        if self.width * self.height * 4 != self.pixel_data.len() as u32 {
            Err(Invalid::Combination(&[
                Value::Attribute("Preview dimensions"),
                Value::Attribute("Preview pixel data length"),
            ]))
        } else {
            Ok(())
        }
    }

    pub fn byte_size(&self) -> usize {
        self.width.byte_size()
            + self.height.byte_size()
            + self.pixel_data.len()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.width.write(write)?;
        self.height.write(write)?;
        write_i8_array(write, &self.pixel_data)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let components_per_pixel = 4;
        let width = u32::read(read)?;
        let height = u32::read(read)?;

        // TODO carefully allocate
        let mut pixel_data = vec![0; (width * height * components_per_pixel) as usize];
        read_i8_array(read, &mut pixel_data)?;

        let preview = Preview {
            width, height,
            pixel_data,
        };

        preview.validate()?;
        Ok(preview)
    }
}

impl ::std::fmt::Debug for Preview {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "Preview {{ width: {}, height: {} }}", self.width, self.height)
    }
}

impl TileDescription {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.x_size, self.y_size)
    }

    pub fn byte_size(&self) -> usize {
        self.x_size.byte_size() + self.y_size.byte_size()
         + 1 // (level mode + rounding mode)
    }

    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.x_size.write(write)?;
        self.y_size.write(write)?;

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
        mode.write(write)
    }

    pub fn read<R: Read>(read: &mut R) -> ReadResult<Self> {
        let x_size = u32::read(read)?;
        let y_size = u32::read(read)?;

        let mode = u8::read(read)?; // wow you really saved that one byte here

        // mode = level_mode + (rounding_mode * 16)
        let level_mode = mode & 0b00001111; // wow that works
        let rounding_mode = mode >> 4; // wow that works

        let level_mode = match level_mode {
            0 => LevelMode::Singular,
            1 => LevelMode::MipMap,
            2 => LevelMode::RipMap,
            _ => return Err(Invalid::Content(
                Value::Enum("level mode"),
                Required::Range { min: 0, max: 2 }
            ).into()),
        };

        let rounding_mode = match rounding_mode {
            0 => RoundingMode::Down,
            1 => RoundingMode::Up,
            _ => return Err(Invalid::Content(
                Value::Enum("rounding mode"),
                Required::Range { min: 0, max: 1 }
            ).into()),
        };

        Ok(TileDescription { x_size, y_size, level_mode, rounding_mode, })
    }
}

impl Attribute {
    pub fn byte_size(&self) -> usize {
        self.name.null_terminated_byte_size()
            + self.value.kind_name().len() + SequenceEnd::byte_size()
            + 0_i32.byte_size() // serialized byte size
            + self.value.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> WriteResult {
        self.name.write_null_terminated(write, Some(long_names))?;
        Text::write_null_terminated_bytes(self.value.kind_name(), write, Some(long_names))?;
        (self.value.byte_size() as i32).write(write)?;
        self.value.write(write, long_names)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read(read: &mut PeekRead<impl Read>) -> ReadResult<Self> {
        let name = Text::read_null_terminated(read)?;
        let kind = Text::read_null_terminated(read)?;
        let size = i32::read(read)? as u32; // TODO .checked_cast.ok_or(err:negative)
        let value = AttributeValue::read(read, kind, size)?;
        Ok(Attribute { name, value, })
    }
}



impl AttributeValue {
    pub fn byte_size(&self) -> usize {
        use self::AttributeValue::*;

        match *self {
            I32Box2(value) => value.byte_size(),
            F32Box2(value) => value.byte_size(),

            I32(value) => value.byte_size(),
            F32(value) => value.byte_size(),
            F64(value) => value.byte_size(),

            Rational(a, b) => { a.byte_size() + b.byte_size() },
            TimeCode(a, b) => { a.byte_size() + b.byte_size() },

            I32Vec2(x, y) => { x.byte_size() + y.byte_size() },
            F32Vec2(x, y) => { x.byte_size() + y.byte_size() },
            I32Vec3(x, y, z) => { x.byte_size() + y.byte_size() + z.byte_size() },
            F32Vec3(x, y, z) => { x.byte_size() + y.byte_size() + z.byte_size() },

            ChannelList(ref channels) => Channel::list_byte_size(channels),
            Chromaticities(ref value) => value.byte_size(),
            Compression(value) => value.byte_size(),
            EnvironmentMap(value) => value.byte_size(),

            KeyCode(value) => value.byte_size(),
            LineOrder(value) => value.byte_size(),

            F32Matrix3x3(ref value) => value.len() * value[0].byte_size(),
            F32Matrix4x4(ref value) => value.len() * value[0].byte_size(),

            Preview(ref value) => value.byte_size(),

            // attribute value texts never have limited size.
            // also, don't serialize size, as it can be inferred from attribute size
            Text(ref value) => value.bytes.len(),

            TextVector(ref value) => value.iter().map(self::Text::i32_sized_byte_size).sum(),
            TileDescription(ref value) => value.byte_size(),
            Custom { ref bytes, .. } => bytes.len(),
            Kind(ref kind) => kind.byte_size()
        }
    }

    pub fn kind_name(&self) -> &[u8] {
        use self::AttributeValue::*;
        use self::attribute_type_names as ty;

        match *self {
            I32Box2(_) =>  ty::I32BOX2,
            F32Box2(_) =>  ty::F32BOX2,
            I32(_) =>  ty::I32,
            F32(_) =>  ty::F32,
            F64(_) =>  ty::F64,
            Rational(_, _) => ty::RATIONAL,
            TimeCode(_, _) => ty::TIME_CODE,
            I32Vec2(_, _) => ty::I32VEC2,
            F32Vec2(_, _) => ty::F32VEC2,
            I32Vec3(_, _, _) => ty::I32VEC3,
            F32Vec3(_, _, _) => ty::F32VEC3,
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
            Kind(_) => super::Kind::TYPE_NAME,
        }
    }

    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> WriteResult {
        use self::AttributeValue::*;
        match *self {
            I32Box2(value) => value.write(write),
            F32Box2(value) => value.write(write),

            I32(value) => value.write(write),
            F32(value) => value.write(write),
            F64(value) => value.write(write),

            Rational(a, b) => { a.write(write)?; b.write(write) },
            TimeCode(a, b) => { a.write(write)?; b.write(write) },

            I32Vec2(x, y) => { x.write(write)?; y.write(write) },
            F32Vec2(x, y) => { x.write(write)?; y.write(write) },
            I32Vec3(x, y, z) => { x.write(write)?; y.write(write)?; z.write(write) },
            F32Vec3(x, y, z) => { x.write(write)?; y.write(write)?; z.write(write) },

            ChannelList(ref channels) => Channel::write_all(channels, write, long_names),
            Chromaticities(ref value) => value.write(write),
            Compression(value) => value.write(write),
            EnvironmentMap(value) => value.write(write),

            KeyCode(value) => value.write(write),
            LineOrder(value) => value.write(write),

            F32Matrix3x3(mut value) => write_f32_array(write, &mut value),
            F32Matrix4x4(mut value) => write_f32_array(write, &mut value),

            Preview(ref value) => { value.validate()?; value.write(write) },

            // attribute value texts never have limited size.
            // also, don't serialize size, as it can be inferred from attribute size
            Text(ref value) => write_u8_array(write, value.bytes.as_slice()),

            TextVector(ref value) => self::Text::write_vec_of_i32_sized_texts(write, value),
            TileDescription(ref value) => value.write(write),
            Custom { ref bytes, .. } => write_u8_array(write, &bytes), // write.write(&bytes).map(|_| ()),
            Kind(kind) => kind.write(write)
        }
    }

    pub fn read(read: &mut PeekRead<impl Read>, kind: Text, byte_size: u32) -> ReadResult<Self> {
        use self::AttributeValue::*;
        use self::attribute_type_names as ty;

        Ok(match kind.bytes.as_slice() {
            ty::I32BOX2 => I32Box2(self::I32Box2::read(read)?),
            ty::F32BOX2 => F32Box2(self::F32Box2::read(read)?),

            ty::I32 => I32(i32::read(read)?),
            ty::F32 => F32(f32::read(read)?),
            ty::F64 => F64(f64::read(read)?),

            ty::RATIONAL => Rational(i32::read(read)?, u32::read(read)?),
            ty::TIME_CODE => TimeCode(u32::read(read)?, u32::read(read)?),

            ty::I32VEC2 => I32Vec2(i32::read(read)?, i32::read(read)?),
            ty::F32VEC2 => F32Vec2(f32::read(read)?, f32::read(read)?),
            ty::I32VEC3 => I32Vec3(i32::read(read)?, i32::read(read)?, i32::read(read)?),
            ty::F32VEC3 => F32Vec3(f32::read(read)?, f32::read(read)?, f32::read(read)?),

            ty::CHANNEL_LIST    => ChannelList(self::Channel::read_all(read)?),
            ty::CHROMATICITIES  => Chromaticities(self::Chromaticities::read(read)?),
            ty::COMPRESSION     => Compression(self::Compression::read(read)?),
            ty::ENVIRONMENT_MAP => EnvironmentMap(self::EnvironmentMap::read(read)?),

            ty::KEY_CODE   => KeyCode(self::KeyCode::read(read)?),
            ty::LINE_ORDER => LineOrder(self::LineOrder::read(read)?),

            ty::F32MATRIX3X3 => F32Matrix3x3({
                let mut result = [0.0_f32; 9];
                read_f32_array(read, &mut result)?;
                result
            }),

            ty::F32MATRIX4X4 => F32Matrix4x4({
                let mut result = [0.0_f32; 16];
                read_f32_array(read, &mut result)?;
                result
            }),

            ty::PREVIEW     => Preview(self::Preview::read(read)?),
            ty::TEXT        => Text(self::Text::read_sized(read, byte_size as usize)?),
            ty::TEXT_VECTOR => TextVector(self::Text::read_vec_of_i32_sized(read, byte_size)?),
            ty::TILES       => TileDescription(self::TileDescription::read(read)?),

            _ => {
                println!("Unknown attribute type: {:?}", kind.to_string());
                let mut bytes = vec![0_u8; byte_size as usize];
                read_u8_array(read, &mut bytes)?;
                Custom { kind, bytes }
            }
        })
    }

    pub fn to_tile_description(&self) -> Result<TileDescription, Invalid> {
        match *self {
            AttributeValue::TileDescription(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("tiledesc")).into()), // TODO make these constants!
        }
    }

    pub fn to_i32(&self) -> Result<i32, Invalid> {
        match *self {
            AttributeValue::I32(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("i32")).into()),
        }
    }

    pub fn to_f32(&self) -> Result<f32, Invalid> {
        match *self {
            AttributeValue::F32(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("f32")).into()),
        }
    }

    pub fn to_i32_box_2(&self) -> Result<I32Box2, Invalid> {
        match *self {
            AttributeValue::I32Box2(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("box2i")).into()),
        }
    }

    pub fn to_f32_vec_2(&self) -> Result<(f32, f32), Invalid> {
        match *self {
            AttributeValue::F32Vec2(x, y) => Ok((x, y)),
            _ => Err(Invalid::Type(Required::Exact("v2f")).into()),
        }
    }

    pub fn to_line_order(&self) -> Result<LineOrder, Invalid> {
        match *self {
            AttributeValue::LineOrder(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("lineorder")).into()),
        }
    }

    pub fn to_compression(&self) -> Result<Compression, Invalid> {
        match *self {
            AttributeValue::Compression(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("compression")).into()),
        }
    }

    pub fn to_text(self) -> Result<Text, Invalid> {
        match self {
            AttributeValue::Text(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("string")).into()),
        }
    }

    pub fn to_kind(self) -> Result<Kind, Invalid> {
        match self {
            AttributeValue::Kind(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("type string")).into()),
        }
    }

    pub fn to_channel_list(self) -> Result<ChannelList, Invalid> {
        match self {
            AttributeValue::ChannelList(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("chlist")).into()),
        }
    }

    pub fn to_chromaticities(&self) -> Result<Chromaticities, Invalid> {
        match *self {
            AttributeValue::Chromaticities(value) => Ok(value),
            _ => Err(Invalid::Type(Required::Exact("chromaticities")).into()),
        }
    }
}

pub mod attribute_type_names {
    macro_rules! define_attribute_type_names {
        ( $($name: ident : $value: expr),* ) => {
            $(
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

pub mod required {
    macro_rules! define_required_attribute_names {
        ( $($name: ident : $value: expr),* ) => {
            $(
                pub const $name: &'static [u8] = $value;
            )*
        };
    }

    define_required_attribute_names! {
        TILES: b"tiles",
        NAME: b"name",
        TYPE: b"type",
        VERSION: b"version",
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

impl RoundingMode {

    /// For x > 0, floorLog2(y) returns floor(log(x)/log(2))
    // taken from https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp
    pub fn floor_log_2(mut number: u32) -> u32 {
        debug_assert_ne!(number, 0);

        // index of the most significant nonzero bit
        let mut log = 0;

        // TODO check if this unrolls properly
        while number > 1 {
            log += 1;
            number >>= 1;
        }

        log
    }

    /// For x > 0, ceilLog2(y) returns ceil(log(x)/log(2))
    // taken from https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp
    pub fn ceil_log_2(mut number: u32) -> u32 {
        debug_assert_ne!(number, 0);

        let mut log = 0;
        let mut round_up = 0;

        // TODO check if this unrolls properly
        while number > 1 {
            if number & 1 != 0 {
                round_up = 1;
            }

            log +=  1;
            number >>= 1;
        }

        log + round_up
    }

    pub fn log2(self, number: u32) -> u32 {
        match self {
            RoundingMode::Down => Self::floor_log_2(number),
            RoundingMode::Up => Self::ceil_log_2(number),
        }
    }

    pub fn divide(self, dividend: u32, divisor: u32) -> u32 {
        match self {
            RoundingMode::Up => (dividend + divisor - 1) / divisor, // only works for positive numbers
            RoundingMode::Down => dividend / divisor,
        }
    }
}



impl Ord for Text {
    // TODO performance?
    fn cmp(&self, other: &Self) -> Ordering {
        self.to_string().cmp(&other.to_string())
    }
}

impl PartialOrd for Text {
    // TODO performance?
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.to_string().partial_cmp(&other.to_string())
    }
}


#[cfg(test)]
mod test {
    use super::*;
    use ::std::io::Cursor;

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
                x_size: 31,
                y_size: 7,
                level_mode: LevelMode::MipMap,
                rounding_mode: RoundingMode::Down,
            },

            TileDescription {
                x_size: 0,
                y_size: 0,
                level_mode: LevelMode::Singular,
                rounding_mode: RoundingMode::Up,
            },

            TileDescription {
                x_size: 4294967294,
                y_size: 4294967295,
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
                name: Text::from_str("greeting"),
                value: AttributeValue::Text(Text::from_str("hello")),
            },
            Attribute {
                name: Text::from_str("age"),
                value: AttributeValue::I32(923),
            },
            Attribute {
                name: Text::from_str("leg count"),
                value: AttributeValue::F64(9.114939599234),
            },
            Attribute {
                name: Text::from_str("rabbit area"),
                value: AttributeValue::F32Box2(F32Box2 {
                    x_min: 23.4234,
                    y_min: 345.23,
                    x_max: 68623.0,
                    y_max: 3.12425926538,
                }),
            },
            Attribute {
                name: Text::from_str("tests are difficult"),
                value: AttributeValue::TextVector(vec![
                    Text::from_str("sdoifjpsdv"),
                    Text::from_str("sdoifjpsdvxxxx"),
                    Text::from_str("sdoifjasd"),
                    Text::from_str("sdoifj"),
                    Text::from_str("sdoifjddddddddasdasd"),
                ]),
            },
            Attribute {
                name: Text::from_str("what should we eat tonight"),
                value: AttributeValue::Preview(Preview {
                    width: 10,
                    height: 30,
                    pixel_data: vec![31; 10 * 30 * 4],
                }),
            },
            Attribute {
                name: Text::from_str("leg count, again"),
                value: AttributeValue::ChannelList(ChannelList {
                    list: smallvec![
                        Channel {
                            name: Text::from_str("Green"),
                            pixel_type: PixelType::F16,
                            is_linear: false,
                            reserved: [0, 0, 0],
                            x_sampling: 1,
                            y_sampling: 2,
                        },
                        Channel {
                            name: Text::from_str("Red"),
                            pixel_type: PixelType::F32,
                            is_linear: true,
                            reserved: [0, 1, 0],
                            x_sampling: 1,
                            y_sampling: 2,
                        },
                        Channel {
                            name: Text::from_str("Purple"),
                            pixel_type: PixelType::U32,
                            is_linear: false,
                            reserved: [1, 2, 7],
                            x_sampling: 0,
                            y_sampling: 0,
                        }
                    ],
                    bytes_per_pixel: 0
                }),
            },
        ];

        for attribute in &attributes {
            let mut bytes = Vec::new();
            attribute.write(&mut bytes, true).unwrap();
            assert_eq!(attribute.byte_size(), bytes.len(), "attribute.byte_size() for {:?}", attribute);

            let new_attribute = Attribute::read(&mut PeekRead::new(Cursor::new(bytes))).unwrap();
            assert_eq!(*attribute, new_attribute, "attribute round trip");
        }


        {
            let too_large_named = Attribute {
                name: Text::from_str("asdkaspfokpaosdkfpaokswdpoakpsfokaposdkf"),
                value: AttributeValue::I32(0),
            };

            let mut bytes = Vec::new();
            too_large_named.write(&mut bytes, false).expect_err("name length check failed");
        }

        {
            let way_too_large_named = Attribute {
                name: Text::from_bytes(SmallVec::from_vec(vec![0; 257])),
                value: AttributeValue::I32(0),
            };

            let mut bytes = Vec::new();
            way_too_large_named.write(&mut bytes, true).expect_err("name length check failed");
        }
    }
}