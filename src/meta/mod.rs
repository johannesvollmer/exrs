
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




#[derive(Debug, Clone, PartialEq)]
pub struct MetaData {
    pub requirements: Requirements,

    /// separate header for each part, requires a null byte signalling the end of each header
    pub headers: Headers,
}

pub type Headers = SmallVec<[Header; 3]>;
pub type OffsetTables = SmallVec<[OffsetTable; 3]>;

/// For scan line blocks, the line offset table is a sequence of scan line offsets,
/// with one offset per scan line block. In the table, scan line offsets are
/// ordered according to increasing scan line y coordinates
///
/// For tiles, the offset table is a sequence of tile offsets, one offset per tile.
/// In the table, scan line offsets are sorted the same way as tiles in IncreasingY order
///
/// For multi-part files, each part defined in the header component has a corresponding chunk offset table
///
/// If the multipart (12) bit is unset and the chunkCount is not present, the number of entries in the
/// chunk table is computed using the dataWindow and tileDesc attributes and the compression format.
/// 2. If the multipart (12) bit is set, the header must contain a chunkCount attribute (which indicates the
/// size of the table and the number of chunks).
///
///
/// one per chunk, relative to file-start (!) in bytes
pub type OffsetTable = Vec<u64>;

// TODO non-public fields?
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub channels: ChannelList,
    pub compression: Compression,
    pub data_window: I32Box2,
    pub display_window: I32Box2,

    // todo: make optionals?
    pub line_order: LineOrder,
    pub pixel_aspect: f32,
    pub screen_window_center: (f32, f32),
    pub screen_window_width: f32,

    /// TileDescription: size of the tiles and the number of resolution levels in the file
    /// Required for parts of type tiledimage and deeptile
    pub tiles: Option<TileDescription>,

    /// The name of the `Part` which contains this Header.
    /// Required if either the multipart bit (12) or the non-image bit (11) is set
    pub name: Option<Text>,

    /// Required if either the multipart bit (12) or the non-image bit (11) is set.
    /// Set to one of: scanlineimage, tiledimage, deepscanline, or deeptile.
    /// Note: This value must agree with the version field's tile bit (9) and non-image (deep data) bit (11) settings
    /// required for deep data. when deep data, Must be set to deepscanline or deeptile
    pub kind: Option<Kind>,

    /// This document describes version 1 data for all
    /// part types. version is required for deep data (deepscanline and deeptile) parts.
    /// If not specified for other parts, assume version=1
    /// required for deep data: Should be set to 1 . It will be changed if the format is updated
    pub deep_data_version: Option<i32>,

    /// Required if either the multipart bit (12) or the deep-data bit (11) is set
    pub chunk_count: Option<u32>,

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
    pub max_samples_per_pixel: Option<u32>,

    /// Requires a null byte signalling the end of each attribute
    /// Contains custom attributes
    pub custom_attributes: Attributes,
}

pub type Attributes = SmallVec<[Attribute; 8]>;


// FIXME TODO this should probably not be a struct but a module, and not passed everywhere,
/// since most of the fields don't matter after the first validation

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Requirements {
    /// is currently 2
    file_format_version: u8,

    /// bit 9
    /// if true: single-part tiles (bits 11 and 12 must be 0).
    /// if false and 11 and 12 are false: single-part scan-line.
    is_single_part_and_tiled: bool,

    /// bit 10
    /// if true: maximum name length is 255,
    /// else: 31 bytes for attribute names, attribute type names, and channel names
    /// in c or bad c++ this might have been relevant (omg is he allowed to say that)
    has_long_names: bool,

    /// bit 11 "non-image bit"
    /// if true: at least one deep (thus non-reqular)
    has_deep_data: bool,

    /// bit 12
    /// if true: is multipart
    /// (end-of-header byte must always be included
    /// and part-number-fields must be added to chunks)
    has_multiple_parts: bool,
}

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct TileIndices {
    pub level: (u32, u32),
    pub position: (u32, u32),
    pub size: (u32, u32),
}






impl Ord for TileIndices {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.position.1.cmp(&other.position.1) {
            Ordering::Equal => {
                self.position.0.cmp(&other.position.0)
            },

            other => other,
        }
    }
}

impl PartialOrd for TileIndices {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


pub mod magic_number {
    use super::*;

    pub const BYTES: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];

    pub fn write(write: &mut impl Write) -> std::io::Result<()> {
        u8::write_slice(write, &self::BYTES)
    }

    pub fn is_exr(read: &mut impl Read) -> std::io::Result<bool> {
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


pub mod sequence_end {
    use super::*;

    pub fn byte_size() -> usize {
        1
    }

    pub fn write<W: Write>(write: &mut W) -> std::io::Result<()> {
        0_u8.write(write)
    }

    pub fn has_come(read: &mut PeekRead<impl Read>) -> std::io::Result<bool> {
        read.skip_if_eq(0)
    }
}


pub fn missing_attribute(name: &str) -> Error {
    Error::invalid(format!("missing `{}` attribute", name))
}

pub fn positive_i32(value: i32, name: &str) -> Result<u32> {
    if value < 0 { Err(Error::invalid(name)) }
    else { Ok(value as u32) }
}



impl MetaData {
    #[must_use]
    pub fn read_from_file(path: impl AsRef<::std::path::Path>) -> Result<Self> {
        Self::read_from_unbuffered(File::open(path)?)
    }

    /// assumes that the provided reader is not buffered, and will create a buffer for it
    #[must_use]
    pub fn read_from_unbuffered<R: Read>(unbuffered: R) -> Result<Self> {
        Self::read_from_buffered(BufReader::new(unbuffered))
    }

    /// assumes the reader is buffered
    #[must_use]
    pub fn read_from_buffered<R: Read>(buffered: R) -> Result<Self> {
        let mut read = PeekRead::new(buffered);
        MetaData::read_from_buffered_peekable(&mut read)
    }

    #[must_use]
    pub fn read_from_buffered_peekable(read: &mut PeekRead<impl Read>) -> Result<Self> {
        magic_number::validate_exr(read)?;
        let requirements = Requirements::read(read)?;
        let headers = Header::read_all(read, &requirements)?;

        // TODO check if supporting requirements 2 always implies supporting requirements 1
        let meta = MetaData { requirements, headers };
        meta.validate()?;

        Ok(meta)
    }

    pub fn write(&self, write: &mut impl Write) -> PassiveResult {
        self.validate()?;

        magic_number::write(write)?;
        self.requirements.write(write)?;
        Header::write_all(self.headers.as_slice(), write, &self.requirements)?;
        Ok(())
    }

    // TODO skip reading offset tables if not required?
    pub fn read_offset_tables(read: &mut PeekRead<impl Read>, headers: &Headers) -> Result<OffsetTables> {
        headers.iter()
            .map(|header| {
                let entry_count = header.compute_offset_table_size()?;
                let vec = u64::read_vec(read, entry_count as usize, std::u16::MAX as usize)?;
                Ok(vec)
            })
            .collect()
    }

//    pub fn write_offset_tables<W: Write>(write: &mut W, tables: &OffsetTables) -> PassiveResult {
//        for table in tables {
//            u64::write_slice(write, &table)?;
//        }
//
//        Ok(())
//    }

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

    pub fn has_deep_data(&self) -> bool {
        match self.kind {
            Some(Kind::DeepTile) | Some(Kind::DeepScanLine) => true,
            _ => false
        }
    }

    pub fn has_tiles(&self, requirements: &Requirements) -> bool {
        requirements.is_single_part_and_tiled || match self.kind {
            Some(Kind::DeepTile) | Some(Kind::Tile) => {
                debug_assert!(self.tiles.is_some());
                true
            },
            _ => false
        }
    }

    pub fn get_raw_block_coordinates(&self, block: &Block) -> Result<I32Box2> {
        Ok(match block {
            Block::Tile(ref tile) => {
                let size = self.get_tile_size(tile.coordinates);
                I32Box2 {
                    x_min: tile.coordinates.tile_x,
                    y_min: tile.coordinates.tile_y,
                    x_max: tile.coordinates.tile_x + size.0 as i32 - 1,
                    y_max: tile.coordinates.tile_y + size.1 as i32 - 1
                }
            },
//                // FIXME is the level required here?
//                let size = self.get_tile_size(coordinates);
//                let level = (coordinates.level_x as u32, coordinates.level_y as u32);
//                let x = coordinates.tile_x - self.data_window.x_min;
//                let y = coordinates.tile_y - self.data_window.y_min;
//                debug_assert!(x >= 0 && y >= 0)
//
//                Ok(TileIndices { position: (x as u32, y as u32), size, level })
            Block::ScanLine(ref block) => {
                let height = self.get_scan_line_block_height(block.y_coordinate as u32);

                I32Box2 {
                    x_min: self.data_window.x_min, y_min: block.y_coordinate,
                    x_max: self.data_window.x_max, y_max: block.y_coordinate + height as i32 - 1
                }
            },

            _ => return Err(Error::unsupported("deep data"))
        })
    }

    pub fn get_block_data_indices(&self, block: &Block) -> Result<TileIndices> {
        let coordinates = self.get_raw_block_coordinates(block)?;

        assert!(coordinates.x_min >= self.data_window.x_min); // TODO Err() instead
        assert!(coordinates.y_min >= self.data_window.y_min); // TODO Err() insteads

        let position = (
            (coordinates.x_min - self.data_window.x_min) as u32,
            (coordinates.y_min - self.data_window.y_min) as u32
        );

        let size = coordinates.dimensions();

        Ok(TileIndices {
            level: match block {
                Block::Tile(ref tile) => (tile.coordinates.level_x as u32, tile.coordinates.level_y as u32),
                Block::ScanLine(ref _block) => (0,0), // FIXME is this correct?

                Block::DeepTile(ref tile) => (tile.coordinates.level_x as u32, tile.coordinates.level_y as u32),
                Block::DeepScanLine(ref _block) => (0,0), // FIXME is this correct?
            },

            position,
            size
        })
    }

    fn get_scan_line_block_height(&self, y: u32) -> u32 {
        debug_assert!(
            y as i32 >= self.data_window.y_min && y as i32 <= self.data_window.y_max,
            "invalid y coordinate: {}, (data window: {:?})", y, self.data_window
        );

        let lines_per_block = self.compression.scan_lines_per_block();
        let next_block_y = y + lines_per_block - self.data_window.y_min as u32;
        let data_height = self.data_window.dimensions().1; // TODO scan line blocks never have levels?

        let height = if next_block_y <= data_height { lines_per_block } else {
            let clipped_line_count = next_block_y - data_height; // TODO +/-1?
            lines_per_block - clipped_line_count
        };

        debug_assert_ne!(
            height, 0,
            "scan line block height is 0 where y = {} in header {:?} ({} x {} px) (window {:?}) ",
            y, self.name, self.data_window.dimensions().0, self.data_window.dimensions().1, self.data_window
        );

        height
    }

    fn get_tile_size(&self, tile: TileCoordinates) -> (u32, u32) {
        let tiles = self.tiles.expect("check failed: tiles not found");
        let round = tiles.rounding_mode;
        let default_tile_width = tiles.size.0;
        let default_tile_height = tiles.size.1;

        let (data_width, data_height) = self.data_window.dimensions();
        let data_width = compute_level_size(round, data_width as u32, tile.level_x as u32);
        let data_height = compute_level_size(round, data_height as u32, tile.level_y as u32);

        let y = tile.tile_y - self.data_window.y_min; // TODO divide by tile size?
        let x = tile.tile_x - self.data_window.x_min; // TODO divide by tile size?

        let next_tile_x = x as u32 + default_tile_width;
        let next_tile_y = y as u32 + default_tile_height;

        let width = if next_tile_x <= data_width { default_tile_width } else {
            let clipped_columns = next_tile_x - data_width; // TODO +/-1?
            default_tile_width - clipped_columns
        };

        let height = if next_tile_y <= data_height { default_tile_height } else {
            let clipped_lines = next_tile_y - data_height; // TODO +/-1?
            default_tile_height - clipped_lines
        };

        debug_assert!(
            height != 0 && width != 0,
            "tile size is 0 for tile {:?} in header {:#?}",
            tile, self
        );

        (width, height)
    }

    pub fn max_block_byte_size(&self) -> usize {
        (
            self.channels.bytes_per_pixel * match self.tiles {
                Some(tiles) => tiles.size.0 * tiles.size.1,
                None => self.compression.scan_lines_per_block() * self.data_window.dimensions().0 // TODO is this how it works?!?! What about deep data???
            }
        ) as usize
    }

    // TODO reuse this algorithm in crate::image::Part::new?
    pub fn compute_offset_table_size(&self) -> Result<u32> {
        if let Some(chunk_count) = self.chunk_count {
            Ok(chunk_count as u32) // TODO will this panic on negative number / invalid data?

        } else {
            // If not multipart and chunkCount not present,
            // the number of entries in the chunk table is computed
            // using the dataWindow and tileDesc attributes and the compression format
            let compression = self.compression;
            let data_window = self.data_window;
            data_window.validate(None)?;

            let data_size = data_window.dimensions();

            if let Some(tiles) = self.tiles {
                let round = tiles.rounding_mode;
                let (tile_width, tile_height) = tiles.size;

                // TODO cache all these level values??
                use crate::meta::attributes::LevelMode::*;
                Ok(match tiles.level_mode {
                    Singular => {
                        let tiles_x = compute_tile_count(data_size.0, tile_width);
                        let tiles_y = compute_tile_count(data_size.1, tile_height);
                        tiles_x * tiles_y
                    }

                    MipMap => {
                        mip_map_resolutions(round, data_size).map(|(level_width, level_height)| {
                            compute_tile_count(level_width, tile_width) * compute_tile_count(level_height, tile_height)
                        }).sum()
                    },

                    RipMap => {
                        // TODO test this
                        rip_map_resolutions(round, data_size).map(|(level_width, level_height)| {
                            compute_tile_count(level_width, tile_width) * compute_tile_count(level_height, tile_height)
                        }).sum()
                    }
                })

            }

            // scan line blocks never have mip maps // TODO check if this is true
            else {
                Ok(compute_tile_count(data_size.1, compression.scan_lines_per_block() as u32))
            }
        }
    }

    // TODO for all other fields too?
    pub fn kind_or_err(&self) -> Result<&Kind> {
        self.kind.as_ref().ok_or(Error::invalid("block type attribute"))
    }

    pub fn validate(&self, requirements: &Requirements) -> PassiveResult {
        if requirements.is_multipart() {
            if self.chunk_count.is_none() {
                return Err(missing_attribute("chunk count"));
            }
            if self.kind.is_none() {
                return Err(missing_attribute("block type"));
            }
            if self.name.is_none() {
                return Err(missing_attribute("image part name"));
            }
        }

        if self.has_deep_data() {
            if self.chunk_count.is_none() {
                return Err(missing_attribute("chunk count"));
            }

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

        if self.has_tiles(&requirements) {
            if self.tiles.is_none() {
                return Err(missing_attribute("tiles"));
            }
        }

        // TODO those do not have to agree
        // version-deepness and attribute-deepness must match
        /*if kind.is_deep_kind() != version.has_deep_data {
            return Err(Invalid::Content(
                Value::Attribute("type"),
                Required::OneOf(&["deepscanlines", "deeptiles"])
            ).into());
        }*/

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
        let has_multiple_headers = headers.len() != 1;

        for header in headers {
            header.write(write, version)?;
        }

        if has_multiple_headers {
            sequence_end::write(write)?;
        }

        Ok(())
    }

    pub fn read(read: &mut PeekRead<impl Read>, requirements: &Requirements) -> Result<Self> {
        let max_string_len = if requirements.has_long_names { 256 } else { 32 }; // TODO DRY this information
        let mut custom = SmallVec::new();

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

        while !sequence_end::has_come(read)? {
            let Attribute { name: attribute_name, value } = Attribute::read(read, max_string_len)?;

            use crate::meta::attributes::required::*;
            match attribute_name.bytes.as_slice() {
                TILES => tiles = Some(value.to_tile_description()?),
                NAME => name = Some(value.into_text()?),
                TYPE => kind = Some(Kind::parse(value.into_text()?)?),
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
                    positive_i32(value.to_i32()?, "max sample count")?
                ),

                CHUNKS => chunk_count = Some(
                    positive_i32(value.to_i32()?, "chunk count")?
                ),

                _ => {
                    // TODO lazy? only for user-specified names?
                    custom.push(Attribute { name: attribute_name, value })
                },
            }
        }

        let header = Header {
            channels: channels.ok_or(missing_attribute("channels"))?,
            compression: compression.ok_or(missing_attribute("compression"))?,
            data_window: data_window.ok_or(missing_attribute("data window"))?,
            display_window: display_window.ok_or(missing_attribute("display window"))?,
            line_order: line_order.ok_or(missing_attribute("line order"))?,
            pixel_aspect: pixel_aspect.ok_or(missing_attribute("pixel aspect"))?,
            screen_window_center: screen_window_center.ok_or(missing_attribute("screen window center"))?,
            screen_window_width: screen_window_width.ok_or(missing_attribute("screen window width"))??,

            tiles,
            name, kind,
            chunk_count,
            max_samples_per_pixel,
            deep_data_version: version,
            custom_attributes: custom,
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

            write_opt_attr(write, long, TILES, self.tiles, TileDescription)?;
            write_opt_attr(write, long, NAME, self.name.clone(), Text)?;
            write_opt_attr(write, long, TYPE, self.kind, Kind)?;
            write_opt_attr(write, long, VERSION, self.deep_data_version, I32)?;
            write_opt_attr(write, long, CHUNKS, self.chunk_count, |u| I32(u as i32))?;
            write_opt_attr(write, long, MAX_SAMPLES, self.max_samples_per_pixel, |u| I32(u as i32))?;
            write_attr(write, long, CHANNELS, self.channels.clone(), ChannelList)?; // FIXME do not clone
            write_attr(write, long, COMPRESSION, self.compression, Compression)?;
            write_attr(write, long, DATA_WINDOW, self.data_window, I32Box2)?;
            write_attr(write, long, DISPLAY_WINDOW, self.display_window, I32Box2)?;
            write_attr(write, long, LINE_ORDER, self.line_order, LineOrder)?;
            write_attr(write, long, PIXEL_ASPECT, self.pixel_aspect, F32)?;
            write_attr(write, long, WINDOW_WIDTH, self.screen_window_width, F32)?;
            write_attr(write, long, WINDOW_CENTER, self.screen_window_center, |(x, y)| F32Vec2(x, y))?;

            // FIXME always write chunk_count for faster read?
        }

        for attrib in &self.custom_attributes {
            attrib.write(write, version.has_long_names)?;
        }

        sequence_end::write(write)?;
        Ok(())
    }
}


impl Requirements {
    pub fn new(version: u8, header_count: usize, has_tiles: bool, long_names: bool, deep: bool) -> Self {
        Requirements {
            file_format_version: version,
            is_single_part_and_tiled: header_count == 1 && has_tiles,
            has_long_names: long_names,
            has_deep_data: deep, // TODO
            has_multiple_parts: header_count != 1
        }
    }


    /// this is actually used for control flow, as the number of headers may be 1 in a multipart file
    pub fn is_multipart(&self) -> bool {
        self.has_multiple_parts
    }

    /*pub fn byte_size(self) -> usize {
        0_u32.byte_size()
    }*/

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
    use crate::meta::attributes::{Text, ChannelList, I32Box2, LineOrder, Channel, PixelType};
    use crate::compression::Compression;

    #[test]
    fn round_trip_requirements() {
        let requirements = Requirements::new(2, 4, true, true, true);

        let mut data: Vec<u8> = Vec::new();
        requirements.write(&mut data).unwrap();
        let read = Requirements::read(&mut data.as_slice()).unwrap();
        assert_eq!(requirements, read);
    }

        #[test]
    fn round_trip(){
        let meta = MetaData {
            requirements: Requirements::new(2, 1, false, false, false),
            headers: smallvec![
                Header {
                    channels: ChannelList {
                        list: smallvec![
                            Channel {
                                name: Text::from_str("main").unwrap(),
                                pixel_type: PixelType::U32,
                                is_linear: false,
                                reserved: [0,0,0],
                                sampling: (1, 1)
                            }
                        ],
                        bytes_per_pixel: 4
                    },
                    compression: Compression::Uncompressed,
                    data_window: I32Box2 {
                        x_min: 0,
                        y_min: 0,
                        x_max: 10,
                        y_max: 10
                    },
                    display_window: I32Box2 {
                        x_min: 0,
                        y_min: 0,
                        x_max: 10,
                        y_max: 10
                    },
                    line_order: LineOrder::IncreasingY,
                    pixel_aspect: 1.0,
                    screen_window_center: (5.0, 5.0),
                    screen_window_width: 10.0,
                    tiles: None,
                    name: None,
                    kind: None,
                    deep_data_version: None,
                    chunk_count: None,
                    max_samples_per_pixel: None,
                    custom_attributes: smallvec![ /* TODO */ ]
                }
            ],
//            offset_tables: smallvec![
//                vec![
//                    0, 2, 3, 4, 5, 6, 7, 1234, 23, 412,4 ,124,4,
//                ]
//            ]
        };


        let mut data: Vec<u8> = Vec::new();
        meta.write(&mut data).unwrap();
        let meta2 = MetaData::read_from_buffered(data.as_slice()).unwrap();
        assert_eq!(meta, meta2);
    }
}

