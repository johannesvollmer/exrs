
use ::smallvec::SmallVec;

/// null-terminated text strings.
/// max 31 bytes long (if bit 10 is set to 0),
/// or max 255 bytes long (if bit 10 is set to 1).
// TODO non public fields?
/// must be at least 1 byte (to avoid confusion with null-terminators)
#[derive(Clone)]
pub struct Text {
    /// vector does not include null terminator
    pub bytes: SmallVec<[u8; 32]>,
}

impl Text {
    fn unchecked_from_str(str_value: &str) -> Self {
        Text { bytes: SmallVec::from_slice(str_value.as_bytes()) }
    }

    pub fn from_bytes(bytes: SmallVec<[u8; 32]>) -> Self {
        Text { bytes }
    }

    /// panics if value is too long (31 bytes max)
    pub fn short_from_str(str_value: &str) -> Self {
        assert!(str_value.as_bytes().len() < 32, "max text length is 31");
        Self::unchecked_from_str(str_value)
    }

    /// panics if value is too long (31 bytes max)
    pub fn long_from_str(str_value: &str) -> Self {
        assert!(str_value.as_bytes().len() < 256, "max text length is 255");
        Self::unchecked_from_str(str_value)
    }

    pub fn to_string(&self) -> String {
        self.bytes.iter()
            .map(|&byte| byte as char)
            .collect() // TODO is this ascii and can be treated as utf-8?
    }
}

impl ::std::fmt::Debug for Text {
    fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(f, "\"{}\"", self.to_string())
    }
}


#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: Text,
    pub kind: Text,

    /// size in bytes can be inferred from value
    pub value: AttributeValue,
}



#[derive(Debug, Clone)]
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
    Text(ParsedText),

    /// the number of strings can be inferred from the total attribute size
    TextVector(Vec<Text>),

    TileDescription(TileDescription),

    // TODO enable conversion to rust time
    TimeCode(u32, u32),

    I32Vec2(i32, i32),
    F32Vec2(f32, f32),
    I32Vec3(i32, i32, i32),
    F32Vec3(f32, f32, f32),
}


/// this enum parses strings to speed up comparisons
/// based on often-used string contents
#[derive(Debug, Clone)]
pub enum ParsedText {
    /// "scanlineimage"
    ScanLine,

    /// "tiledimage"
    Tile,

    /// "deepscanline"
    DeepScanLine,

    /// "deeptile"
    DeepTile,

    Arbitrary(Text),
}

impl ParsedText {
    pub fn parse(text: Text) -> Self {
        match text.bytes.as_slice() {
            b"scanlineimage" => ParsedText::ScanLine,
            b"tiledimage" => ParsedText::Tile,
            b"deepscanline" => ParsedText::DeepScanLine,
            b"deeptile" => ParsedText::DeepTile,
            _ => ParsedText::Arbitrary(text),
        }
    }
}


#[derive(Debug, Clone, Copy)]
pub struct I32Box2 {
    pub x_min: i32, pub y_min: i32,
    pub x_max: i32, pub y_max: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct F32Box2 {
    pub x_min: f32, pub y_min: f32,
    pub x_max: f32, pub y_max: f32,
}

/// followed by a null byte
/// sorted alphabetically?
pub type ChannelList = SmallVec<[Channel; 5]>;

#[derive(Debug, Clone)]
pub struct Channel {
    /// zero terminated, 1 to 255 bytes
    pub name: Text,

    /// int
    pub pixel_type: PixelType,

    pub is_linear: bool,

    /// three signed chars, should be zero
    pub reserved: [i8; 3],

    /// can be used for chroma-subsampling
    pub x_sampling: i32,

    /// can be used for chroma-subsampling
    pub y_sampling: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum PixelType {
    U32, F16, F32,
}

#[derive(Debug, Clone, Copy)]
pub struct Chromaticities {
    pub red_x: f32,     pub red_y: f32,
    pub green_x: f32,   pub green_y: f32,
    pub blue_x: f32,    pub blue_y: f32,
    pub white_x: f32,   pub white_y: f32
}

#[derive(Debug, Clone, Copy)]
pub enum EnvironmentMap {
    LatitudeLongitude,
    Cube,
}

/// uniquely identifies a motion picture film frame
#[derive(Debug, Clone, Copy)]
pub struct KeyCode {
    pub film_manufacturer_code: i32,
    pub film_type: i32,
    pub film_roll_prefix: i32,

    pub count: i32,

    pub perforation_offset: i32,
    pub perforations_per_frame: i32,
    pub perforations_per_count: i32,
}

#[derive(Debug, Clone, Copy)]
pub enum LineOrder {
    IncreasingY,
    DecreasingY,
    RandomY,
}

#[derive(Debug, Clone)]
pub struct Preview {
    pub width: u32,
    pub height: u32,

    /// 4 × width × height bytes,
    /// Scan lines are stored top to bottom; within a scan line pixels are stored from left
    /// to right. A pixel consists of four unsigned chars, R, G, B, A
    pub pixel_data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct TileDescription {
    pub x_size: u32, pub y_size: u32,
    pub level_mode: LevelMode,
    pub rounding_mode: RoundingMode,
}

#[derive(Debug, Clone, Copy)]
pub enum LevelMode {
    One, MipMap, RipMap,
}

#[derive(Debug, Clone, Copy)]
pub enum RoundingMode {
    Down, Up,
}

#[derive(Debug, Clone, Copy)]
pub enum Compression {
    None, RLE, ZIPSingle,
    ZIP, PIZ, PXR24,
    B44, B44A,
}

impl AttributeValue {
    pub fn get_byte_size(&self) -> usize {
//        use self::AttributeValue::*;
        match *self {
            _ => unimplemented!()
        }
    }

    pub fn to_tile_description(&self) -> Option<TileDescription> {
        match *self {
            AttributeValue::TileDescription(tile) => Some(tile),
            _ => None,
        }
    }

    pub fn to_i32_box_2(&self) -> Option<I32Box2> {
        match *self {
            AttributeValue::I32Box2(ibox) => Some(ibox),
            _ => None,
        }
    }

    pub fn to_compression(&self) -> Option<Compression> {
        match *self {
            AttributeValue::Compression(compr) => Some(compr),
            _ => None,
        }
    }

    pub fn to_text(&self) -> Option<&ParsedText> {
        match *self {
            AttributeValue::Text(ref t) => Some(t),
            _ => None,
        }
    }
}