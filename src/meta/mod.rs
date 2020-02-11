
///! Describes all meta data possible in an exr file.
pub mod attributes;

use crate::io::*;
use ::smallvec::SmallVec;
use self::attributes::*;
use crate::chunks::{TileCoordinates, Block};
use crate::error::*;
use std::fs::File;
use std::io::{BufReader};
use std::cmp::Ordering;
use crate::math::*;

/// Contains the complete meta data of an exr image.
/// Defines how the image is split up in the file,
/// the number and type of images and channels,
/// and various other attributes.
/// The usage of custom attributes is encouraged.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaData {

    /// Some flags summarizing the features that must be supported to decode the file.
    pub requirements: Requirements,

    /// One header to describe each image part in this file.
    pub headers: Headers,
}

pub type Headers = SmallVec<[Header; 3]>;
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

/// Describes a single image part in a file.
/// A file can have any number of image parts.
/// The meta data contains one header per image part.
// TODO non-public fields?
#[derive(Clone, Debug, PartialEq)]
pub struct Header {

    /// List of channels in this image part.
    pub channels: ChannelList,

    /// How the pixel data of all channels in this image part is compressed. May be `Compression::Uncompressed`.
    pub compression: Compression,

    /// The rectangle that positions this image part within the infinite 2D space.
    pub data_window: Box2I32,

    /// The rectangle anywhere in 2D space that clips all contents of the file.
    pub display_window: Box2I32,


    /// In what order the tiles of this header occur in the file.
    // todo: make optional?
    pub line_order: LineOrder,

    /// Aspect ratio of each pixel in this header.
    // todo: make optional?
    pub pixel_aspect: f32,

    /// Part of the perspective projection. Default should be `(0, 0)`.
    // todo: make optional?
    pub screen_window_center: Vec2<f32>,

    /// Part of the perspective projection. Default should be `1`.
    // todo: make optional?
    pub screen_window_width: f32,

    /// The name of this image part.
    /// Required if this file contains deep data or multiple image parts.
    pub name: Option<Text>,

    /// Describes how the pixels of this image part are divided into smaller blocks.
    /// A single block can be loaded without processing all bytes of a file.
    ///
    /// Also describes whether a file contains multiple resolution levels: mip maps or rip maps.
    /// This allows loading not the full resolution, but the smallest sensible resolution.
    //
    // Required if file contains deep data or multiple image parts.
    // Note: This value must agree with the version field's tile bit and deep data bit.
    // In this crate, this attribute will always have a value, for simplicity.
    pub blocks: Blocks,

    /// Whether this image part contains deep data.
    pub deep: bool,

    /// This library supports only deep data version 1.
    // TODO throw error on other version found
    pub deep_data_version: Option<i32>,

    /// Number of chunks, that is, scan line blocks or tiles, that this image has been divided into.
    /// This number is calculated once at the beginning
    /// of the read process or when creating a channel object.
    ///
    /// This value includes all chunks of all resolution levels.
    pub chunk_count: u32,

    // Required for deep data (deepscanline and deeptile) parts.
    // Note: Since the value of "maxSamplesPerPixel"
    // maybe be unknown at the time of opening the
    // file, the value “ -1 ” is written to the file to
    // indicate an unknown value. When the file is
    // closed, this will be overwritten with the correct
    // value.
    // If file writing does not complete
    // correctly due to an error, the value -1 will
    // remain. In this case, the value must be derived
    // by decoding each chunk in the part
    /// Maximum number of samples in a single pixel in a deep image.
    pub max_samples_per_pixel: Option<u32>,

    /// Optional attributes. May contain custom attributes.
    /// Does not contain the attributes already present in the `Header` struct.
    pub custom_attributes: Attributes,
}

pub type Attributes = Vec<Attribute>;

/// A summary of requirements that must be met to read this exr file.
/// Used to determine whether this file can be read by a given reader.
/// It includes the OpenEXR version number. This library aims to support version `2.0`.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Requirements {

    /// This library supports reading version 1 and 2, and writing version 2.
    // TODO write version 1 for simple images
    file_format_version: u8,

    /// If true, this image has tiled blocks and contains only a single image part.
    /// If false and not deep and not multipart, this image is a single part image with scan line blocks.
    is_single_part_and_tiled: bool,

    // in c or bad c++ this might have been relevant (omg is he allowed to say that)
    /// Whether this file has strings with a length greater than 31.
    /// Strings can never be longer than 255.
    has_long_names: bool,

    /// This image contains at least one image part with deep data.
    has_deep_data: bool,

    /// Whether this file contains multiple image parts.
    has_multiple_parts: bool,
}


/// Locates a rectangular section of pixels in an image.
#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct TileIndices {
    pub location: TileCoordinates,
    pub size: Vec2<u32>,
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

impl TileIndices {
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
}

impl Blocks {
    /// Whether this image is tiled. If false, this image is divided into scan line blocks.
    pub fn has_tiles(&self) -> bool {
        match self {
            Blocks::Tiles { .. } => true,
            _ => false
        }
    }
}


/// The first four bytes of each exr file.
/// Used to abort reading non-exr files.
pub mod magic_number {
    use super::*;

    /// The first four bytes of each exr file.
    pub const BYTES: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];

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

    pub fn validate_exr(read: &mut impl Read) -> PassiveResult {
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

    pub fn byte_size() -> usize {
        1
    }

    pub fn write<W: Write>(write: &mut W) -> PassiveResult {
        0_u8.write(write)
    }

    /// Peeks the next byte. If it is zero, consumes the byte and returns true.
    pub fn has_come(read: &mut PeekRead<impl Read>) -> Result<bool> {
        Ok(read.skip_if_eq(0)?)
    }
}


fn missing_attribute(name: &str) -> Error {
    Error::invalid(format!("missing `{}` attribute", name))
}


/// Compute the number of tiles required to contain all values.
pub fn compute_block_count(full_res: u32, tile_size: u32) -> u32 {
    // round up, because if the image is not evenly divisible by the tiles,
    // we add another tile at the end (which is only partially used)
    RoundingMode::Up.divide(full_res, tile_size)
}

/// Compute the start position and size of a block inside a dimension.
#[inline]
pub fn calculate_block_position_and_size(total_size: u32, block_size: u32, block_index: u32) -> (u32, u32) {
    let block_position = block_size * block_index;
    (block_position, calculate_block_size(total_size, block_size, block_position))
}

/// Calculate the size of a single block. If this is the last block,
/// this only returns the required size, which is always smaller than the default block size.
// TODO use this method everywhere instead of convoluted formulas
#[inline]
pub fn calculate_block_size(total_size: u32, block_size: u32, block_position: u32) -> u32 {
    debug_assert!(block_position < total_size, "pos: {}, size: {}", block_position, total_size);

    if block_position + block_size <= total_size {
        block_size
    }
    else {
        total_size - block_position
    }
}


/// Calculate number of mip levels in a given resolution.
// TODO this should be cached? log2 may be very expensive
pub fn compute_level_count(round: RoundingMode, full_res: u32) -> u32 {
    round.log2(full_res) + 1
}

/// Calculate the size of a single mip level by index.
// TODO this should be cached? log2 may be very expensive
pub fn compute_level_size(round: RoundingMode, full_res: u32, level_index: u32) -> u32 {
    round.divide(full_res,  1 << level_index).max(1)
}

/// Iterates over all rip map level resolutions of a given size, including the indices of each level.
/// The order of iteration conforms to `LineOrder::Increasing`.
// TODO cache these?
// TODO compute these directly instead of summing up an iterator?
pub fn rip_map_levels(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=(Vec2<u32>, Vec2<u32>)> {
    rip_map_indices(round, max_resolution).map(move |level_indices|{
        // TODO progressively divide instead??
        let width = compute_level_size(round, max_resolution.0, level_indices.0);
        let height = compute_level_size(round, max_resolution.1, level_indices.1);
        (level_indices, Vec2(width, height))
    })
}

/// Iterates over all mip map level resolutions of a given size, including the indices of each level.
/// The order of iteration conforms to `LineOrder::Increasing`.
// TODO cache all these level values when computing table offset size??
// TODO compute these directly instead of summing up an iterator?
pub fn mip_map_levels(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=(u32, Vec2<u32>)> {
    mip_map_indices(round, max_resolution)
        .map(move |level_index|{
            // TODO progressively divide instead??
            let width = compute_level_size(round, max_resolution.0, level_index);
            let height = compute_level_size(round, max_resolution.1, level_index);
            (level_index, Vec2(width, height))
        })
}

/// Iterates over all rip map level indices of a given size.
/// The order of iteration conforms to `LineOrder::Increasing`.
pub fn rip_map_indices(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=Vec2<u32>> {
    let (width, height) = (
        compute_level_count(round, max_resolution.0),
        compute_level_count(round, max_resolution.1)
    );

    (0..height).flat_map(move |y_level|{
        (0..width).map(move |x_level|{
            Vec2(x_level, y_level)
        })
    })
}

/// Iterates over all mip map level indices of a given size.
/// The order of iteration conforms to `LineOrder::Increasing`.
pub fn mip_map_indices(round: RoundingMode, max_resolution: Vec2<u32>) -> impl Iterator<Item=u32> {
    (0..compute_level_count(round, max_resolution.0.max(max_resolution.1)))
}

/// Compute the number of chunks that an image is divided into. May be an expensive operation.
// If not multipart and chunkCount not present,
// the number of entries in the chunk table is computed
// using the dataWindow and tileDesc attributes and the compression format
pub fn compute_chunk_count(compression: Compression, data_window: Box2I32, blocks: Blocks) -> crate::error::Result<u32> {
    let data_size = data_window.size;

    if let Blocks::Tiles(tiles) = blocks {
        let round = tiles.rounding_mode;
        let Vec2(tile_width, tile_height) = tiles.tile_size;

        // TODO cache all these level values??
        use crate::meta::attributes::LevelMode::*;
        Ok(match tiles.level_mode {
            Singular => {
                let tiles_x = compute_block_count(data_size.0, tile_width);
                let tiles_y = compute_block_count(data_size.1, tile_height);
                tiles_x * tiles_y
            }

            MipMap => {
                mip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_block_count(level_width, tile_width) * compute_block_count(level_height, tile_height)
                }).sum()
            },

            RipMap => {
                // TODO test this
                rip_map_levels(round, data_size).map(|(_, Vec2(level_width, level_height))| {
                    compute_block_count(level_width, tile_width) * compute_block_count(level_height, tile_height)
                }).sum()
            }
        })
    }

    // scan line blocks never have mip maps // TODO check if this is true
    else {
        Ok(compute_block_count(data_size.1, compression.scan_lines_per_block()))
    }
}



impl MetaData {
    /// Read the exr meta data from a file.
    /// Use `read_from_unbuffered` instead if you do not have a file.
    #[must_use]
    pub fn read_from_file(path: impl AsRef<::std::path::Path>) -> Result<Self> {
        Self::read_from_unbuffered(File::open(path)?)
    }

    /// Buffer the reader and then read the exr meta data from it.
    /// Use `read_from_buffered` if your reader is an in-memory reader.
    /// Use `read_from_file` if you have a file path.
    #[must_use]
    pub fn read_from_unbuffered(unbuffered: impl Read) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered))
    }

    /// Read the exr meta data from a reader.
    /// Use `read_from_file` if you have a file path.
    /// Use `read_from_unbuffered` if this is not an in-memory reader.
    #[must_use]
    pub fn read_from_buffered(buffered: impl Read) -> Result<Self> {
        let mut read = PeekRead::new(buffered);
        MetaData::read_from_buffered_peekable(&mut read)
    }

    #[must_use]
    pub(crate) fn read_from_buffered_peekable(read: &mut PeekRead<impl Read>) -> Result<Self> {
        magic_number::validate_exr(read)?;
        let requirements = Requirements::read(read)?;
        let headers = Header::read_all(read, &requirements)?;

        // TODO check if supporting requirements 2 always implies supporting requirements 1
        let meta = MetaData { requirements, headers };
        meta.validate()?;

        Ok(meta)
    }

    pub(crate) fn write_to_buffered(&self, write: &mut impl Write) -> PassiveResult {
        self.validate()?;

        magic_number::write(write)?;
        self.requirements.write(write)?;
        Header::write_all(self.headers.as_slice(), write, &self.requirements)?;
        Ok(())
    }

    /// Read one offset table from the reader for each header.
    pub fn read_offset_tables(read: &mut PeekRead<impl Read>, headers: &Headers) -> Result<OffsetTables> {
        headers.iter()
            .map(|header| u64::read_vec(read, header.chunk_count as usize, std::u16::MAX as usize, None))
            .collect()
    }

    /// Skip the offset tables by advancing the reader.
    // TODO use seek for large (probably all) tables!
    pub fn skip_offset_tables(read: &mut PeekRead<impl Read>, headers: &Headers) -> Result<u64> {
        let chunk_count: u64 = headers.iter().map(|header| header.chunk_count as u64).sum();
         crate::io::skip_bytes(read, chunk_count * u64::BYTE_SIZE as u64)?;
        Ok(chunk_count)
    }

    // TODO also check for writing valid files
    pub fn validate(&self) -> PassiveResult {
        let headers = self.headers.len();

        if headers == 0 {
            return Err(Error::invalid("missing headers"));
        }

        self.requirements.validate()?;
        if self.requirements.file_format_version == 1 {
            debug_assert_eq!(headers, 1);
        }

        for header in &self.headers {
            header.validate(&self.requirements)?;
        }

        Ok(())
    }
}



impl Header {
    /// Iterate over all tile indices in this header in `LineOrder::Increasing` order.
    pub fn blocks_increasing_y_order(&self) -> impl Iterator<Item = TileIndices> + ExactSizeIterator + DoubleEndedIterator {
        fn tiles_of(image_size: Vec2<u32>, tile_size: Vec2<u32>, level_index: Vec2<u32>) -> impl Iterator<Item=TileIndices> {
            fn divide_and_rest(total_size: u32, block_size: u32) -> impl Iterator<Item=(u32, u32)> {
                let block_count = compute_block_count(total_size, block_size);
                (0..block_count).map(move |block_index| (block_index, calculate_block_size(total_size, block_size, block_index)))
            }

            divide_and_rest(image_size.1, tile_size.1).flat_map(move |(y_index, tile_height)|{
                divide_and_rest(image_size.0, tile_size.0).map(move |(x_index, tile_width)|{
                    TileIndices {
                        size: Vec2(tile_width, tile_height),
                        location: TileCoordinates {
                            tile_index: Vec2::try_from(Vec2(x_index, y_index)).unwrap(),
                            level_index: Vec2::try_from(level_index).unwrap(),
                        },
                    }
                })
            })
        }

        let vec: Vec<TileIndices> = {
            if let Blocks::Tiles(tiles) = self.blocks {
                match tiles.level_mode {
                    LevelMode::Singular => {
                        tiles_of(self.data_window.size, tiles.tile_size, Vec2(0, 0)).collect()
                    },
                    LevelMode::MipMap => {
                        mip_map_levels(tiles.rounding_mode, self.data_window.size)
                            .flat_map(move |(level_index, level_size)|{
                                tiles_of(level_size, tiles.tile_size, Vec2(level_index, level_index))
                            })
                            .collect()
                    },
                    LevelMode::RipMap => {
                        rip_map_levels(tiles.rounding_mode, self.data_window.size)
                            .flat_map(move |(level_index, level_size)| {
                                tiles_of(level_size, tiles.tile_size, level_index)
                            })
                            .collect()
                    }
                }
            }
            else {
                let tiles = Vec2(self.data_window.size.0, self.compression.scan_lines_per_block());
                tiles_of(self.data_window.size, tiles, Vec2(0,0)).collect()
            }
        };

        vec.into_iter() // TODO without collect
    }

    /// Calculate the position of a block in the global infinite 2D space of a file. May be negative.
    pub fn get_block_data_window_coordinates(&self, tile: TileCoordinates) -> Result<Box2I32> {
        let data = self.get_absolute_block_indices(tile)?;
        Ok(data.with_origin(self.data_window.start))
    }

    /// Calculate the pixel index rectangle inside this header. Is not negative. Starts at `0`.
    pub fn get_absolute_block_indices(&self, tile: TileCoordinates) -> Result<Box2I32> {
        Ok(if let Blocks::Tiles(tiles) = self.blocks { // FIXME set to none if tile attribute exists but image is not tiled!
            let round = tiles.rounding_mode;

            let tile_size = Vec2::try_from(tiles.tile_size).unwrap();
            let Vec2(data_width, data_height) = self.data_window.size;

            let data_width = compute_level_size(round, data_width, tile.level_index.0 as u32);
            let data_height = compute_level_size(round, data_height, tile.level_index.1 as u32);
            let absolute_tile_coordinates = tile.to_data_indices(tile_size, Vec2(data_width, data_height));

            if absolute_tile_coordinates.start.0 >= data_width as i32 || absolute_tile_coordinates.start.1 >= data_height as i32 {
                return Err(Error::invalid("data block tile index"))
            }

            absolute_tile_coordinates
        }
        else {
            debug_assert_eq!(tile.tile_index.0, 0);

            let (y, height) = calculate_block_position_and_size(
                self.data_window.size.1,
                self.compression.scan_lines_per_block(),
                tile.tile_index.1 as u32
            );

            Box2I32 {
                start: Vec2(0, y as i32),
                size: Vec2(self.data_window.size.0, height)
            }
        })

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
                let y = (block.y_coordinate - self.data_window.start.1)
                    / self.compression.scan_lines_per_block() as i32;

                TileCoordinates {
                    tile_index: Vec2(0, y),
                    level_index: Vec2(0, 0)
                }
            },

            _ => return Err(Error::unsupported("deep data"))
        })
    }

    /// Maximum byte length of a compressed block, used for validation.
    pub fn max_block_byte_size(&self) -> usize {
        (
            self.channels.bytes_per_pixel * match self.blocks {
                Blocks::Tiles(tiles) => tiles.tile_size.0 * tiles.tile_size.1,
                Blocks::ScanLines => self.compression.scan_lines_per_block() * self.data_window.size.0
                // TODO What about deep data???
            }
        ) as usize
    }

    pub fn validate(&self, requirements: &Requirements) -> PassiveResult {
        if requirements.is_multipart() {
            if self.name.is_none() {
                return Err(missing_attribute("image part name"));
            }
        }

        if self.deep {
            if self.name.is_none() {
                return Err(missing_attribute("image part name"));
            }

            match self.deep_data_version {
                Some(1) => {},
                Some(_) => return Err(Error::invalid("deep data version")),
                None => return Err(missing_attribute("deep data version")),
            }

            // make maxSamplesPerPixel optional because some files don't have it
            /*if self.indices.max_samples_per_pixel.is_none() {
                return Err(Invalid::Missing(Value::Attribute("maxSamplesPerPixel (for deepdata)")).into());
            }*/

//            if !self.compression.supports_deep_data() {
//                return Err(Error::invalid("compress deep data"))
//                return Err(Invalid::Content(
//                    Value::Attribute("compression (for deepdata)"),
//                    Required::OneOf(&["none", "rle", "zips", "zip"])
//                ).into());
//            }
        }

        Ok(())
    }

    pub fn read_all(read: &mut PeekRead<impl Read>, version: &Requirements) -> Result<Headers> {
        if !version.is_multipart() { // TODO check a different way?
            Ok(smallvec![ Header::read(read, version)? ])
        }
        else {
            let mut headers = SmallVec::new();

            while !sequence_end::has_come(read)? {
                headers.push(Header::read(read, version)?);
            }

            Ok(headers)
        }
    }

    pub fn write_all(headers: &[Header], write: &mut impl Write, version: &Requirements) -> PassiveResult {
        for header in headers {
            header.write(write, version)?;
        }

        if version.is_multipart() {
            sequence_end::write(write)?;
        }

        Ok(())
    }

    pub fn read(read: &mut PeekRead<impl Read>, requirements: &Requirements) -> Result<Self> {
        let max_string_len = if requirements.has_long_names { 256 } else { 32 }; // TODO DRY this information
        let mut custom = Vec::new();

        // these required attributes will be filled when encountered while parsing
        let mut tiles = None;
        let mut name = None;
        let mut block_type = None;
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

        // read each attribute in this header
        while !sequence_end::has_come(read)? {
            let Attribute { name: attribute_name, value } = Attribute::read(read, max_string_len)?;

            // if the attribute is a required attribute, set the corresponding variable directly.
            // otherwise, add the attribute to the vector of custom attributes
            use crate::meta::attributes::required::*;
            match attribute_name.bytes() {
                TILES => tiles = Some(value.to_tile_description()?),
                NAME => name = Some(value.into_text()?),
                BLOCK_TYPE => block_type = Some(BlockType::parse(value.into_text()?)?),
                CHANNELS => channels = Some(value.into_channel_list()?),
                COMPRESSION => compression = Some(value.to_compression()?),
                DATA_WINDOW => data_window = Some(value.to_i32_box_2()?),
                DISPLAY_WINDOW => display_window = Some(value.to_i32_box_2()?),
                LINE_ORDER => line_order = Some(value.to_line_order()?),
                PIXEL_ASPECT => pixel_aspect = Some(value.to_f32()?),
                WINDOW_CENTER => screen_window_center = Some(value.to_f32_vec_2()?),
                WINDOW_WIDTH => screen_window_width = Some(value.to_f32()),
                VERSION => version = Some(value.to_i32()?),

                MAX_SAMPLES => max_samples_per_pixel = Some(
                    i32_to_u32_at(value.to_i32()?, "max sample count")?
                ),

                CHUNKS => chunk_count = Some(
                    i32_to_u32_at(value.to_i32()?, "chunk count")?
                ),

                _ => custom.push(Attribute { name: attribute_name, value }), // TODO only requested attributes?
            }
        }

        let compression = compression.ok_or(missing_attribute("compression"))?;
        let data_window = data_window.ok_or(missing_attribute("data window"))?;

        let blocks = match block_type {
            None if requirements.is_single_part_and_tiled => {
                Blocks::Tiles(tiles.ok_or(missing_attribute("tiles"))?)
            },
            Some(BlockType::Tile) | Some(BlockType::DeepTile) => {
                Blocks::Tiles(tiles.ok_or(missing_attribute("tiles"))?)
            },

            _ => Blocks::ScanLines,
        };

        let chunk_count = match chunk_count {
            None => compute_chunk_count(compression, data_window, blocks)?,
            Some(count) => count,
        };

        let header = Header {
            compression, data_window, chunk_count,

            channels: channels.ok_or(missing_attribute("channels"))?,
            display_window: display_window.ok_or(missing_attribute("display window"))?,
            line_order: line_order.ok_or(missing_attribute("line order"))?,
            pixel_aspect: pixel_aspect.ok_or(missing_attribute("pixel aspect"))?,
            screen_window_center: screen_window_center.ok_or(missing_attribute("screen window center"))?,
            screen_window_width: screen_window_width.ok_or(missing_attribute("screen window width"))??,

            blocks,
            name,
            max_samples_per_pixel,
            deep_data_version: version,
            custom_attributes: custom,
            deep: block_type == Some(BlockType::DeepScanLine) || block_type == Some(BlockType::DeepTile)
        };

        header.validate(requirements)?;
        Ok(header)
    }

    pub fn write(&self, write: &mut impl Write, version: &Requirements) -> PassiveResult {
        self.validate(&version).expect("check failed: header invalid");

        // FIXME do not allocate text object for writing!
        fn write_attr<T>(write: &mut impl Write, long: bool, name: &[u8], value: T, variant: impl Fn(T) -> AnyValue) -> PassiveResult {
            Attribute { name: Text::from_bytes_unchecked(SmallVec::from_slice(name)), value: variant(value) }
                .write(write, long)
        };

        fn write_opt_attr<T>(write: &mut impl Write, long: bool, name: &[u8], attribute: Option<T>, variant: impl Fn(T) -> AnyValue) -> PassiveResult {
            if let Some(value) = attribute { write_attr(write, long, name, value, variant) }
            else { Ok(()) }
        };

        {
            let long = version.has_long_names;
            use crate::meta::attributes::required::*;
            use AnyValue::*;


            let (block_type, tiles) = match self.blocks {
                Blocks::ScanLines => (attributes::BlockType::ScanLine, None),
                Blocks::Tiles(tiles) => (attributes::BlockType::Tile, Some(tiles))
            };

            write_opt_attr(write, long, TILES, tiles, TileDescription)?;

            write_opt_attr(write, long, NAME, self.name.clone(), Text)?;
            write_opt_attr(write, long, VERSION, self.deep_data_version, I32)?;
            write_opt_attr(write, long, MAX_SAMPLES, self.max_samples_per_pixel, |u| I32(u as i32))?;

            // not actually required, but always computed in this library anyways
            write_attr(write, long, CHUNKS, self.chunk_count, |u| I32(u as i32))?;
            write_attr(write, long, BLOCK_TYPE, block_type, BlockType)?;

            write_attr(write, long, CHANNELS, self.channels.clone(), ChannelList)?; // FIXME do not clone
            write_attr(write, long, COMPRESSION, self.compression, Compression)?;
            write_attr(write, long, DATA_WINDOW, self.data_window, I32Box2)?;
            write_attr(write, long, DISPLAY_WINDOW, self.display_window, I32Box2)?;
            write_attr(write, long, LINE_ORDER, self.line_order, LineOrder)?;
            write_attr(write, long, PIXEL_ASPECT, self.pixel_aspect, F32)?;
            write_attr(write, long, WINDOW_WIDTH, self.screen_window_width, F32)?;
            write_attr(write, long, WINDOW_CENTER, self.screen_window_center, F32Vec2)?;
        }

        for attrib in &self.custom_attributes {
            attrib.write(write, version.has_long_names)?;
        }

        sequence_end::write(write)?;
        Ok(())
    }
}


impl Requirements {
    pub fn new(version: u8, multipart: bool, has_tiles: bool, long_names: bool, deep: bool) -> Self {
        Requirements {
            file_format_version: version,
            is_single_part_and_tiled: !multipart && has_tiles,
            has_long_names: long_names,
            has_deep_data: deep, // TODO
            has_multiple_parts: multipart
        }
    }

    // this is actually used for control flow, as the number of headers may be 1 in a multipart file
    pub fn is_multipart(&self) -> bool {
        self.has_multiple_parts
    }

    pub fn read<R: Read>(read: &mut R) -> Result<Self> {
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
            return Err(Error::unsupported("file feature flags"));
        }

        let version = Requirements {
            file_format_version: version,
            is_single_part_and_tiled: is_single_tile, has_long_names,
            has_deep_data, has_multiple_parts,
        };

        version.validate()?;
        Ok(version)
    }

    pub fn write<W: Write>(self, write: &mut W) -> PassiveResult {
        use ::bit_field::BitField;

        self.validate()?;

        // the 8 least significant bits contain the file format version number
        // and the flags are set to 0
        let mut version_and_flags = self.file_format_version as u32;

        // the 24 most significant bits are treated as a set of boolean flags
        version_and_flags.set_bit(9, self.is_single_part_and_tiled);
        version_and_flags.set_bit(10, self.has_long_names);
        version_and_flags.set_bit(11, self.has_deep_data);
        version_and_flags.set_bit(12, self.has_multiple_parts);
        // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0

        version_and_flags.write(write)?;
        Ok(())
    }

    pub fn validate(&self) -> PassiveResult {
        if let 1..=2 = self.file_format_version {

            match (
                self.is_single_part_and_tiled, self.has_deep_data, self.has_multiple_parts,
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


#[cfg(test)]
mod test {
    use crate::meta::{MetaData, Requirements, Header};
    use crate::meta::attributes::{Text, ChannelList, Box2I32, LineOrder, Channel, PixelType};
    use crate::compression::Compression;
    use crate::meta::Blocks;
    use crate::math::*;

    #[test]
    fn round_trip_requirements() {
        let requirements = Requirements::new(2, true, true, true, true);

        let mut data: Vec<u8> = Vec::new();
        requirements.write(&mut data).unwrap();
        let read = Requirements::read(&mut data.as_slice()).unwrap();
        assert_eq!(requirements, read);
    }

    #[test]
    fn round_trip(){
        let meta = MetaData {
            requirements: Requirements::new(2, false, false, false, false),
            headers: smallvec![
                Header {
                    channels: ChannelList {
                        list: smallvec![
                            Channel {
                                name: Text::from_str("main").unwrap(),
                                pixel_type: PixelType::U32,
                                is_linear: false,
                                sampling: Vec2(1, 1)
                            }
                        ],
                        bytes_per_pixel: 4
                    },
                    compression: Compression::Uncompressed,
                    data_window: Box2I32 {
                        start: Vec2(-3,-1),
                        size: Vec2(22, 21)
                    },
                    display_window: Box2I32 {
                        start: Vec2(2,1),
                        size: Vec2(11, 9)
                    },
                    line_order: LineOrder::Increasing,
                    pixel_aspect: 1.0,
                    screen_window_center: Vec2(5.0, 5.0),
                    screen_window_width: 10.0,
                    name: None,
                    deep_data_version: None,
                    chunk_count: 1,
                    max_samples_per_pixel: None,
                    custom_attributes: vec![ /* TODO */ ],

                    blocks: Blocks::ScanLines,
                    deep: false,
                }
            ],
//            offset_tables: smallvec![
//                vec![
//                    0, 2, 3, 4, 5, 6, 7, 1234, 23, 412,4 ,124,4,
//                ]
//            ]
        };


        let mut data: Vec<u8> = Vec::new();
        meta.write_to_buffered(&mut data).unwrap();
        let meta2 = MetaData::read_from_buffered(data.as_slice()).unwrap();
        assert_eq!(meta, meta2);
    }
}

