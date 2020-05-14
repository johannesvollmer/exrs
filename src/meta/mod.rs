
//! Describes all meta data possible in an exr file.

pub mod attributes;


use crate::io::*;
use ::smallvec::SmallVec;
use self::attributes::*;
use crate::chunk::{TileCoordinates, Block};
use crate::error::*;
use std::fs::File;
use std::io::{BufReader};
use crate::math::*;
use std::collections::{HashSet, HashMap};
use std::convert::TryFrom;
use smallvec::alloc::fmt::Formatter;


/// Contains the complete meta data of an exr image.
/// Defines how the image is split up in the file,
/// the number and type of images and channels,
/// and various other attributes.
/// The usage of custom attributes is encouraged.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaData {

    /// Some flags summarizing the features that must be supported to decode the file.
    pub requirements: Requirements,

    /// One header to describe each layer in this file.
    pub headers: Headers,
}


/// List of `Header`s.
pub type Headers = SmallVec<[Header; 3]>;

/// List of `OffsetTable`s.
pub type OffsetTables = SmallVec<[OffsetTable; 3]>;


/// The offset table is an ordered list of indices referencing pixel data in the exr file.
/// For each pixel tile in the image, an index exists, which points to the byte-location
/// of the corresponding pixel data in the file. That index can be used to load specific
/// portions of an image without processing all bytes in a file. For each header,
/// an offset table exists with its indices ordered by `LineOrder::Increasing`.
// If the multipart bit is unset and the chunkCount attribute is not present,
// the number of entries in the chunk table is computed using the
// dataWindow, tileDesc, and compression attribute.
//
// If the multipart bit is set, the header must contain a
// chunkCount attribute, that contains the length of the offset table.
pub type OffsetTable = Vec<u64>;

/// Describes a single layer in a file.
/// A file can have any number of layers.
/// The meta data contains one header per layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Header {

    /// List of channels in this layer.
    pub channels: ChannelList,

    /// How the pixel data of all channels in this layer is compressed. May be `Compression::Uncompressed`.
    pub compression: Compression,

    /// Describes how the pixels of this layer are divided into smaller blocks.
    /// A single block can be loaded without processing all bytes of a file.
    ///
    /// Also describes whether a file contains multiple resolution levels: mip maps or rip maps.
    /// This allows loading not the full resolution, but the smallest sensible resolution.
    //
    // Required if file contains deep data or multiple layers.
    // Note: This value must agree with the version field's tile bit and deep data bit.
    // In this crate, this attribute will always have a value, for simplicity.
    pub blocks: Blocks,

    /// In what order the tiles of this header occur in the file.
    pub line_order: LineOrder,

    /// The resolution of this layer. Equals the size of the data window.
    pub data_size: Vec2<usize>,

    /// Whether this layer contains deep data.
    pub deep: bool,

    /// This library supports only deep data version 1.
    pub deep_data_version: Option<i32>,

    /// Number of chunks, that is, scan line blocks or tiles, that this image has been divided into.
    /// This number is calculated once at the beginning
    /// of the read process or when creating a header object.
    ///
    /// This value includes all chunks of all resolution levels.
    ///
    ///
    /// __Warning__
    /// _This value is relied upon. You should probably use `Header::with_encoding`,
    /// which automatically updates the chunk count._
    pub chunk_count: usize,

    // Required for deep data (deepscanline and deeptile) layers.
    // Note: Since the value of "maxSamplesPerPixel"
    // maybe be unknown at the time of opening the
    // file, the value “ -1 ” is written to the file to
    // indicate an unknown value. When the file is
    // closed, this will be overwritten with the correct value.
    // If file writing does not complete
    // correctly due to an error, the value -1 will
    // remain. In this case, the value must be derived
    // by decoding each chunk in the layer
    /// Maximum number of samples in a single pixel in a deep image.
    pub max_samples_per_pixel: Option<usize>,

    /// Includes mandatory fields like pixel aspect or display window
    /// which must be the same for all layers.
    pub shared_attributes: ImageAttributes,

    /// Does not include the attributes required for reading the file contents.
    /// Excludes standard fields that must be the same for all headers.
    pub own_attributes: LayerAttributes,
}


/// Includes mandatory fields like pixel aspect or display window
/// which must be the same for all layers.
/// For more attributes, see struct `LayerAttributes`.
#[derive(Clone, PartialEq, Debug)]
pub struct ImageAttributes {

    /// The rectangle anywhere in the global infinite 2D space
    /// that clips all contents of the file.
    pub display_window: IntRect,

    /// Aspect ratio of each pixel in this header.
    pub pixel_aspect: f32,

    /// The chromaticities attribute of the image. See the `Chromaticities` type.
    pub chromaticities: Option<Chromaticities>,

    /// The time code of the image.
    pub time_code: Option<TimeCode>,

    /// Optional attributes. Contains custom attributes.
    /// Does not contain the attributes already present in the `ImageAttributes`.
    /// Contains only attributes that are standardized to be the same for all headers: chromaticities and time codes.
    pub custom: HashMap<Text, AttributeValue>,
}

/// Does not include the attributes required for reading the file contents.
/// Excludes standard fields that must be the same for all headers.
/// For more attributes, see struct `ImageAttributes`.
#[derive(Clone, PartialEq)]
pub struct LayerAttributes {

    /// The name of this layer.
    /// Required if this file contains deep data or multiple layers.
    // As this is an attribute value, it is not restricted in length, may even be empty
    pub name: Option<Text>,

    /// The bottom left corner of the rectangle that positions this layer
    /// within the global infinite 2D space of the whole file.
    /// Equals the position of the data window.
    pub data_position: Vec2<i32>,

    /// Part of the perspective projection. Default should be `(0, 0)`.
    // TODO same for all layers?
    pub screen_window_center: Vec2<f32>, // TODO integrate into `list`

    // TODO same for all layers?
    /// Part of the perspective projection. Default should be `1`.
    pub screen_window_width: f32, // TODO integrate into `list`

    /// The white luminance of the colors.
    /// Defines the luminance in candelas per square meter, Nits, of the RGB value `(1, 1, 1)`.
    // If the chromaticities and the whiteLuminance of an RGB image are
    // known, then it is possible to convert the image's pixels from RGB
    // to CIE XYZ tristimulus values (see function RGBtoXYZ() in header
    // file ImfChromaticities.h).
    pub white_luminance: Option<f32>,

    /// The adopted neutral of the colors. Specifies the CIE (x,y) frequency coordinates that should
    /// be considered neutral during color rendering. Pixels in the image
    /// whose CIE (x,y) frequency coordinates match the adopted neutral value should
    /// be mapped to neutral values on the given display.
    pub adopted_neutral: Option<Vec2<f32>>,

    /// Name of the color transform function that is applied for rendering the image.
    pub rendering_transform: Option<Text>,

    /// Name of the color transform function that computes the look modification of the image.
    pub look_modification_transform: Option<Text>,

    /// The horizontal density, in pixels per inch.
    /// The image's vertical output density can be computed using `x_density * pixel_aspect_ratio`.
    pub x_density: Option<f32>,

    /// Name of the owner.
    pub owner: Option<Text>,

    /// Additional textual information.
    pub comments: Option<Text>,

    /// The date of image creation, in `YYYY:MM:DD hh:mm:ss` format.
    // TODO parse!
    pub capture_date: Option<Text>,

    /// Time offset from UTC.
    pub utc_offset: Option<f32>,

    /// Geographical image location.
    pub longitude: Option<f32>,

    /// Geographical image location.
    pub latitude: Option<f32>,

    /// Geographical image location.
    pub altitude: Option<f32>,

    /// Camera focus in meters.
    pub focus: Option<f32>,

    /// Exposure time in seconds.
    pub exposure: Option<f32>,

    /// Camera aperture measured in f-stops. Equals the focal length
    /// of the lens divided by the diameter of the iris opening.
    pub aperture: Option<f32>,

    /// Iso-speed of the camera sensor.
    pub iso_speed: Option<f32>,

    /// If this is an environment map, specifies how to interpret it.
    pub environment_map: Option<EnvironmentMap>,

    /// Identifies film manufacturer, film type, film roll and frame position within the roll.
    pub key_code: Option<KeyCode>,

    /// Specifies how texture map images are extrapolated.
    /// Values can be `black`, `clamp`, `periodic`, or `mirror`.
    pub wrap_modes: Option<Text>,

    /// Frames per second if this is a frame in a sequence.
    pub frames_per_second: Option<Rational>,

    /// Specifies the view names for multi-view, for example stereo, image files.
    pub multi_view: Option<Vec<Text>>,

    /// The matrix that transforms 3D points from the world to the camera coordinate space.
    /// Left-handed coordinate system, y up, z forward.
    pub world_to_camera: Option<Matrix4x4>,

    /// The matrix that transforms 3D points from the world to the "Normalized Device Coordinate" space.
    /// Left-handed coordinate system, y up, z forward.
    pub world_to_normalized_device: Option<Matrix4x4>,

    /// Specifies whether the pixels in a deep image are sorted and non-overlapping.
    pub deep_image_state: Option<Rational>,

    /// If the image was cropped, contains the original data window.
    pub original_data_window: Option<IntRect>,

    /// Level of compression in DWA images.
    pub dwa_compression_level: Option<f32>,

    /// An 8-bit RGBA image representing the rendered image.
    pub preview: Option<Preview>,

    /// Name of the view, which is probably either `"right"` or `"left"` for a stereoscopic image.
    pub view: Option<Text>,

    /// Optional attributes. Contains custom attributes.
    /// Does not contain the attributes already present in the `Header` or `LayerAttributes` struct.
    /// Does not contain attributes that are standardized to be the same for all layers: no chromaticities and no time codes.
    pub custom: HashMap<Text, AttributeValue>,
}

/// A summary of requirements that must be met to read this exr file.
/// Used to determine whether this file can be read by a given reader.
/// It includes the OpenEXR version number. This library aims to support version `2.0`.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Requirements {

    /// This library supports reading version 1 and 2, and writing version 2.
    // TODO write version 1 for simple images
    file_format_version: u8,

    /// If true, this image has tiled blocks and contains only a single layer.
    /// If false and not deep and not multilayer, this image is a single layer image with scan line blocks.
    is_single_layer_and_tiled: bool,

    // in c or bad c++ this might have been relevant (omg is he allowed to say that)
    /// Whether this file has strings with a length greater than 31.
    /// Strings can never be longer than 255.
    has_long_names: bool,

    /// This image contains at least one layer with deep data.
    has_deep_data: bool,

    /// Whether this file contains multiple layers.
    has_multiple_layers: bool,
}


/// Locates a rectangular section of pixels in an image.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct TileIndices {

    /// Index of the tile.
    pub location: TileCoordinates,

    /// Pixel size of the tile.
    pub size: Vec2<usize>,
}

/// How the image pixels are split up into separate blocks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Blocks {

    /// The image is divided into scan line blocks.
    /// The number of scan lines in a block depends on the compression method.
    ScanLines,

    /// The image is divided into tile blocks.
    /// Also specifies the size of each tile in the image
    /// and whether this image contains multiple resolution levels.
    Tiles(TileDescription)
}


/*impl TileIndices {
    pub fn cmp(&self, other: &Self) -> Ordering {
        match self.location.level_index.1.cmp(&other.location.level_index.1) {
            Ordering::Equal => {
                match self.location.level_index.0.cmp(&other.location.level_index.0) {
                    Ordering::Equal => {
                        match self.location.tile_index.1.cmp(&other.location.tile_index.1) {
                            Ordering::Equal => {
                                self.location.tile_index.0.cmp(&other.location.tile_index.0)
                            },

                            other => other,
                        }
                    },

                    other => other
                }
            },

            other => other
        }
    }
}*/

impl Blocks {

    /// Whether this image is tiled. If false, this image is divided into scan line blocks.
    pub fn has_tiles(&self) -> bool {
        match self {
            Blocks::Tiles { .. } => true,
            _ => false
        }
    }
}



impl LayerAttributes {

    /// Create default layer attributes with a data position of zero.
    pub fn new(layer_name: Text) -> Self {
        Self {
            name: Some(layer_name),
            .. Self::default()
        }
    }

    /// Set the data position of this layer.
    pub fn with_position(self, data_position: Vec2<i32>) -> Self {
        Self { data_position, ..self }
    }
}

impl ImageAttributes {

    /// Create default image attributes with the specified display window size.
    /// The display window position is set to zero.
    pub fn new(display_size: impl Into<Vec2<usize>>) -> Self {
        Self {
            display_window: IntRect::from_dimensions(display_size),
            .. Self::default()
        }
    }

    /// Set the data position of this layer.
    pub fn with_display_window(self, display_window: IntRect) -> Self {
        Self { display_window, ..self }
    }
}


/// The first four bytes of each exr file.
/// Used to abort reading non-exr files.
pub mod magic_number {
    use super::*;

    /// The first four bytes of each exr file.
    pub const BYTES: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];

    /// Without validation, write this instance to the byte stream.
    pub fn write(write: &mut impl Write) -> Result<()> {
        u8::write_slice(write, &self::BYTES)
    }

    /// Consumes four bytes from the reader and returns whether the file may be an exr file.
    // TODO check if exr before allocating BufRead
    pub fn is_exr(read: &mut impl Read) -> Result<bool> {
        let mut magic_num = [0; 4];
        u8::read_slice(read, &mut magic_num)?;
        Ok(magic_num == self::BYTES)
    }

    /// Validate this image. If it is an exr file, return `Ok(())`.
    pub fn validate_exr(read: &mut impl Read) -> UnitResult {
        if self::is_exr(read)? {
            Ok(())

        } else {
            Err(Error::invalid("file identifier missing"))
        }
    }
}

/// A `0_u8` at the end of a sequence.
pub mod sequence_end {
    use super::*;

    /// Number of bytes this would consume in an exr file.
    pub fn byte_size() -> usize {
        1
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(write: &mut W) -> UnitResult {
        0_u8.write(write)
    }

    /// Peeks the next byte. If it is zero, consumes the byte and returns true.
    pub fn has_come(read: &mut PeekRead<impl Read>) -> Result<bool> {
        Ok(read.skip_if_eq(0)?)
    }
}

fn missing_attribute(name: &str) -> Error {
    Error::invalid(format!("missing or invalid {} attribute", name))
}


/// Compute the number of tiles required to contain all values.
pub fn compute_block_count(full_res: usize, tile_size: usize) -> usize {
    // round up, because if the image is not evenly divisible by the tiles,
    // we add another tile at the end (which is only partially used)
    RoundingMode::Up.divide(full_res, tile_size)
}

/// Compute the start position and size of a block inside a dimension.
#[inline]
pub fn calculate_block_position_and_size(total_size: usize, block_size: usize, block_index: usize) -> Result<(usize, usize)> {
    let block_position = block_size * block_index;

    Ok((
        block_position,
        calculate_block_size(total_size, block_size, block_position)?
    ))
}

/// Calculate the size of a single block. If this is the last block,
/// this only returns the required size, which is always smaller than the default block size.
// TODO use this method everywhere instead of convoluted formulas
#[inline]
pub fn calculate_block_size(total_size: usize, block_size: usize, block_position: usize) -> Result<usize> {
    if block_position >= total_size {
        return Err(Error::invalid("block index"))
    }

    if block_position + block_size <= total_size {
        Ok(block_size)
    }
    else {
        Ok(total_size - block_position)
    }
}


/// Calculate number of mip levels in a given resolution.
// TODO this should be cached? log2 may be very expensive
pub fn compute_level_count(round: RoundingMode, full_res: usize) -> usize {
    round.log2(full_res) + 1
}

/// Calculate the size of a single mip level by index.
// TODO this should be cached? log2 may be very expensive
pub fn compute_level_size(round: RoundingMode, full_res: usize, level_index: usize) -> usize {
    assert!(level_index < std::mem::size_of::<usize>() * 8, "largest level size exceeds maximum integer value");
    round.divide(full_res,  1 << level_index).max(1)
}

/// Iterates over all rip map level resolutions of a given size, including the indices of each level.
/// The order of iteration conforms to `LineOrder::Increasing`.
// TODO cache these?
// TODO compute these directly instead of summing up an iterator?
pub fn rip_map_levels(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=(Vec2<usize>, Vec2<usize>)> {
    rip_map_indices(round, max_resolution).map(move |level_indices|{
        // TODO progressively divide instead??
        let width = compute_level_size(round, max_resolution.width(), level_indices.x());
        let height = compute_level_size(round, max_resolution.height(), level_indices.y());
        (level_indices, Vec2(width, height))
    })
}

/// Iterates over all mip map level resolutions of a given size, including the indices of each level.
/// The order of iteration conforms to `LineOrder::Increasing`.
// TODO cache all these level values when computing table offset size??
// TODO compute these directly instead of summing up an iterator?
pub fn mip_map_levels(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=(usize, Vec2<usize>)> {
    mip_map_indices(round, max_resolution)
        .map(move |level_index|{
            // TODO progressively divide instead??
            let width = compute_level_size(round, max_resolution.width(), level_index);
            let height = compute_level_size(round, max_resolution.height(), level_index);
            (level_index, Vec2(width, height))
        })
}

/// Iterates over all rip map level indices of a given size.
/// The order of iteration conforms to `LineOrder::Increasing`.
pub fn rip_map_indices(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=Vec2<usize>> {
    let (width, height) = (
        compute_level_count(round, max_resolution.width()),
        compute_level_count(round, max_resolution.height())
    );

    (0..height).flat_map(move |y_level|{
        (0..width).map(move |x_level|{
            Vec2(x_level, y_level)
        })
    })
}

/// Iterates over all mip map level indices of a given size.
/// The order of iteration conforms to `LineOrder::Increasing`.
pub fn mip_map_indices(round: RoundingMode, max_resolution: Vec2<usize>) -> impl Iterator<Item=usize> {
    (0..compute_level_count(round, max_resolution.width().max(max_resolution.height())))
}

/// Compute the number of chunks that an image is divided into. May be an expensive operation.
// If not multilayer and chunkCount not present,
// the number of entries in the chunk table is computed
// using the dataWindow and tileDesc attributes and the compression format
pub fn compute_chunk_count(compression: Compression, data_size: Vec2<usize>, blocks: Blocks) -> usize {

    if let Blocks::Tiles(tiles) = blocks {
        let round = tiles.rounding_mode;
        let Vec2(tile_width, tile_height) = tiles.tile_size;

        // TODO cache all these level values??
        use crate::meta::attributes::LevelMode::*;
        match tiles.level_mode {
            Singular => {
                let tiles_x = compute_block_count(data_size.width(), tile_width);
                let tiles_y = compute_block_count(data_size.height(), tile_height);
                tiles_x * tiles_y
            }

            MipMap => {
                mip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_block_count(level_width, tile_width) * compute_block_count(level_height, tile_height)
                }).sum()
            },

            RipMap => {
                rip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_block_count(level_width, tile_width) * compute_block_count(level_height, tile_height)
                }).sum()
            }
        }
    }

    // scan line blocks never have mip maps
    else {
        compute_block_count(data_size.height(), compression.scan_lines_per_block())
    }
}



impl MetaData {

    /// Infers version requirements from headers.
    pub fn new(headers: Headers) -> Self {
        MetaData {
            requirements: Requirements::infer(headers.as_slice()),
            headers
        }
    }

    /// Read the exr meta data from a file.
    /// Use `read_from_unbuffered` instead if you do not have a file.
    /// Does not validate the meta data.
    #[must_use]
    pub fn read_from_file(path: impl AsRef<::std::path::Path>, skip_invalid_attributes: bool) -> Result<Self> {
        Self::read_from_unbuffered(File::open(path)?, skip_invalid_attributes)
    }

    /// Buffer the reader and then read the exr meta data from it.
    /// Use `read_from_buffered` if your reader is an in-memory reader.
    /// Use `read_from_file` if you have a file path.
    /// Does not validate the meta data.
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read, skip_invalid_attributes: bool) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered), skip_invalid_attributes)
    }

    /// Read the exr meta data from a reader.
    /// Use `read_from_file` if you have a file path.
    /// Use `read_from_unbuffered` if this is not an in-memory reader.
    /// Does not validate the meta data.
    #[must_use]
    pub fn read_from_buffered(buffered: impl Read, skip_invalid_attributes: bool) -> Result<Self> {
        let mut read = PeekRead::new(buffered);
        MetaData::read_unvalidated_from_buffered_peekable(&mut read, skip_invalid_attributes)
    }

    /// Does __not validate__ the meta data.
    #[must_use]
    pub(crate) fn read_unvalidated_from_buffered_peekable(read: &mut PeekRead<impl Read>, skip_invalid_attributes: bool) -> Result<Self> {
        magic_number::validate_exr(read)?;
        let requirements = Requirements::read(read)?;
        let headers = Header::read_all(read, &requirements, skip_invalid_attributes)?;

        // TODO check if supporting requirements 2 always implies supporting requirements 1
        Ok(MetaData { requirements, headers })
    }

    /// Validates the meta data.
    #[must_use]
    pub(crate) fn read_from_buffered_peekable(read: &mut PeekRead<impl Read>, max_pixel_bytes: Option<usize>, skip_invalid_attributes: bool) -> Result<Self> {
        let meta_data = Self::read_unvalidated_from_buffered_peekable(read, skip_invalid_attributes)?;

        // relaxed validation to allow slightly invalid files
        // that still can be read correctly
        meta_data.validate(max_pixel_bytes, false)?;

        Ok(meta_data)
    }

    /// Validates the meta data and writes it to the stream.
    /// If pedantic, throws errors for files that may produce errors in other exr readers.
    pub(crate) fn write_validating_to_buffered(&self, write: &mut impl Write, pedantic: bool) -> UnitResult {
        // pedantic validation to not allow slightly invalid files
        // that still could be read correctly in theory
        self.validate(None, pedantic)?;

        magic_number::write(write)?;
        self.requirements.write(write)?;
        Header::write_all(self.headers.as_slice(), write, self.requirements.has_multiple_layers)?;
        Ok(())
    }

    /// Read one offset table from the reader for each header.
    pub fn read_offset_tables(read: &mut PeekRead<impl Read>, headers: &Headers) -> Result<OffsetTables> {
        headers.iter()
            .map(|header| u64::read_vec(read, header.chunk_count, std::u16::MAX as usize, None))
            .collect()
    }

    /// Skip the offset tables by advancing the reader by the required byte count.
    // TODO use seek for large (probably all) tables!
    pub fn skip_offset_tables(read: &mut PeekRead<impl Read>, headers: &Headers) -> Result<usize> {
        let chunk_count: usize = headers.iter().map(|header| header.chunk_count).sum();
        crate::io::skip_bytes(read, chunk_count * u64::BYTE_SIZE)?; // TODO this should seek for large tables
        Ok(chunk_count)
    }

    /// Validates this meta data.
    /// Set strict to false when reading and true when writing for maximum compatibility.
    pub fn validate(&self, max_pixel_bytes: Option<usize>, strict: bool) -> UnitResult {
        self.requirements.validate()?;

        let headers = self.headers.len();

        if headers == 0 {
            return Err(Error::invalid("at least one layer is required"));
        }

        for header in &self.headers {
            header.validate(&self.requirements, strict)?;
        }

        if let Some(max) = max_pixel_bytes {
            let byte_size: usize = self.headers.iter()
                .map(|header| header.data_size.area() * header.channels.bytes_per_pixel)
                .sum();

            if byte_size > max {
                return Err(Error::invalid("image larger than specified maximum"));
            }
        }

        if strict { // check for duplicate header names
            let mut header_names = HashSet::with_capacity(headers);
            for header in &self.headers {
                if !header_names.insert(&header.own_attributes.name) {
                    return Err(Error::invalid(format!(
                        "duplicate layer name: `{}`",
                        header.own_attributes.name.as_ref().expect("header validation bug")
                    )));
                }
            }
        }

        if strict {
            let must_share = self.headers.iter().flat_map(|header| header.own_attributes.custom.iter())
                .any(|(_, value)| value.to_chromaticities().is_ok() || value.to_time_code().is_ok());

            if must_share {
                return Err(Error::invalid("chromaticities and time code attributes must must not exist in own attributes but shared instead"));
            }
        }

        if strict && headers > 1 { // check for attributes that should not differ in between headers
            let first_header = self.headers.first().expect("header count validation bug");
            let first_header_attributes = &first_header.shared_attributes.custom;

            for header in &self.headers[1..] {
                let attributes = &header.shared_attributes.custom;
                if attributes != first_header_attributes
                    || header.shared_attributes.display_window != first_header.shared_attributes.display_window
                    || header.shared_attributes.pixel_aspect != first_header.shared_attributes.pixel_aspect
                {
                    return Err(Error::invalid("display window, pixel aspect, chromaticities, and time code attributes must be equal for all headers"))
                }
            }
        }

        if self.requirements.file_format_version == 1 || !self.requirements.has_multiple_layers {
            if headers != 1 {
                return Err(Error::invalid("multipart flag for header count"));
            }
        }

        Ok(())
    }
}



impl Header {

    /// Create a new Header with the specified name, display window and channels.
    /// Use `Header::with_encoding` and the similar methods to add further properties to the header.
    ///
    /// The other settings are left to their default values:
    /// - no compression
    /// - display window equal to data window
    /// - scan line blocks
    /// - unspecified line order
    /// - no custom attributes
    pub fn new(name: Text, data_size: impl Into<Vec2<usize>>, channels: SmallVec<[Channel; 5]>) -> Self {
        let data_size: Vec2<usize> = data_size.into();
        let compression = Compression::Uncompressed;
        let blocks = Blocks::ScanLines;

        Self {
            data_size,
            compression,
            blocks,

            channels: ChannelList::new(channels),
            line_order: LineOrder::Unspecified,

            shared_attributes: ImageAttributes::new(data_size),
            own_attributes: LayerAttributes::new(name),

            chunk_count: compute_chunk_count(compression, data_size, blocks),

            deep: false,
            deep_data_version: None,
            max_samples_per_pixel: None,
        }
    }

    /// Set the display window, that is, the global clipping rectangle.
    /// __Must be the same for all headers of a file.__
    pub fn with_display_window(mut self, display_window: IntRect) -> Self {
        self.shared_attributes.display_window = display_window;
        self
    }

    /// Set the offset of this layer.
    pub fn with_position(mut self, position: Vec2<i32>) -> Self {
        self.own_attributes.data_position = position;
        self
    }

    /// Set compression, tiling, and line order. Automatically computes chunk count.
    pub fn with_encoding(self, compression: Compression, blocks: Blocks, line_order: LineOrder) -> Self {
        Self {
            chunk_count: compute_chunk_count(compression, self.data_size, blocks),
            compression, blocks, line_order,
            .. self
        }
    }

    /// Add some custom attributes to the header that are not shared with all other headers in the image.
    pub fn with_attributes(self, own_attributes: LayerAttributes) -> Self {
        Self { own_attributes, .. self }
    }

    /// Add some custom attributes to the header that are shared with all other headers in the image.
    pub fn with_shared_attributes(self, shared_attributes: ImageAttributes) -> Self {
        Self { shared_attributes, .. self }
    }

    /// Iterate over all blocks, in the order specified by the headers line order attribute,
    /// with an index returning the original index of the block if it were `LineOrder::Increasing`.
    pub fn enumerate_ordered_blocks(&self) -> impl Iterator<Item = (usize, TileIndices)> + Send {
        let increasing_y = self.blocks_increasing_y_order().enumerate();

        let ordered: Box<dyn Send + Iterator<Item = (usize, TileIndices)>> = {
            if self.line_order == LineOrder::Decreasing {
                Box::new(increasing_y.rev()) // TODO without box?
            }
            else {
                Box::new(increasing_y)
            }
        };

        ordered
    }

    /// Iterate over all tile indices in this header in `LineOrder::Increasing` order.
    pub fn blocks_increasing_y_order(&self) -> impl Iterator<Item = TileIndices> + ExactSizeIterator + DoubleEndedIterator {
        fn tiles_of(image_size: Vec2<usize>, tile_size: Vec2<usize>, level_index: Vec2<usize>) -> impl Iterator<Item=TileIndices> {
            fn divide_and_rest(total_size: usize, block_size: usize) -> impl Iterator<Item=(usize, usize)> {
                let block_count = compute_block_count(total_size, block_size);
                (0..block_count).map(move |block_index| (
                    block_index, calculate_block_size(total_size, block_size, block_index).expect("block size calculation bug")
                ))
            }

            divide_and_rest(image_size.height(), tile_size.height()).flat_map(move |(y_index, tile_height)|{
                divide_and_rest(image_size.width(), tile_size.width()).map(move |(x_index, tile_width)|{
                    TileIndices {
                        size: Vec2(tile_width, tile_height),
                        location: TileCoordinates { tile_index: Vec2(x_index, y_index), level_index, },
                    }
                })
            })
        }

        let vec: Vec<TileIndices> = {
            if let Blocks::Tiles(tiles) = self.blocks {
                match tiles.level_mode {
                    LevelMode::Singular => {
                        tiles_of(self.data_size, tiles.tile_size, Vec2(0, 0)).collect()
                    },
                    LevelMode::MipMap => {
                        mip_map_levels(tiles.rounding_mode, self.data_size)
                            .flat_map(move |(level_index, level_size)|{
                                tiles_of(level_size, tiles.tile_size, Vec2(level_index, level_index))
                            })
                            .collect()
                    },
                    LevelMode::RipMap => {
                        rip_map_levels(tiles.rounding_mode, self.data_size)
                            .flat_map(move |(level_index, level_size)| {
                                tiles_of(level_size, tiles.tile_size, level_index)
                            })
                            .collect()
                    }
                }
            }
            else {
                let tiles = Vec2(self.data_size.0, self.compression.scan_lines_per_block());
                tiles_of(self.data_size, tiles, Vec2(0,0)).collect()
            }
        };

        vec.into_iter() // TODO without collect
    }

    /// The dimensions, in pixels, of every block in this image.
    /// The default block size may be deviated from in the last column or row of an image.
    /// Those blocks only have the size necessary to include all pixels of the image,
    /// which may be smaller than the default block size.
    // TODO reuse this function everywhere
    pub fn default_block_pixel_size(&self) -> Vec2<usize> {
        match self.blocks {
            Blocks::ScanLines => Vec2(self.data_size.0, self.compression.scan_lines_per_block()),
            Blocks::Tiles(tiles) => tiles.tile_size,
        }
    }

    /// Calculate the position of a block in the global infinite 2D space of a file. May be negative.
    pub fn get_block_data_window_pixel_coordinates(&self, tile: TileCoordinates) -> Result<IntRect> {
        let data = self.get_absolute_block_pixel_coordinates(tile)?;
        Ok(data.with_origin(self.own_attributes.data_position))
    }

    /// Calculate the pixel index rectangle inside this header. Is not negative. Starts at `0`.
    pub fn get_absolute_block_pixel_coordinates(&self, tile: TileCoordinates) -> Result<IntRect> {
        if let Blocks::Tiles(tiles) = self.blocks {
            let Vec2(data_width, data_height) = self.data_size;

            let data_width = compute_level_size(tiles.rounding_mode, data_width, tile.level_index.x());
            let data_height = compute_level_size(tiles.rounding_mode, data_height, tile.level_index.y());
            let absolute_tile_coordinates = tile.to_data_indices(tiles.tile_size, Vec2(data_width, data_height))?;

            if absolute_tile_coordinates.position.x() as i64 >= data_width as i64 || absolute_tile_coordinates.position.y() as i64 >= data_height as i64 {
                return Err(Error::invalid("data block tile index"))
            }

            Ok(absolute_tile_coordinates)
        }
        else { // this is a scanline image
            debug_assert_eq!(tile.tile_index.0, 0, "block index calculation bug");

            let (y, height) = calculate_block_position_and_size(
                self.data_size.height(),
                self.compression.scan_lines_per_block(),
                tile.tile_index.y()
            )?;

            Ok(IntRect {
                position: Vec2(0, usize_to_i32(y)),
                size: Vec2(self.data_size.width(), height)
            })
        }

        // TODO deep data?
    }

    /// Return the tile index, converting scan line block coordinates to tile indices.
    /// Starts at `0` and is not negative.
    pub fn get_block_data_indices(&self, block: &Block) -> Result<TileCoordinates> {
        Ok(match block {
            Block::Tile(ref tile) => {
                tile.coordinates
            },

            Block::ScanLine(ref block) => {
                let size = self.compression.scan_lines_per_block() as i32;
                let y = (block.y_coordinate - self.own_attributes.data_position.y()) / size;

                if y < 0 {
                    return Err(Error::invalid("scan block y coordinate"));
                }

                TileCoordinates {
                    tile_index: Vec2(0, y as usize),
                    level_index: Vec2(0, 0)
                }
            },

            _ => return Err(Error::unsupported("deep data not supported yet"))
        })
    }

    /// Computes the absolute tile coordinate data indices, which start at `0`.
    pub fn get_scan_line_block_tile_coordinates(&self, block_y_coordinate: i32) -> Result<TileCoordinates> {
        let size = self.compression.scan_lines_per_block() as i32;
        let y = (block_y_coordinate - self.own_attributes.data_position.1) / size;
        
        if y < 0 {
            return Err(Error::invalid("scan block y coordinate"));
        }

        Ok(TileCoordinates {
            tile_index: Vec2(0, y as usize),
            level_index: Vec2(0, 0)
        })
    }

    /// Maximum byte length of an uncompressed or compressed block, used for validation.
    pub fn max_block_byte_size(&self) -> usize {
        self.channels.bytes_per_pixel * match self.blocks {
            Blocks::Tiles(tiles) => tiles.tile_size.area(),
            Blocks::ScanLines => self.compression.scan_lines_per_block() * self.data_size.width()
            // TODO What about deep data???
        }
    }

    /// Validate this instance.
    pub fn validate(&self, requirements: &Requirements, strict: bool) -> UnitResult {
        debug_assert_eq!(
            self.chunk_count, compute_chunk_count(self.compression, self.data_size, self.blocks),
            "incorrect chunk count value"
        );

        self.data_window().validate(None)?;
        self.shared_attributes.display_window.validate(None)?;

        if strict {
            if requirements.is_multilayer() {
                if self.own_attributes.name.is_none() {
                    return Err(missing_attribute("layer name for multi layer file"));
                }
            }

            if self.blocks == Blocks::ScanLines && self.line_order == LineOrder::Unspecified {
                return Err(Error::invalid("unspecified line order in scan line images"));
            }

            if self.data_size == Vec2(0,0) {
                return Err(Error::invalid("empty data window"));
            }

            if self.shared_attributes.display_window.size == Vec2(0,0) {
                return Err(Error::invalid("empty display window"));
            }

            if !self.shared_attributes.pixel_aspect.is_normal() || self.shared_attributes.pixel_aspect < 1.0e-6 || self.shared_attributes.pixel_aspect > 1.0e6 {
                return Err(Error::invalid("pixel aspect ratio"));
            }

            if self.own_attributes.screen_window_width < 0.0 {
                return Err(Error::invalid("screen window width"));
            }
        }


        let allow_subsampling = !self.deep && self.blocks == Blocks::ScanLines;
        self.channels.validate(allow_subsampling, self.data_window(), strict)?;

        for (name, value) in &self.shared_attributes.custom {
            attributes::validate(name, value, requirements.has_long_names, allow_subsampling, self.data_window(), strict)?;
        }

        for (name, value) in &self.own_attributes.custom {
            attributes::validate(name, value, requirements.has_long_names, allow_subsampling, self.data_window(), strict)?;
        }


        // check if attribute names appear twice
        if strict {
            for (name, _) in &self.shared_attributes.custom {
                if !self.own_attributes.custom.contains_key(&name) {
                    return Err(Error::invalid(format!("duplicate attribute name: `{}`", name)));
                }
            }

            use attributes::required_attribute_names::*;
            let reserved_names = [
                TILES, NAME, BLOCK_TYPE, DEEP_DATA_VERSION, CHUNKS, MAX_SAMPLES, CHANNELS, COMPRESSION,
                DATA_WINDOW, DISPLAY_WINDOW, LINE_ORDER, PIXEL_ASPECT, WINDOW_CENTER, WINDOW_WIDTH,
                WHITE_LUMINANCE, ADOPTED_NEUTRAL, RENDERING_TRANSFORM, LOOK_MOD_TRANSFORM, X_DENSITY,
                OWNER, COMMENTS, CAPTURE_DATE, UTC_OFFSET, LONGITUDE, LATITUDE, ALTITUDE, FOCUS,
                EXPOSURE_TIME, APERTURE, ISO_SPEED, ENVIRONMENT_MAP, KEY_CODE, TIME_CODE, WRAP_MODES,
                FRAMES_PER_SECOND, MULTI_VIEW, WORLD_TO_CAMERA, WORLD_TO_NDC, DEEP_IMAGE_STATE,
                ORIGINAL_DATA_WINDOW, DWA_COMPRESSION_LEVEL, PREVIEW, VIEW, CHROMATICITIES
            ];

            for &reserved in reserved_names.iter() {
                let name  = Text::from_bytes_unchecked(SmallVec::from_slice(reserved));
                if self.own_attributes.custom.contains_key(&name) || self.shared_attributes.custom.contains_key(&name) {
                    return Err(Error::invalid(format!(
                        "attribute name `{}` is reserved and cannot be custom",
                         Text::from_bytes_unchecked(reserved.into())
                    )));
                }
            }
        }

        if self.deep {
            if strict {
                if self.own_attributes.name.is_none() {
                    return Err(missing_attribute("layer name for deep file"));
                }

                if self.max_samples_per_pixel.is_none() {
                    return Err(Error::invalid("missing max samples per pixel attribute for deepdata"));
                }
            }

            match self.deep_data_version {
                Some(1) => {},
                Some(_) => return Err(Error::unsupported("deep data version")),
                None => return Err(missing_attribute("deep data version")),
            }

            if !self.compression.supports_deep_data() {
                return Err(Error::invalid("compression method does not support deep data"));
            }
        }

        Ok(())
    }

    /// Read the headers without validating them.
    pub fn read_all(read: &mut PeekRead<impl Read>, version: &Requirements, skip_invalid_attributes: bool) -> Result<Headers> {
        if !version.is_multilayer() {
            Ok(smallvec![ Header::read(read, version, skip_invalid_attributes)? ])
        }
        else {
            let mut headers = SmallVec::new();

            while !sequence_end::has_come(read)? {
                headers.push(Header::read(read, version, skip_invalid_attributes)?);
            }

            Ok(headers)
        }
    }

    /// Without validation, write the headers to the byte stream.
    pub fn write_all(headers: &[Header], write: &mut impl Write, is_multilayer: bool) -> UnitResult {
        for header in headers {
            header.write(write)?;
        }

        if is_multilayer {
            sequence_end::write(write)?;
        }

        Ok(())
    }

    /// Read the value without validating.
    pub fn read(read: &mut PeekRead<impl Read>, requirements: &Requirements, skip_invalid_attributes: bool) -> Result<Self> {
        let max_string_len = if requirements.has_long_names { 256 } else { 32 }; // TODO DRY this information

        // these required attributes will be filled when encountered while parsing
        let mut tiles = None;
        let mut block_type = None;
        let mut version = None;
        let mut chunk_count = None;
        let mut max_samples_per_pixel = None;
        let mut channels = None;
        let mut compression = None;
        let mut data_window = None;
        let mut display_window = None;
        let mut line_order = None;
        let mut layer_attributes = LayerAttributes::default();
        let mut image_attributes = ImageAttributes::default();

        // read each attribute in this header
        while !sequence_end::has_come(read)? {
            let (attribute_name, value) = attributes::read(read, max_string_len)?;

            // if the attribute value itself is ok, record it
            match value {
                Ok(value) => {
                    use crate::meta::attributes::required_attribute_names as ty;
                    use crate::meta::attributes::AttributeValue::*;

                    // if the attribute is a required attribute, set the corresponding variable directly.
                    // otherwise, add the attribute to the vector of custom attributes

                    // the following attributes will only be set if the type matches the commonly used type for that attribute
                    match (attribute_name.bytes(), value) {
                        (ty::BLOCK_TYPE, Text(value)) => block_type = Some(attributes::BlockType::parse(value)?),
                        (ty::TILES, TileDescription(value)) => tiles = Some(value),
                        (ty::CHANNELS, ChannelList(value)) => channels = Some(value),
                        (ty::COMPRESSION, Compression(value)) => compression = Some(value),
                        (ty::DATA_WINDOW, IntRect(value)) => data_window = Some(value),
                        (ty::DISPLAY_WINDOW, IntRect(value)) => display_window = Some(value),
                        (ty::LINE_ORDER, LineOrder(value)) => line_order = Some(value),
                        (ty::DEEP_DATA_VERSION, I32(value)) => version = Some(value),

                        (ty::MAX_SAMPLES, I32(value)) => max_samples_per_pixel = Some(
                            i32_to_usize(value, "max sample count")?
                        ),

                        (ty::CHUNKS, I32(value)) => chunk_count = Some(
                            i32_to_usize(value, "chunk count")?
                        ),

                        (ty::NAME, Text(value)) => layer_attributes.name = Some(value),
                        (ty::WINDOW_CENTER, FloatVec2(value)) => layer_attributes.screen_window_center = value,
                        (ty::WINDOW_WIDTH, F32(value)) => layer_attributes.screen_window_width = value,

                        (ty::WHITE_LUMINANCE, F32(value)) => layer_attributes.white_luminance = Some(value),
                        (ty::ADOPTED_NEUTRAL, FloatVec2(value)) => layer_attributes.adopted_neutral = Some(value),
                        (ty::RENDERING_TRANSFORM, Text(value)) => layer_attributes.rendering_transform = Some(value),
                        (ty::LOOK_MOD_TRANSFORM, Text(value)) => layer_attributes.look_modification_transform = Some(value),
                        (ty::X_DENSITY, F32(value)) => layer_attributes.x_density = Some(value),

                        (ty::OWNER, Text(value)) => layer_attributes.owner = Some(value),
                        (ty::COMMENTS, Text(value)) => layer_attributes.comments = Some(value),
                        (ty::CAPTURE_DATE, Text(value)) => layer_attributes.capture_date = Some(value),
                        (ty::UTC_OFFSET, F32(value)) => layer_attributes.utc_offset = Some(value),
                        (ty::LONGITUDE, F32(value)) => layer_attributes.longitude = Some(value),
                        (ty::LATITUDE, F32(value)) => layer_attributes.latitude = Some(value),
                        (ty::ALTITUDE, F32(value)) => layer_attributes.altitude = Some(value),
                        (ty::FOCUS, F32(value)) => layer_attributes.focus = Some(value),
                        (ty::EXPOSURE_TIME, F32(value)) => layer_attributes.exposure = Some(value),
                        (ty::APERTURE, F32(value)) => layer_attributes.aperture = Some(value),
                        (ty::ISO_SPEED, F32(value)) => layer_attributes.iso_speed = Some(value),
                        (ty::ENVIRONMENT_MAP, EnvironmentMap(value)) => layer_attributes.environment_map = Some(value),
                        (ty::KEY_CODE, KeyCode(value)) => layer_attributes.key_code = Some(value),
                        (ty::WRAP_MODES, Text(value)) => layer_attributes.wrap_modes = Some(value),
                        (ty::FRAMES_PER_SECOND, Rational(value)) => layer_attributes.frames_per_second = Some(value),
                        (ty::MULTI_VIEW, TextVector(value)) => layer_attributes.multi_view = Some(value),
                        (ty::WORLD_TO_CAMERA, Matrix4x4(value)) => layer_attributes.world_to_camera = Some(value),
                        (ty::WORLD_TO_NDC, Matrix4x4(value)) => layer_attributes.world_to_normalized_device = Some(value),
                        (ty::DEEP_IMAGE_STATE, Rational(value)) => layer_attributes.deep_image_state = Some(value),
                        (ty::ORIGINAL_DATA_WINDOW, IntRect(value)) => layer_attributes.original_data_window = Some(value),
                        (ty::DWA_COMPRESSION_LEVEL, F32(value)) => layer_attributes.dwa_compression_level = Some(value),
                        (ty::PREVIEW, Preview(value)) => layer_attributes.preview = Some(value),
                        (ty::VIEW, Text(value)) => layer_attributes.view = Some(value),

                        (ty::PIXEL_ASPECT, F32(value)) => image_attributes.pixel_aspect = value,
                        (ty::TIME_CODE, TimeCode(value)) => image_attributes.time_code = Some(value),
                        (ty::CHROMATICITIES, Chromaticities(value)) => image_attributes.chromaticities = Some(value),

                        // insert unknown attributes of these types into image attributes,
                        // as these must be the same for all headers
                        (_, value @ Chromaticities(_)) |
                        (_, value @ TimeCode(_)) => {
                            image_attributes.custom.insert(attribute_name, value);
                        },

                        // insert unknown attributes into layer attributes
                        (_, value) => {
                            layer_attributes.custom.insert(attribute_name, value);
                        },

                    }
                },

                // in case the attribute value itself is not ok, but the rest of the image is
                // only abort reading the image if desired
                Err(error) => {
                    if !skip_invalid_attributes { return Err(error); }
                }
            }
        }

        let compression = compression.ok_or(missing_attribute("compression"))?;
        let data_window = data_window.ok_or(missing_attribute("data window"))?;

        image_attributes.display_window = display_window.ok_or(missing_attribute("display window"))?;
        layer_attributes.data_position = data_window.position;

        let data_size = data_window.size;

        let blocks = match block_type {
            None if requirements.is_single_layer_and_tiled => {
                Blocks::Tiles(tiles.ok_or(missing_attribute("tiles"))?)
            },
            Some(BlockType::Tile) | Some(BlockType::DeepTile) => {
                Blocks::Tiles(tiles.ok_or(missing_attribute("tiles"))?)
            },

            _ => Blocks::ScanLines,
        };

        // check size now to prevent panics while computing the chunk size
        data_window.validate(None)?;

        let computed_chunk_count = compute_chunk_count(compression, data_size, blocks);
        if chunk_count.is_some() && chunk_count != Some(computed_chunk_count) {
            return Err(Error::invalid("chunk count not matching data size"));
        }

        let header = Header {
            compression,

            // always compute ourselves, because we cannot trust anyone out there 😱
            chunk_count: computed_chunk_count,

            data_size,

            shared_attributes: image_attributes,
            own_attributes: layer_attributes,

            channels: channels.ok_or(missing_attribute("channels"))?,
            line_order: line_order.unwrap_or(LineOrder::Unspecified),

            blocks,
            max_samples_per_pixel,
            deep_data_version: version,
            deep: block_type == Some(BlockType::DeepScanLine) || block_type == Some(BlockType::DeepTile),
        };

        Ok(header)
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write(&self, write: &mut impl Write) -> UnitResult {

        macro_rules! write_attributes {
            ( $($name: ident : $variant: ident = $value: expr),* ) => { $(
                attributes::write($name, & $variant ($value .clone()), write)?; // TODO without clone
            )* };
        }

        macro_rules! write_optional_attributes {
            ( $($name: ident : $variant: ident = $value: expr),* ) => { $(
                if let Some(value) = $value {
                    attributes::write($name, & $variant (value.clone()), write)?; // TODO without clone
                };
            )* };
        }

        {
            use crate::meta::attributes::required_attribute_names::*;
            use AttributeValue::*;

            let (block_type, tiles) = match self.blocks {
                Blocks::ScanLines => (attributes::BlockType::ScanLine, None),
                Blocks::Tiles(tiles) => (attributes::BlockType::Tile, Some(tiles))
            };

            fn usize_as_i32(value: usize) -> AttributeValue {
                I32(i32::try_from(value).expect("u32 exceeds i32 range"))
            }

            write_optional_attributes!(
                TILES: TileDescription = &tiles,
                DEEP_DATA_VERSION: I32 = &self.deep_data_version,
                MAX_SAMPLES: usize_as_i32 = &self.max_samples_per_pixel
            );

            write_attributes!(
                // chunks is not actually required, but always computed in this library anyways
                CHUNKS: usize_as_i32 = &self.chunk_count,

                BLOCK_TYPE: BlockType = &block_type,
                CHANNELS: ChannelList = &self.channels,
                COMPRESSION: Compression = &self.compression,
                LINE_ORDER: LineOrder = &self.line_order,
                DATA_WINDOW: IntRect = &self.data_window(),

                DISPLAY_WINDOW: IntRect = &self.shared_attributes.display_window,
                PIXEL_ASPECT: F32 = &self.shared_attributes.pixel_aspect,

                WINDOW_CENTER: FloatVec2 = &self.own_attributes.screen_window_center,
                WINDOW_WIDTH: F32 = &self.own_attributes.screen_window_width
            );

            write_optional_attributes!(
                NAME: Text = &self.own_attributes.name,
                WHITE_LUMINANCE: F32 = &self.own_attributes.white_luminance,
                ADOPTED_NEUTRAL: FloatVec2 = &self.own_attributes.adopted_neutral,
                RENDERING_TRANSFORM: Text = &self.own_attributes.rendering_transform,
                LOOK_MOD_TRANSFORM: Text = &self.own_attributes.look_modification_transform,
                X_DENSITY: F32 = &self.own_attributes.x_density,
                OWNER: Text = &self.own_attributes.owner,
                COMMENTS: Text = &self.own_attributes.comments,
                CAPTURE_DATE: Text = &self.own_attributes.capture_date,
                UTC_OFFSET: F32 = &self.own_attributes.utc_offset,
                LONGITUDE: F32 = &self.own_attributes.longitude,
                LATITUDE: F32 = &self.own_attributes.latitude,
                ALTITUDE: F32 = &self.own_attributes.altitude,
                FOCUS: F32 = &self.own_attributes.focus,
                EXPOSURE_TIME: F32 = &self.own_attributes.exposure,
                APERTURE: F32 = &self.own_attributes.aperture,
                ISO_SPEED: F32 = &self.own_attributes.iso_speed,
                ENVIRONMENT_MAP: EnvironmentMap = &self.own_attributes.environment_map,
                KEY_CODE: KeyCode = &self.own_attributes.key_code,
                TIME_CODE: TimeCode = &self.shared_attributes.time_code,
                WRAP_MODES: Text = &self.own_attributes.wrap_modes,
                FRAMES_PER_SECOND: Rational = &self.own_attributes.frames_per_second,
                MULTI_VIEW: TextVector = &self.own_attributes.multi_view,
                WORLD_TO_CAMERA: Matrix4x4 = &self.own_attributes.world_to_camera,
                WORLD_TO_NDC: Matrix4x4 = &self.own_attributes.world_to_normalized_device,
                DEEP_IMAGE_STATE: Rational = &self.own_attributes.deep_image_state,
                ORIGINAL_DATA_WINDOW: IntRect = &self.own_attributes.original_data_window,
                DWA_COMPRESSION_LEVEL: F32 = &self.own_attributes.dwa_compression_level,
                CHROMATICITIES: Chromaticities = &self.shared_attributes.chromaticities,
                PREVIEW: Preview = &self.own_attributes.preview,
                VIEW: Text = &self.own_attributes.view
            );
        }

        for (name, value) in &self.shared_attributes.custom {
            attributes::write(name.bytes(), value, write)?;
        }

        for (name, value) in &self.own_attributes.custom {
            attributes::write(name.bytes(), value, write)?;
        }

        sequence_end::write(write)?;
        Ok(())
    }

    /// The rectangle describing the bounding box of this layer
    /// within the infinite global 2D space of the file.
    pub fn data_window(&self) -> IntRect {
        IntRect::new(self.own_attributes.data_position, self.data_size)
    }
}


impl Requirements {

    /// Infer version requirements from headers.
    pub fn infer(headers: &[Header]) -> Self {
        let first_header_has_tiles = headers.iter().next()
            .map_or(false, |header| header.blocks.has_tiles());

        let is_multilayer = headers.len() > 1;
        let deep = false; // TODO deep data

        Requirements {
            file_format_version: 2, // TODO find minimum
            is_single_layer_and_tiled: !is_multilayer && first_header_has_tiles,
            has_long_names: true, // TODO query header?
            has_multiple_layers: is_multilayer,
            has_deep_data: deep,
        }
    }


    // this is actually used for control flow, as the number of headers may be 1 in a multilayer file
    /// Is this file declared to contain multiple layers?
    pub fn is_multilayer(&self) -> bool {
        self.has_multiple_layers
    }

    /// Read the value without validating.
    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
        use ::bit_field::BitField;

        let version_and_flags = u32::read(read)?;

        // take the 8 least significant bits, they contain the file format version number
        let version = (version_and_flags & 0x000F) as u8;

        // the 24 most significant bits are treated as a set of boolean flags
        let is_single_tile = version_and_flags.get_bit(9);
        let has_long_names = version_and_flags.get_bit(10);
        let has_deep_data = version_and_flags.get_bit(11);
        let has_multiple_layers = version_and_flags.get_bit(12);

        // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0
        // if a file has any of these bits set to 1, it means this file contains
        // a feature that we don't support
        let unknown_flags = version_and_flags >> 13; // all flags excluding the 12 bits we already parsed

        if unknown_flags != 0 { // TODO test if this correctly detects unsupported files
            return Err(Error::unsupported("too new file feature flags"));
        }

        let version = Requirements {
            file_format_version: version,
            is_single_layer_and_tiled: is_single_tile, has_long_names,
            has_deep_data, has_multiple_layers,
        };

        Ok(version)
    }

    /// Without validation, write this instance to the byte stream.
    pub fn write<W: Write>(self, write: &mut W) -> UnitResult {
        use ::bit_field::BitField;

        // the 8 least significant bits contain the file format version number
        // and the flags are set to 0
        let mut version_and_flags = self.file_format_version as u32;

        // the 24 most significant bits are treated as a set of boolean flags
        version_and_flags.set_bit(9, self.is_single_layer_and_tiled);
        version_and_flags.set_bit(10, self.has_long_names);
        version_and_flags.set_bit(11, self.has_deep_data);
        version_and_flags.set_bit(12, self.has_multiple_layers);
        // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0

        version_and_flags.write(write)?;
        Ok(())
    }

    /// Validate this instance.
    pub fn validate(&self) -> UnitResult {
        if self.has_deep_data { // TODO deep data (and then remove this check)
            return Err(Error::unsupported("deep data not supported yet"));
        }

        if let 1..=2 = self.file_format_version {

            match (
                self.is_single_layer_and_tiled, self.has_deep_data, self.has_multiple_layers,
                self.file_format_version
            ) {
                // Single-part scan line. One normal scan line image.
                (false, false, false, 1..=2) => Ok(()),

                // Single-part tile. One normal tiled image.
                (true, false, false, 1..=2) => Ok(()),

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

                _ => Err(Error::invalid("file feature flags"))
            }
        }
        else {
            Err(Error::unsupported("file version newer than `2.0`"))
        }

    }
}

impl Default for LayerAttributes {
    fn default() -> Self {
        Self {
            data_position: Vec2(0, 0),
            screen_window_center: Vec2(0.0, 0.0),
            screen_window_width: 1.0,
            name: None,
            white_luminance: None,
            adopted_neutral: None,
            rendering_transform: None,
            look_modification_transform: None,
            x_density: None,
            owner: None,
            comments: None,
            capture_date: None,
            utc_offset: None,
            longitude: None,
            latitude: None,
            altitude: None,
            focus: None,
            exposure: None,
            aperture: None,
            iso_speed: None,
            environment_map: None,
            key_code: None,
            wrap_modes: None,
            frames_per_second: None,
            multi_view: None,
            world_to_camera: None,
            world_to_normalized_device: None,
            deep_image_state: None,
            original_data_window: None,
            dwa_compression_level: None,
            preview: None,
            view: None,
            custom: Default::default()
        }
    }
}

impl std::fmt::Debug for LayerAttributes {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let default_self = Self::default();

        let mut debug = formatter.debug_struct("LayerAttributes (only relevant attributes)");

        // always debug the following fields
        debug.field("data_position", &self.data_position);
        debug.field("name", &self.name);

        macro_rules! debug_non_default_fields {
            ( $( $name: ident ),* ) => { $(

                if self.$name != default_self.$name {
                    debug.field(stringify!($name), &self.$name);
                }

            )* };
        }

        // only debug these fields if they are not the default value
        debug_non_default_fields! {
            screen_window_center, screen_window_width,
            white_luminance, adopted_neutral, x_density,
            rendering_transform, look_modification_transform,
            owner, comments,
            capture_date, utc_offset,
            longitude, latitude, altitude,
            focus, exposure, aperture, iso_speed,
            environment_map, key_code, wrap_modes,
            frames_per_second, multi_view,
            world_to_camera, world_to_normalized_device,
            deep_image_state, original_data_window,
            dwa_compression_level,
            preview, view,
            custom
        }

        debug.finish()
    }
}

impl Default for ImageAttributes {
    fn default() -> Self {
        Self {
            pixel_aspect: 1.0,
            chromaticities: None,
            time_code: None,
            custom: Default::default(),
            display_window: Default::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::meta::{MetaData, Requirements, Header, ImageAttributes, LayerAttributes, compute_chunk_count};
    use crate::meta::attributes::{Text, ChannelList, IntRect, LineOrder, Channel, SampleType};
    use crate::compression::Compression;
    use crate::meta::Blocks;
    use crate::math::*;

    #[test]
    fn round_trip_requirements() {
        let requirements = Requirements {
            file_format_version: 2,
            is_single_layer_and_tiled: true,
            has_long_names: false,
            has_deep_data: true,
            has_multiple_layers: false
        };

        let mut data: Vec<u8> = Vec::new();
        requirements.write(&mut data).unwrap();
        let read = Requirements::read(&mut data.as_slice()).unwrap();
        assert_eq!(requirements, read);
    }

    #[test]
    fn round_trip(){
        let header = Header {
            channels: ChannelList {
                list: smallvec![
                    Channel {
                        name: Text::from("main").unwrap(),
                        sample_type: SampleType::U32,
                        quantize_linearly: false,
                        sampling: Vec2(1, 1)
                    }
                ],
                bytes_per_pixel: 4
            },
            compression: Compression::Uncompressed,
            line_order: LineOrder::Increasing,
            deep_data_version: Some(1),
            chunk_count: compute_chunk_count(Compression::Uncompressed, Vec2(2000, 333), Blocks::ScanLines),
            max_samples_per_pixel: Some(4),
            shared_attributes: ImageAttributes {
                display_window: IntRect {
                    position: Vec2(2,1),
                    size: Vec2(11, 9)
                },
                pixel_aspect: 3.0,
                .. Default::default()
            },

            blocks: Blocks::ScanLines,
            deep: false,
            data_size: Vec2(2000, 333),
            own_attributes: LayerAttributes {
                name: Some(Text::from("test name lol").unwrap()),
                data_position: Vec2(3, -5),
                screen_window_center: Vec2(0.3, 99.0),
                screen_window_width: 0.19,
                .. Default::default()
            }
        };

        let meta = MetaData {
            requirements: Requirements {
                file_format_version: 2,
                is_single_layer_and_tiled: false,
                has_long_names: false,
                has_deep_data: false,
                has_multiple_layers: false
            },
            headers: smallvec![ header ],
        };


        let mut data: Vec<u8> = Vec::new();
        meta.write_validating_to_buffered(&mut data, true).unwrap();
        let meta2 = MetaData::read_from_buffered(data.as_slice(), false).unwrap();
        meta2.validate(None, true).unwrap();
        assert_eq!(meta, meta2);
    }
}

