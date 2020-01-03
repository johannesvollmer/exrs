use smallvec::SmallVec;











#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: Text,

    /// kind can be inferred from value
    /// size in bytes can be inferred from value
    pub value: AnyValue,
}


// TODO custom attribute
#[derive(Debug, Clone, PartialEq)]
pub enum AnyValue {
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

    TimeCode(TimeCodes),

    I32Vec2(i32, i32),
    F32Vec2(f32, f32),
    I32Vec3(i32, i32, i32),
    F32Vec3(f32, f32, f32),

    Custom { kind: Text, bytes: Vec<u8> }
}


/// null-terminated text strings.
/// max 31 bytes long (if bit 10 is set to 0),
/// or max 255 bytes long (if bit 10 is set to 1).
/// must be at least 1 byte (to avoid confusion with null-terminators)
// TODO non public fields?
#[derive(Clone, Eq, PartialEq)]
pub struct Text {
    /// vector does not include null terminator
    /// those strings will mostly be "R", "G", "B" or "deepscanlineimage"
    pub bytes: TextBytes,
}

// TODO enable conversion to rust time
pub type TimeCodes = (u32, u32);

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


pub use crate::compression::Compression;

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
    pub bytes_per_pixel: u32, // FIXME only makes sense for flat images!
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
    pub sampling: (u32, u32),
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
    pub size: (u32, u32),
    pub level_mode: LevelMode,
    pub rounding_mode: RoundingMode,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LevelMode {
    Singular, MipMap, RipMap,
}

pub type TextBytes = SmallVec<[u8; 24]>;



use crate::io::*;
use crate::meta::sequence_end;
use std::cmp::Ordering;
use crate::error::*;
use crate::math::RoundingMode;


fn invalid_type() -> Error {
    Error::invalid("wrong attribute type")
}


impl Text {
    pub fn from_str(str: &str) -> Option<Self> {
        let vec : Option<TextBytes> = str.chars()
            .map(|char| Some(char as u8)) // u8::try_from(char).ok())
            .collect();

        vec.map(Self::from_bytes_unchecked)
    }

    pub fn from_bytes_unchecked(bytes: TextBytes) -> Self {
        Text { bytes }
    }

    pub fn validate(&self, long_names: Option<bool>) -> PassiveResult {
        Self::validate_bytes(self.bytes.as_slice(), long_names)
    }

    pub fn validate_bytes(text: &[u8], long_names: Option<bool>) -> PassiveResult {
        let is_valid = !text.is_empty() && match long_names {
            Some(false) => text.len() < 32,
            Some(true) => text.len() < 256,
            None => true,
        };

        if is_valid { Ok(()) } else {
            if long_names.unwrap() {
                Err(Error::invalid("text longer than 255"))
            }
            else {
                Err(Error::invalid("text longer than 31"))
            }
        }
    }


    pub fn null_terminated_byte_size(&self) -> usize {
        self.bytes.len() + sequence_end::byte_size()
    }

    pub fn i32_sized_byte_size(&self) -> usize {
        self.bytes.len() + 0_i32.byte_size()
    }

    pub fn write_i32_sized<W: Write>(&self, write: &mut W, long_names: Option<bool>) -> PassiveResult {
        (self.bytes.len() as i32).write(write)?;
        Self::write_unsized_bytes(self.bytes.as_slice(), write, long_names)
    }

    pub fn write_unsized_bytes<W: Write>(bytes: &[u8], write: &mut W, long_names: Option<bool>) -> PassiveResult {
        Text::validate_bytes(bytes, long_names)?;
        u8::write_slice(write, bytes)?;
        Ok(())
    }

    pub fn read_i32_sized<R: Read>(read: &mut R, max_size: usize) -> Result<Self> {
        let size = i32::read(read)? as usize;
        Ok(Text::from_bytes_unchecked(SmallVec::from_vec(u8::read_vec(read, size, max_size.min(2048))?)))
    }

    pub fn read_sized<R: Read>(read: &mut R, size: usize) -> Result<Self> {
        // TODO read into small vec without heap?
        Ok(Text::from_bytes_unchecked(SmallVec::from_vec(u8::read_vec(read, size, 2048)?)))
    }

    pub fn write_null_terminated<W: Write>(&self, write: &mut W, long_names: Option<bool>) -> PassiveResult {
        if self.bytes.is_empty() { return Err(Error::invalid("text is empty")) } // required to avoid mixup with "sequece_end"
        Self::write_unsized_bytes(self.bytes.as_slice(), write, long_names)?;
        sequence_end::write(write)?;
        Ok(())
    }

    pub fn write_null_terminated_bytes<W: Write>(bytes: &[u8], write: &mut W, long_names: Option<bool>) -> PassiveResult {
        if bytes.is_empty() { return Err(Error::invalid("text is empty")) } // required to avoid mixup with "sequece_end"
        Text::write_unsized_bytes(bytes, write, long_names)?;
        sequence_end::write(write)?;
        Ok(())
    }

    pub fn read_null_terminated<R: Read>(read: &mut R, max_len: usize) -> Result<Self> {
        let mut bytes = SmallVec::new();

        loop {
            if bytes.len() >= max_len {
                return Err(Error::invalid("text too long"))
            }

            match u8::read(read)? {
                0 => break,
                non_terminator => bytes.push(non_terminator),
            }
        }

        Ok(Text { bytes })
    }

    fn read_vec_of_i32_sized(
        read: &mut PeekRead<impl Read>,
        total_byte_size: u32
    ) -> Result<Vec<Text>>
    {
        let mut result = Vec::with_capacity(2);

        // length of the text-vector can be inferred from attribute size
        let mut processed_bytes = 0;

        while processed_bytes < total_byte_size {
            let text = Text::read_i32_sized(read, total_byte_size as usize)?;
            processed_bytes += ::std::mem::size_of::<i32>() as u32; // size i32 of the text
            processed_bytes += text.bytes.len() as u32;
            result.push(text);
        }

        debug_assert_eq!(processed_bytes, total_byte_size, "text lengths did not match attribute size");
        Ok(result)
    }

    /// allows any text length since it is only used for attribute values,
    /// but not attribute names, attribute type names, or channel names
    fn write_vec_of_i32_sized_texts<W: Write>(write: &mut W, texts: &[Text]) -> PassiveResult {
        // length of the text-vector can be inferred from attribute size
        for text in texts {
            text.write_i32_sized(write, None)?;
        }
        Ok(())
    }
}

impl Into<String> for Text {
    fn into(self) -> String {
        self.to_string()
    }
}

impl ::std::fmt::Debug for Text {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "exr::Text(\"{}\")", self.to_string())
    }
}

// automatically implements to_string for us
impl ::std::fmt::Display for Text {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        use std::fmt::Write;

        for &byte in self.bytes.iter() {
            f.write_char(byte as char)?;
        }

        Ok(())
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

    pub fn parse(text: Text) -> Result<Self> {
        match text.bytes.as_slice() {
            kind::SCAN_LINE => Ok(Kind::ScanLine),
            kind::TILE => Ok(Kind::Tile),

            kind::DEEP_SCAN_LINE => Ok(Kind::DeepScanLine),
            kind::DEEP_TILE => Ok(Kind::DeepTile),

            _ => Err(Error::invalid("block type value")),
        }
    }

    pub fn write(&self, write: &mut impl Write) -> PassiveResult {
        u8::write_slice(write, self.to_text_bytes())?;
        Ok(())
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

    pub fn from_dimensions(size: (u32, u32)) -> Self {
        Self::new((0,0), size)
    }

    pub fn new(position: (i32, i32), size: (u32, u32)) -> Self {
        Self {
            x_min: position.0,
            y_min: position.1,
            x_max: position.0 + size.0 as i32 - 1,
            y_max: position.1 + size.1 as i32 - 1
        }
    }

    pub fn validate(&self, max: Option<(u32, u32)>) -> PassiveResult {
        if self.x_min > self.x_max || self.y_min > self.y_max {
            return Err(Error::invalid("box attribute dimensions"));
        }

        if let Some(bounds) = max {
            let dimensions = self.dimensions();

            if dimensions.0 > bounds.0 || dimensions.1 > bounds.1  {
                return Err(Error::invalid("box attribute dimensions"));
            }
        }

        Ok(())
    }

    pub fn dimensions(&self) -> (u32, u32) {
        debug_assert!(self.validate(None).is_ok());

        (
            (self.x_max + 1 - self.x_min) as u32,
            (self.y_max + 1 - self.y_min) as u32,
        )
    }

    pub fn byte_size(&self) -> usize {
        4 * self.x_min.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        // validate?
        self.x_min.write(write)?;
        self.y_min.write(write)?;
        self.x_max.write(write)?;
        self.y_max.write(write)?;
        Ok(())
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let value = I32Box2 {
            x_min: i32::read(read)?,
            y_min: i32::read(read)?,
            x_max: i32::read(read)?,
            y_max: i32::read(read)?,
        };

        value.validate(None)?;
        Ok(value)
    }
}

impl F32Box2 {
    pub fn byte_size(&self) -> usize {
        4 * self.x_min.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.x_min.write(write)?;
        self.y_min.write(write)?;
        self.x_max.write(write)?;
        self.y_max.write(write)?;
        Ok(())
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
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

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        match *self {
            PixelType::U32 => 0_i32,
            PixelType::F16 => 1_i32,
            PixelType::F32 => 2_i32,
        }.write(write)?;

        Ok(())
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        // there's definitely going to be more than 255 different pixel types
        // in the future, when exr is still used
        Ok(match i32::read(read)? {
            0 => PixelType::U32,
            1 => PixelType::F16,
            2 => PixelType::F32,
            _ => return Err(Error::invalid("pixel type attribute value")),
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
            dimensions.0 / self.sampling.0 as u32,
            dimensions.1 / self.sampling.1 as u32,
        )
    }

    pub fn byte_size(&self) -> usize {
        self.name.null_terminated_byte_size()
            + self.pixel_type.byte_size()
            + 1 // is_linear
            + self.reserved.len()
            + self.sampling.0.byte_size()
            + self.sampling.1.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> PassiveResult {
        Text::write_null_terminated(&self.name, write, Some(long_names))?;
        self.pixel_type.write(write)?;

        match self.is_linear {
            false => 0_u8,
            true  => 1_u8,
        }.write(write)?;

        i8::write_slice(write, &self.reserved)?;
        self.sampling.0.write(write)?;
        self.sampling.1.write(write)?;
        Ok(())
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let name = Text::read_null_terminated(read, 256)?;
        let pixel_type = PixelType::read(read)?;

        let is_linear = match u8::read(read)? {
            1 => true,
            0 => false,
            _ => return Err(Error::invalid("channel linearity attribute value")),
        };

        let mut reserved = [0; 3];
        i8::read_slice(read, &mut reserved)?;

        let x_sampling = i32::read(read)?;
        let y_sampling = i32::read(read)?; // TODO make u32?

        if x_sampling < 0 || y_sampling < 0 {
            return Err(Error::invalid("channel sampling value"))
        }

        Ok(Channel {
            name, pixel_type, is_linear,
            reserved, sampling: (x_sampling as u32, y_sampling as u32),
        })
    }

    pub fn list_byte_size(channels: &ChannelList) -> usize {
        channels.list.iter().map(Channel::byte_size).sum::<usize>() + sequence_end::byte_size()
    }

    pub fn write_all<W: Write>(channels: &ChannelList, write: &mut W, long_names: bool) -> PassiveResult {
        // FIXME validate if channel names are sorted alphabetically

        for channel in &channels.list {
            channel.write(write, long_names)?;
        }

        sequence_end::write(write)?;
        Ok(())
    }

    pub fn read_all(read: &mut PeekRead<impl Read>) -> Result<ChannelList> {
        let mut channels = SmallVec::new();
        while !sequence_end::has_come(read)? {
            channels.push(Channel::read(read)?);
        }

        Ok(ChannelList::new(channels))
    }
}

impl Chromaticities {
    pub fn byte_size(&self) -> usize {
        8 * self.red_x.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.red_x.write(write)?;
        self.red_y.write(write)?;
        self.green_x.write(write)?;
        self.green_y.write(write)?;
        self.blue_x.write(write)?;
        self.blue_y.write(write)?;
        self.white_x.write(write)?;
        self.white_y.write(write)?;
        Ok(())
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
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

    pub fn write<W: Write>(self, write: &mut W) -> PassiveResult {
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
            _ => return Err(Error::unsupported("compression method")),
        })
    }
}

impl EnvironmentMap {
    pub fn byte_size(&self) -> usize {
        0_u32.byte_size()
    }

    pub fn write<W: Write>(self, write: &mut W) -> PassiveResult {
        use self::EnvironmentMap::*;
        match self {
            LatitudeLongitude => 0_u8,
            Cube => 1_u8
        }.write(write)?;

        Ok(())
    }

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
    pub fn byte_size(&self) -> usize {
        6 * self.film_manufacturer_code.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.film_manufacturer_code.write(write)?;
        self.film_type.write(write)?;
        self.film_roll_prefix.write(write)?;
        self.count.write(write)?;
        self.perforation_offset.write(write)?;
        self.perforations_per_count.write(write)?;
        Ok(())
    }

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
    pub fn byte_size(&self) -> usize {
        0_u32.byte_size()
    }

    pub fn write<W: Write>(self, write: &mut W) -> PassiveResult {
        use self::LineOrder::*;
        match self {
            IncreasingY => 0_u8,
            DecreasingY => 1_u8,
            RandomY => 2_u8,
        }.write(write)?;

        Ok(())
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        use self::LineOrder::*;
        Ok(match u8::read(read)? {
            0 => IncreasingY,
            1 => DecreasingY,
            2 => RandomY,
            _ => return Err(Error::invalid("line order attribute value")),
        })
    }
}

impl Preview {
    pub fn validate(&self) -> PassiveResult {
        if self.width * self.height * 4 != self.pixel_data.len() as u32 {
            Err(Error::invalid("preview dimensions do not match content length"))
        }
        else {
            Ok(())
        }
    }

    pub fn byte_size(&self) -> usize {
        self.width.byte_size()
            + self.height.byte_size()
            + self.pixel_data.len()
    }

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.width.write(write)?;
        self.height.write(write)?;
        i8::write_slice(write, &self.pixel_data)?;
        Ok(())
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        let components_per_pixel = 4;
        let width = u32::read(read)?;
        let height = u32::read(read)?;

        // TODO carefully allocate
        let mut pixel_data = vec![0; (width * height * components_per_pixel) as usize];
        i8::read_slice(read, &mut pixel_data)?;

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
        write!(f, "Preview ({}x{} px)", self.width, self.height)
    }
}

impl TileDescription {

    pub fn byte_size(&self) -> usize {
        self.size.0.byte_size() + self.size.1.byte_size()
         + 1 // (level mode + rounding mode)
    }

    pub fn write<W: Write>(&self, write: &mut W) -> PassiveResult {
        self.size.0.write(write)?;
        self.size.1.write(write)?;

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

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
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
            _ => return Err(Error::invalid("tile description level mode")),
        };

        let rounding_mode = match rounding_mode {
            0 => RoundingMode::Down,
            1 => RoundingMode::Up,
            _ => return Err(Error::invalid("tile description rounding mode")),
        };

        Ok(TileDescription { size: (x_size, y_size), level_mode, rounding_mode, })
    }
}

impl Attribute {
    pub fn byte_size(&self) -> usize {
        self.name.null_terminated_byte_size()
            + self.value.kind_name().len() + sequence_end::byte_size()
            + 0_i32.byte_size() // serialized byte size
            + self.value.byte_size()
    }

    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> PassiveResult {
        self.name.write_null_terminated(write, Some(long_names))?;
        Text::write_null_terminated_bytes(self.value.kind_name(), write, Some(long_names))?;
        (self.value.byte_size() as i32).write(write)?;
        self.value.write(write, long_names)
    }

    // TODO parse lazily, always skip size, ... ?
    pub fn read(read: &mut PeekRead<impl Read>, max_size: usize) -> Result<Self> {
        let name = Text::read_null_terminated(read, max_size)?;
        let kind = Text::read_null_terminated(read, max_size)?;
        let size = i32::read(read)? as u32; // TODO .checked_cast.ok_or(err:negative)
        let value = AnyValue::read(read, kind, size)?;
        Ok(Attribute { name, value, })
    }
}



impl AnyValue {
    pub fn byte_size(&self) -> usize {
        use self::AnyValue::*;

        match *self {
            I32Box2(value) => value.byte_size(),
            F32Box2(value) => value.byte_size(),

            I32(value) => value.byte_size(),
            F32(value) => value.byte_size(),
            F64(value) => value.byte_size(),

            Rational(a, b) => { a.byte_size() + b.byte_size() },
            TimeCode((a, b)) => { a.byte_size() + b.byte_size() },

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
        use self::AnyValue::*;
        use self::attribute_type_names as ty;

        match *self {
            I32Box2(_) =>  ty::I32BOX2,
            F32Box2(_) =>  ty::F32BOX2,
            I32(_) =>  ty::I32,
            F32(_) =>  ty::F32,
            F64(_) =>  ty::F64,
            Rational(_, _) => ty::RATIONAL,
            TimeCode((_, _)) => ty::TIME_CODE,
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

    pub fn write<W: Write>(&self, write: &mut W, long_names: bool) -> PassiveResult {
        use self::AnyValue::*;
        match *self {
            I32Box2(value) => value.write(write)?,
            F32Box2(value) => value.write(write)?,

            I32(value) => value.write(write)?,
            F32(value) => value.write(write)?,
            F64(value) => value.write(write)?,

            Rational(a, b) => { a.write(write)?; b.write(write)?; },
            TimeCode((a, b)) => { a.write(write)?; b.write(write)?; },

            I32Vec2(x, y) => { x.write(write)?; y.write(write)?; },
            F32Vec2(x, y) => { x.write(write)?; y.write(write)?; },
            I32Vec3(x, y, z) => { x.write(write)?; y.write(write)?; z.write(write)?; },
            F32Vec3(x, y, z) => { x.write(write)?; y.write(write)?; z.write(write)?; },

            ChannelList(ref channels) => Channel::write_all(channels, write, long_names)?,
            Chromaticities(ref value) => value.write(write)?,
            Compression(value) => value.write(write)?,
            EnvironmentMap(value) => value.write(write)?,

            KeyCode(value) => value.write(write)?,
            LineOrder(value) => value.write(write)?,

            F32Matrix3x3(mut value) => f32::write_slice(write, &mut value)?,
            F32Matrix4x4(mut value) => f32::write_slice(write, &mut value)?,

            Preview(ref value) => { value.validate()?; value.write(write)?; },

            // attribute value texts never have limited size.
            // also, don't serialize size, as it can be inferred from attribute size
            Text(ref value) => u8::write_slice(write, value.bytes.as_slice())?,

            TextVector(ref value) => self::Text::write_vec_of_i32_sized_texts(write, value)?,
            TileDescription(ref value) => value.write(write)?,
            Custom { ref bytes, .. } => u8::write_slice(write, &bytes)?, // write.write(&bytes).map(|_| ()),
            Kind(kind) => kind.write(write)?
        };

        Ok(())
    }

    pub fn read(read: &mut PeekRead<impl Read>, kind: Text, byte_size: u32) -> Result<Self> {
        use self::AnyValue::*;
        use self::attribute_type_names as ty;

        Ok(match kind.bytes.as_slice() {
            ty::I32BOX2 => I32Box2(self::I32Box2::read(read)?),
            ty::F32BOX2 => F32Box2(self::F32Box2::read(read)?),

            ty::I32 => I32(i32::read(read)?),
            ty::F32 => F32(f32::read(read)?),
            ty::F64 => F64(f64::read(read)?),

            ty::RATIONAL => Rational(i32::read(read)?, u32::read(read)?),
            ty::TIME_CODE => TimeCode((u32::read(read)?, u32::read(read)?)),

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
                f32::read_slice(read, &mut result)?;
                result
            }),

            ty::F32MATRIX4X4 => F32Matrix4x4({
                let mut result = [0.0_f32; 16];
                f32::read_slice(read, &mut result)?;
                result
            }),

            ty::PREVIEW     => Preview(self::Preview::read(read)?),
            ty::TEXT        => Text(self::Text::read_sized(read, byte_size as usize)?),
            ty::TEXT_VECTOR => TextVector(self::Text::read_vec_of_i32_sized(read, byte_size.min(2048))?),
            ty::TILES       => TileDescription(self::TileDescription::read(read)?),

            _ => {
                println!("Unknown attribute type: {:?}", kind.to_string());
                let mut bytes = vec![0_u8; byte_size as usize];
                u8::read_slice(read, &mut bytes)?;
                Custom { kind, bytes }
            }
        })
    }

    pub fn to_tile_description(&self) -> Result<TileDescription> {
        match *self {
            AnyValue::TileDescription(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn to_i32(&self) -> Result<i32> {
        match *self {
            AnyValue::I32(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn to_f32(&self) -> Result<f32> {
        match *self {
            AnyValue::F32(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn to_i32_box_2(&self) -> Result<I32Box2> {
        match *self {
            AnyValue::I32Box2(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn to_f32_vec_2(&self) -> Result<(f32, f32)> {
        match *self {
            AnyValue::F32Vec2(x, y) => Ok((x, y)),
            _ => Err(invalid_type())
        }
    }

    pub fn to_line_order(&self) -> Result<LineOrder> {
        match *self {
            AnyValue::LineOrder(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn to_compression(&self) -> Result<Compression> {
        match *self {
            AnyValue::Compression(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn into_text(self) -> Result<Text> {
        match self {
            AnyValue::Text(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn into_kind(self) -> Result<Kind> {
        match self {
            AnyValue::Kind(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn into_channel_list(self) -> Result<ChannelList> {
        match self {
            AnyValue::ChannelList(value) => Ok(value),
            _ => Err(invalid_type())
        }
    }

    pub fn to_chromaticities(&self) -> Result<Chromaticities> {
        match *self {
            AnyValue::Chromaticities(value) => Ok(value),
            _ => Err(invalid_type())
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
                size: (31, 7),
                level_mode: LevelMode::MipMap,
                rounding_mode: RoundingMode::Down,
            },

            TileDescription {
                size: (0,0),
                level_mode: LevelMode::Singular,
                rounding_mode: RoundingMode::Up,
            },

            TileDescription {
                size: (4294967294, 4294967295),
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
                name: Text::from_str("greeting").unwrap(),
                value: AnyValue::Text(Text::from_str("hello").unwrap()),
            },
            Attribute {
                name: Text::from_str("age").unwrap(),
                value: AnyValue::I32(923),
            },
            Attribute {
                name: Text::from_str("leg count").unwrap(),
                value: AnyValue::F64(9.114939599234),
            },
            Attribute {
                name: Text::from_str("rabbit area").unwrap(),
                value: AnyValue::F32Box2(F32Box2 {
                    x_min: 23.4234,
                    y_min: 345.23,
                    x_max: 68623.0,
                    y_max: 3.12425926538,
                }),
            },
            Attribute {
                name: Text::from_str("tests are difficult").unwrap(),
                value: AnyValue::TextVector(vec![
                    Text::from_str("sdoifjpsdv").unwrap(),
                    Text::from_str("sdoifjpsdvxxxx").unwrap(),
                    Text::from_str("sdoifjasd").unwrap(),
                    Text::from_str("sdoifj").unwrap(),
                    Text::from_str("sdoifjddddddddasdasd").unwrap(),
                ]),
            },
            Attribute {
                name: Text::from_str("what should we eat tonight").unwrap(),
                value: AnyValue::Preview(Preview {
                    width: 10,
                    height: 30,
                    pixel_data: vec![31; 10 * 30 * 4],
                }),
            },
            Attribute {
                name: Text::from_str("leg count, again").unwrap(),
                value: AnyValue::ChannelList(ChannelList {
                    list: smallvec![
                        Channel {
                            name: Text::from_str("Green").unwrap(),
                            pixel_type: PixelType::F16,
                            is_linear: false,
                            reserved: [0, 0, 0],
                            sampling: (1,2)
                        },
                        Channel {
                            name: Text::from_str("Red").unwrap(),
                            pixel_type: PixelType::F32,
                            is_linear: true,
                            reserved: [0, 1, 0],
                            sampling: (1,2)
                        },
                        Channel {
                            name: Text::from_str("Purple").unwrap(),
                            pixel_type: PixelType::U32,
                            is_linear: false,
                            reserved: [1, 2, 7],
                            sampling: (0,0)
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

            let new_attribute = Attribute::read(&mut PeekRead::new(Cursor::new(bytes)), 300).unwrap();
            assert_eq!(*attribute, new_attribute, "attribute round trip");
        }


        {
            let too_large_named = Attribute {
                name: Text::from_str("asdkaspfokpaosdkfpaokswdpoakpsfokaposdkf").unwrap(),
                value: AnyValue::I32(0),
            };

            let mut bytes = Vec::new();
            too_large_named.write(&mut bytes, false).expect_err("name length check failed");
        }

        {
            let way_too_large_named = Attribute {
                name: Text::from_str("sdöksadöofkaspdolkpöasolfkcöalsod,kfcöaslodkcpöasolkfpo").unwrap(),
                value: AnyValue::I32(0),
            };

            let mut bytes = Vec::new();
            way_too_large_named.write(&mut bytes, true).expect_err("name length check failed");
        }
    }
}