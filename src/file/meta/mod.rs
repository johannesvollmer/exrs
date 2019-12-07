
pub mod attributes;

use crate::error::validity::*;
use super::io::*;

use ::smallvec::SmallVec;
use self::attributes::*;
use crate::file::data::{TileCoordinates, Block};
use crate::error::{ReadResult, WriteResult};


#[derive(Debug, Clone)]
pub struct MetaData {
    pub requirements: Requirements,

    /// separate header for each part, requires a null byte signalling the end of each header

    // TODO in validate, make sure that:
    /// The values of the displayWindow
    /// and pixelAspectRatio attributes must be the same for all parts of a file.
    /// if the headers include timeCode and chromaticities attributes, then the values of those
    /// attributes must also be the same for all parts of a file.
    pub headers: Headers,

    /// one table per header.
    /// In the table, scan line offsets are ordered according to increasing scan line y coordinates.
    /// In the table, tile offsets are sorted the same way as tiles in INCREASING_Y order.
    pub offset_tables: OffsetTables,
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
    pub chunk_count: Option<i32>,

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
    pub max_samples_per_pixel: Option<i32>,

    /// Requires a null byte signalling the end of each attribute
    /// Contains custom attributes
    pub custom_attributes: SmallVec<[Attribute; 6]>,
}


// FIXME this merely reports the features which must be supported by our library,
// FIXME do not use these features to control the flow of the program, use the attributes instead!
// check these once while reading, remove those fields from this struct otherwise

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct Requirements {
    /// is currently 2
    file_format_version: u8,

    /// bit 9
    /// if true: single-part tiles (bits 11 and 12 must be 0).
    /// if false and 11 and 12 are false: single-part scan-line.
    is_single_tile: bool,

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

        let is_multi_part = headers > 1;
        /*if is_multi_part != self.requirements.has_multiple_parts {
            return Err(Invalid::Combination(&[
                Value::Version("multipart"),
                Value::Part("multipart"),
            ]));
        }*/

        // TODO
        // The values of the displayWindow
        // and pixelAspectRatio attributes must be the same for all parts of a file.

        self.requirements.validate()?;
        for header in &self.headers {
            header.validate(is_multi_part)?;
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

    pub fn has_tiles(&self) -> bool {
        match self.kind {
            Some(Kind::DeepTile) | Some(Kind::Tile) => {
                debug_assert!(self.tiles.is_some());
                true
            },
            _ => false
        }
    }

    pub fn get_raw_block_coordinates(&self, block: &Block) -> ReadResult<I32Box2> {
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

            _ => unimplemented!()
        })
    }

    pub fn get_block_data_indices(&self, block: &Block) -> ReadResult<TileIndices> {
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
                _ => unimplemented!()
            },

            position,
            size
        })
    }

    fn get_scan_line_block_height(&self, y: u32) -> u32 {
        let lines_per_block = self.compression.scan_lines_per_block();
        let data_height = self.data_window.dimensions().1;

        // find out how much the last scan block would be cut off (or 0):
        let block_end = y + lines_per_block;
        let clip = block_end.checked_sub(data_height).unwrap_or(0);

        lines_per_block - clip
    }

    fn get_tile_size(&self, tile: TileCoordinates) -> (u32, u32) {
        let tiles = self.tiles.expect("check failed: tiles not found");

        let (data_width, data_height) = self.data_window.dimensions();
        let default_width = tiles.x_size;
        let default_height = tiles.y_size;
        let round = tiles.rounding_mode;

        // FIXME is the level required here or not?? indices should always start at 0 and not exceed bounds
        let level_x = tile.level_x;
        let level_data_width = compute_level_size(round, data_width as u32, level_x as u32);

        let default_right = tile.tile_x as u32 + default_width;
        let right_overflow = default_right.checked_sub(level_data_width).unwrap_or(0);

        let level_y = tile.level_y;
        let level_data_height = compute_level_size(round, data_height as u32, level_y as u32);

        assert!(level_x == 1 && level_y == 1, "unimplemented: tiled levels data unpacking");

        let default_bottom = tile.tile_y as u32 + default_height;
        let bottom_overflow = default_bottom.checked_sub(level_data_height).unwrap_or(0);

        let width = default_width - right_overflow;
        let height = default_height - bottom_overflow;
        (width, height)
    }

    // TODO for all other fields too?
    pub fn kind_or_err(&self) -> Result<&Kind, Invalid> {
        self.kind.as_ref().ok_or(Invalid::Missing(Value::Attribute("kind")))
    }

    pub fn validate(&self, is_multipart: bool) -> Validity {
        if is_multipart {
            if self.chunk_count.is_none() {
                return Err(Invalid::Missing(Value::Attribute("chunkCount (for multipart)")).into());
            }
            if self.kind.is_none() {
                return Err(Invalid::Missing(Value::Attribute("type (for multipart)")).into());
            }
            if self.name.is_none() {
                return Err(Invalid::Missing(Value::Attribute("name (for multipart)")).into());
            }
        }

        if self.has_deep_data() {
            if self.chunk_count.is_none() {
                return Err(Invalid::Missing(Value::Attribute("chunkCount (for deepdata)")).into());
            }
            if self.name.is_none() {
                return Err(Invalid::Missing(Value::Attribute("name (for deepdata)")).into());
            }
            if self.deep_data_version.is_none() {
                return Err(Invalid::Missing(Value::Attribute("version (for deepdata)")).into());
            }

            if self.deep_data_version != Some(1) {
                return Err(Invalid::NotSupported("deep data version other than 1"));
            }

            // make maxSamplesPerPixel optional because some files don't have it
            /*if self.indices.max_samples_per_pixel.is_none() {
                return Err(Invalid::Missing(Value::Attribute("maxSamplesPerPixel (for deepdata)")).into());
            }*/

            let compression = self.compression; // attribute is already checked
            if !compression.supports_deep_data() {
                return Err(Invalid::Content(
                    Value::Attribute("compression (for deepdata)"),
                    Required::OneOf(&["none", "rle", "zips", "zip"])
                ).into());
            }
        }

        if self.has_tiles() {
            if self.tiles.is_none() {
                return Err(Invalid::Missing(Value::Attribute("tiles (for tiledimage or deeptiles)")).into());
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

    pub fn write_all<W: Write>(headers: &Headers, write: &mut W, version: Requirements) -> WriteResult {
        let has_multiple_headers = headers.len() != 1;
        if headers.is_empty() || version.has_multiple_parts != has_multiple_headers {
            // TODO return combination?
            return Err(Invalid::Content(Value::Part("headers count"), Required::Exact("1")).into());
        }

        for header in headers {
            debug_assert!(header.validate(headers.len() > 1).is_ok(), "check failed: header invalid");

            // header.tiles.write(write, version.has_long_names)?;
            println!("FIXME write all header attributes!!!");

            for attrib in &header.custom_attributes {
                attrib.write(write, version.has_long_names)?;
            }

            SequenceEnd::write(write)?;

        }
        SequenceEnd::write(write)?;

        Ok(())
    }

    pub fn read_all(read: &mut PeekRead<impl Read>, version: Requirements) -> ReadResult<Headers> {
        Ok({
            if !version.has_multiple_parts { // TODO check a different way?
                SmallVec::from_elem(Header::read(read, false)?, 1)

            } else {
                let mut headers = SmallVec::new();
                while !SequenceEnd::has_come(read)? {
                    headers.push(Header::read(read, true)?);
                }

                headers
            }
        })
    }

    pub fn read(read: &mut PeekRead<impl Read>, is_multipart: bool) -> ReadResult<Self> {
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

        while !SequenceEnd::has_come(read)? {
            let Attribute { name: attribute_name, value } = Attribute::read(read)?;

            use crate::file::meta::attributes::required::*;
            match attribute_name.bytes.as_slice() {
                TILES => tiles = Some(value.to_tile_description()?),
                NAME => name = Some(value.to_text()?),
                TYPE => kind = Some(Kind::parse(value.to_text()?)?),
                VERSION => version = Some(value.to_i32()?),
                CHUNKS => chunk_count = Some(value.to_i32()?),
                MAX_SAMPLES => max_samples_per_pixel = Some(value.to_i32()?),
                CHANNELS => channels = Some(value.to_channel_list()?),
                COMPRESSION => compression = Some(value.to_compression()?),
                DATA_WINDOW => data_window = Some(value.to_i32_box_2()?),
                DISPLAY_WINDOW => display_window = Some(value.to_i32_box_2()?),
                LINE_ORDER => line_order = Some(value.to_line_order()?),
                PIXEL_ASPECT => pixel_aspect = Some(value.to_f32()?),
                WINDOW_CENTER => screen_window_center = Some(value.to_f32_vec_2()?),
                WINDOW_WIDTH => screen_window_width = Some(value.to_f32()),

                _ => {
                    // TODO lazy? only for user-specified names?
                    custom.push(Attribute { name: attribute_name, value })
                },
            }
        }

        let header = Header {
            channels: channels.ok_or(Invalid::Missing(Value::Attribute("channels")))?,
            compression: compression.ok_or(Invalid::Missing(Value::Attribute("compression")))?,
            data_window: data_window.ok_or(Invalid::Missing(Value::Attribute("data_window")))?,
            display_window: display_window.ok_or(Invalid::Missing(Value::Attribute("display_window")))?,
            line_order: line_order.ok_or(Invalid::Missing(Value::Attribute("line_order")))?,
            pixel_aspect: pixel_aspect.ok_or(Invalid::Missing(Value::Attribute("pixel_aspect")))?,
            screen_window_center: screen_window_center.ok_or(Invalid::Missing(Value::Attribute("screen_window_center")))?,
            screen_window_width: screen_window_width.ok_or(Invalid::Missing(Value::Attribute("screen_window_width")))??,

            tiles,
            name, kind,
            deep_data_version: version, chunk_count,
            max_samples_per_pixel,
            custom_attributes: custom,
        };

        header.validate(is_multipart)?;
        Ok(header)
    }
}


impl Requirements {
    /// this is actually used for control flow, as the number of headers may be 1 in a multipart file
    pub fn is_multipart(&self) -> bool {
        self.has_multiple_parts
    }

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

        let version = Requirements {
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
        if let 1..=2 = self.file_format_version {

            match (
                self.is_single_tile, self.has_deep_data, self.has_multiple_parts,
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

                _ => Err(Invalid::Combination(&[
                    Value::Version("is_single_tile"),
                    Value::Version("has_long_names"),
                    Value::Version("has_deep_data"),
                    Value::Version("format_version"),
                ]))
            }
        }
        else {
            Err(Invalid::Content(
                Value::Version("file_format_number"),
                Required::Range { min: 1, max: 2, })
            )
        }

    }
}



impl MetaData {
    pub fn write<W: Write>(&self, write: &mut W) -> WriteResult {
        self.validate()?;

        MagicNumber::write(write)?;
        self.requirements.write(write)?;
        Header::write_all(&self.headers, write, self.requirements)?;

        println!("calculate tables???");
        write_offset_tables(write, &self.offset_tables)
    }

    pub fn read_unvalidated(read: &mut PeekRead<impl Read>) -> ReadResult<Self> {
        MagicNumber::validate_exr(read)?;
        let version = Requirements::read(read)?;
        let headers = Header::read_all(read, version)?;
        let offset_tables = read_offset_tables(read, version, &headers)?;

        // TODO check if supporting version 2 implies supporting version 1
        Ok(MetaData { requirements: version, headers, offset_tables })
    }

    pub fn read_validated(read: &mut PeekRead<impl Read>) -> ReadResult<Self> {
        let meta = Self::read_unvalidated(read)?;
        meta.validate()?;
        Ok(meta)
    }
}



// calculations inspired by
// https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfTiledMisc.cpp

pub fn compute_tile_count(full_res: u32, tile_size: u32) -> u32 {
    // round up, because if the image is not evenly divisible by the tiles,
    // we add another tile at the end (which is only partially used)
    RoundingMode::Up.divide(full_res, tile_size)
}

pub fn compute_scan_line_block_count(height: u32, block_size: u32) -> u32 {
    // round up, because if the image is not evenly divisible by the block size,
    // we add another block at the end (which is only partially used)
    RoundingMode::Up.divide(height, block_size)
}

// TODO this should be cached? log2 may be very expensive
pub fn compute_level_count(round: RoundingMode, full_res: u32) -> u32 {
    round.log2(full_res) + 1
}

pub fn compute_level_size(round: RoundingMode, full_res: u32, level_index: u32) -> u32 {
    round.divide(full_res,  1 << level_index).max(1)
}

// TODO reuse this algorithm in crate::image::Part::new?
pub fn compute_offset_table_size(version: Requirements, header: &Header) -> ReadResult<u32> {
    if let Some(chunk_count) = header.chunk_count {
        Ok(chunk_count as u32) // TODO will this panic on negative number / invalid data?

    } else {
        debug_assert!(!version.has_multiple_parts, "check failed: chunkCount missing for multi-part image");

        // If not multipart and chunkCount not present,
        // the number of entries in the chunk table is computed
        // using the dataWindow and tileDesc attributes and the compression format
        let compression = header.compression;
        let data_window = header.data_window;
        data_window.validate()?;

        let (data_width, data_height) = data_window.dimensions();

        if let Some(tiles) = header.tiles {
            let round = tiles.rounding_mode;
            let (tile_width, tile_height) = tiles.dimensions();

            let level_count = |full_res: u32| {
                compute_level_count(round, full_res)
            };

            let level_size = |full_res: u32, level_index: u32| {
                compute_level_size(round, full_res, level_index)
            };

            // TODO cache all these level values??
            use crate::file::meta::attributes::LevelMode::*;
            Ok(match tiles.level_mode {
                Singular => {
                    let tiles_x = compute_tile_count(data_width, tile_width);
                    let tiles_y = compute_tile_count(data_height, tile_height);
                    tiles_x * tiles_y
                }

                MipMap => {
                    // sum all tiles per level
                    // note: as levels shrink, tiles stay the same pixel size.
                    // so at lower levels, tiles cover up a visually bigger are of the smaller resolution image
                    (0..level_count(data_width.max(data_height))).map(|level|{
                        let tiles_x = compute_tile_count(level_size(data_width, level), tile_width);
                        let tiles_y = compute_tile_count(level_size(data_height, level), tile_height);
                        tiles_x * tiles_y
                    }).sum()
                },

                RipMap => {
                    // TODO test this
                    (0..level_count(data_width)).map(|x_level|{ // TODO may swap y and x?
                        (0..level_count(data_height)).map(|y_level| {
                            let tiles_x = compute_tile_count(level_size(data_width, x_level), tile_width);
                            let tiles_y = compute_tile_count(level_size(data_height, y_level), tile_height);
                            tiles_x * tiles_y
                        }).sum::<u32>()
                    }).sum()
                }
            })

        }

        // scan line blocks never have mip maps // TODO check if this is true
        else {
            Ok(compute_scan_line_block_count(data_height, compression.scan_lines_per_block() as u32))
        }
    }
}


// TODO make instance fn
pub fn read_offset_table(
    read: &mut PeekRead<impl Read>, version: Requirements, header: &Header
) -> ReadResult<OffsetTable>
{
    let entry_count = compute_offset_table_size(version, header)?;
    read_u64_vec(read, entry_count as usize, ::std::u16::MAX as usize)
}


fn read_offset_tables(
    read: &mut PeekRead<impl Read>, version: Requirements, headers: &Headers,
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