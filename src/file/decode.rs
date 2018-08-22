//! Contains Error type definitions and
//! all the functions that can only be used to decode an image

use ::std::io::{Read, Seek, SeekFrom};
use ::seek_bufread::BufReader as SeekBufRead;
use ::byteorder::{LittleEndian, ReadBytesExt};
use ::bit_field::BitField;
use ::smallvec::SmallVec;

use super::*;
use super::blocks::*;
use ::image::attributes::*;



pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    NotEXR,
    Invalid(&'static str),
    Missing(&'static str),
    UnknownAttributeType { bytes_to_skip: u32 },

    IoError(::std::io::Error),
    CompressionError(::image::compress::Error),

    NotSupported(&'static str),
}

/// Enable using the `?` operator on io::Result
impl From<::std::io::Error> for Error {
    fn from(io_err: ::std::io::Error) -> Self {
        panic!("give me that nice stack trace like you always do"); // TODO remove
        Error::IoError(io_err)
    }
}

/// Enable using the `?` operator on compress::Result
impl From<::image::compress::Error> for Error {
    fn from(compress_err: ::image::compress::Error) -> Self {
        Error::CompressionError(compress_err)
    }
}








fn identify_exr<R: Read>(read: &mut R) -> Result<bool> {
    let mut magic_num = [0; 4];
    read.read_exact(&mut magic_num)?;
    Ok(magic_num == self::MAGIC_NUMBER)
}

fn skip_identification_bytes<R: Read>(read: &mut R) -> Result<()> {
    if identify_exr(read)? {
        Ok(())

    } else {
        Err(Error::NotEXR)
    }
}

fn version<R: ReadBytesExt>(read: &mut R) -> Result<Version> {
    let version_and_flags = read.read_i32::<LittleEndian>()?;

    // take the 8 least significant bits, they contain the file format version number
    let version = (version_and_flags & 0x000F) as u8;

    // the 24 most significant bits are treated as a set of boolean flags
    let is_single_tile = version_and_flags.get_bit(9);
    let has_long_names = version_and_flags.get_bit(10);
    let has_deep_data = version_and_flags.get_bit(11);
    let has_multiple_parts = version_and_flags.get_bit(12);
    // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0

    Ok(Version {
        file_format_version: version,
        is_single_tile, has_long_names,
        has_deep_data, has_multiple_parts,
    })
}

/// `peek` the next byte, and consume it if it is 0
fn skip_null_byte_if_present<R: Read + Seek>(read: &mut SeekBufRead<R>) -> Result<bool> {
    if read_u8(read)? == 0 {
        Ok(true)

    } else {
        // go back that wasted byte because its not 0
        // TODO benchmark peeking the buffer performance
        read.seek(SeekFrom::Current(-1))?;
        Ok(false)
    }
}


fn read_u8<R: ReadBytesExt>(read: &mut R) -> Result<u8> {
    read.read_u8().map_err(Error::from)
}

fn read_i32<R: ReadBytesExt>(read: &mut R) -> Result<i32> {
    read.read_i32::<LittleEndian>().map_err(Error::from)
}

fn read_f32<R: ReadBytesExt>(read: &mut R) -> Result<f32> {
    read.read_f32::<LittleEndian>().map_err(Error::from)
}

fn read_u32<R: ReadBytesExt>(read: &mut R) -> Result<u32> {
    read.read_u32::<LittleEndian>().map_err(Error::from)
}

fn read_u64<R: ReadBytesExt>(read: &mut R) -> Result<u64> {
    read.read_u64::<LittleEndian>().map_err(Error::from)
}

fn read_f64<R: ReadBytesExt>(read: &mut R) -> Result<f64> {
    read.read_f64::<LittleEndian>().map_err(Error::from)
}

fn null_terminated_text<R: ReadBytesExt>(read: &mut R) -> Result<Text> {
    let mut bytes = SmallVec::new();

    loop {
        match read_u8(read)? {
            0 => break,
            non_terminator => bytes.push(non_terminator),
        }
    }

    Ok(Text { bytes })
}

fn i32_sized_text<R: Read + Seek>(read: &mut SeekBufRead<R>, expected_attribute_bytes: Option<u32>) -> Result<Text> {
    let string_byte_length = expected_attribute_bytes
        .map(|u| Ok(u as i32)) // use expected attribute bytes if known,
        .unwrap_or_else(|| read_i32(read))?
        as usize; // or read from bytes otherwise

    let bytes = large_byte_vec(read, string_byte_length, 1024)?;

    // using a regular vec is ok because this is only for some rare string attributes
    Ok(Text::from_bytes(SmallVec::from_vec(bytes)))
}

fn box2i<R: Read>(read: &mut R) -> Result<I32Box2> {
    Ok(I32Box2 {
        x_min: read_i32(read)?, y_min: read_i32(read)?,
        x_max: read_i32(read)?, y_max: read_i32(read)?,
    })
}

fn box2f<R: Read>(read: &mut R) -> Result<F32Box2> {
    Ok(F32Box2 {
        x_min: read_f32(read)?, y_min: read_f32(read)?,
        x_max: read_f32(read)?, y_max: read_f32(read)?,
    })
}

fn channel<R: Read + Seek>(read: &mut SeekBufRead<R>) -> Result<Channel> {
    let name = null_terminated_text(read)?;

    let pixel_type = match read_i32(read)? {
        0 => PixelType::U32,
        1 => PixelType::F16,
        2 => PixelType::F32,
        _ => return Err(Error::Invalid("pixelType"))
    };

    let is_linear = match read_u8(read)? {
        1 => true,
        0 => false,
        _ => return Err(Error::Invalid("pLinear"))
    };

    let reserved = [
        read.read_i8()?,
        read.read_i8()?,
        read.read_i8()?,
    ];

    let x_sampling = read_i32(read)?;
    let y_sampling = read_i32(read)?;

    Ok(Channel {
        name, pixel_type, is_linear,
        reserved, x_sampling, y_sampling,
    })
}

fn channel_list<R: Read + Seek>(read: &mut SeekBufRead<R>) -> Result<ChannelList> {
    let mut channels = SmallVec::new();
    while !skip_null_byte_if_present(read)? {
        channels.push(channel(read)?);
    }

    Ok(channels)
}

fn chromaticities<R: Read>(read: &mut R) -> Result<Chromaticities> {
    Ok(Chromaticities {
        red_x:   read_f32(read)?,   red_y:   read_f32(read)?,
        green_x: read_f32(read)?,   green_y: read_f32(read)?,
        blue_x:  read_f32(read)?,   blue_y:  read_f32(read)?,
        white_x: read_f32(read)?,   white_y: read_f32(read)?,
    })
}

fn compression<R: Read>(read: &mut R) -> Result<Compression> {
    use ::image::attributes::Compression::*;
    Ok(match read_u8(read)? {
        0 => None,
        1 => RLE,
        2 => ZIPSingle,
        3 => ZIP,
        4 => PIZ,
        5 => PXR24,
        6 => B44,
        7 => B44A,
        _ => return Err(Error::Invalid("compression")),
    })
}

fn environment_map<R: Read>(read: &mut R) -> Result<EnvironmentMap> {
    Ok(match read_u8(read)? {
        0 => EnvironmentMap::LatitudeLongitude,
        1 => EnvironmentMap::Cube,
        _ => return Err(Error::Invalid("environment map"))
    })
}

fn key_code<R: Read>(read: &mut R) -> Result<KeyCode> {
    Ok(KeyCode {
        film_manufacturer_code: read_i32(read)?,
        film_type: read_i32(read)?,
        film_roll_prefix: read_i32(read)?,
        count: read_i32(read)?,
        perforation_offset: read_i32(read)?,
        perforations_per_frame: read_i32(read)?,
        perforations_per_count: read_i32(read)?,
    })
}

fn line_order<R: Read>(read: &mut R) -> Result<LineOrder> {
    use ::image::attributes::LineOrder::*;
    Ok(match read_u8(read)? {
        0 => IncreasingY,
        1 => DecreasingY,
        2 => RandomY,
        _ => return Err(Error::Invalid("line order")),
    })
}


fn f32_matrix_3x3<R: Read>(read: &mut R) -> Result<[f32; 9]> {
    let mut result = [0.0; 9];
    read.read_f32_into::<LittleEndian>(&mut result)?;
    Ok(result)
}

fn f32_matrix_4x4<R: Read>(read: &mut R) -> Result<[f32; 16]> {
    let mut result = [0.0; 16];
    read.read_f32_into::<LittleEndian>(&mut result)?;
    Ok(result)
}

fn i32_sized_text_vector<R: Read + Seek>(read: &mut SeekBufRead<R>, attribute_value_byte_size: u32) -> Result<Vec<Text>> {
    let mut result = Vec::with_capacity(2);
    let mut processed_bytes = 0_usize;

    while processed_bytes < attribute_value_byte_size as usize {
        let text = i32_sized_text(read, None)?;
        processed_bytes += ::std::mem::size_of::<i32>(); // size i32 of the text
        processed_bytes += text.bytes.len();
        result.push(text);
    }

    debug_assert_eq!(processed_bytes, attribute_value_byte_size as usize);
    Ok(result)
}

fn preview<R: Read>(read: &mut R) -> Result<Preview> {
    let width = read_u32(read)?;
    let height = read_u32(read)?;
    let components_per_pixel = 4;

    // TODO should be seen as char, not unsigned char!
    let mut pixel_data = vec![0_u8; (width * height * components_per_pixel) as usize];

    // TODO don't blindly allocate too much memory
    read.read_exact(&mut pixel_data)?;

    Ok(Preview {
        width, height,
        pixel_data,
    })
}

fn tile_description<R: Read>(read: &mut R) -> Result<TileDescription> {
    let x_size = read_u32(read)?;
    let y_size = read_u32(read)?;

    // mode = level_mode + (rounding_mode * 16)
    let mode = read_u8(read)?;

    let level_mode = mode & 0b00001111; // wow that works
    let rounding_mode = mode >> 4; // wow that works

    let level_mode = match level_mode {
        0 => LevelMode::One,
        1 => LevelMode::MipMap,
        2 => LevelMode::RipMap,
        _ => return Err(Error::Invalid("level mode"))
    };

    let rounding_mode = match rounding_mode {
        0 => RoundingMode::Down,
        1 => RoundingMode::Up,
        _ => return Err(Error::Invalid("rounding mode"))
    };

    Ok(TileDescription { x_size, y_size, level_mode, rounding_mode, })
}


fn attribute_value<R: Read + Seek>(read: &mut SeekBufRead<R>, kind: &Text, byte_size: u32) -> Result<AttributeValue> {
    Ok(match kind.bytes.as_slice() {
        // TODO replace these literals with constants
        b"box2i" => AttributeValue::I32Box2(box2i(read)?),
        b"box2f" => AttributeValue::F32Box2(box2f(read)?),

        b"int"    => AttributeValue::I32(read_i32(read)?),
        b"float"  => AttributeValue::F32(read_f32(read)?),
        b"double" => AttributeValue::F64(read_f64(read)?),

        b"rational" => AttributeValue::Rational(read_i32(read)?, read_u32(read)?),
        b"timecode" => AttributeValue::TimeCode(read_u32(read)?, read_u32(read)?),

        b"v2i" => AttributeValue::I32Vec2(read_i32(read)?, read_i32(read)?),
        b"v2f" => AttributeValue::F32Vec2(read_f32(read)?, read_f32(read)?),
        b"v3i" => AttributeValue::I32Vec3(read_i32(read)?, read_i32(read)?, read_i32(read)?),
        b"v3f" => AttributeValue::F32Vec3(read_f32(read)?, read_f32(read)?, read_f32(read)?),

        b"chlist" => AttributeValue::ChannelList(channel_list(read)?),
        b"chromaticities" => AttributeValue::Chromaticities(chromaticities(read)?),
        b"compression" => AttributeValue::Compression(compression(read)?),
        b"envmap" => AttributeValue::EnvironmentMap(environment_map(read)?),

        b"keycode" => AttributeValue::KeyCode(key_code(read)?),
        b"lineOrder" => AttributeValue::LineOrder(line_order(read)?),

        b"m33f" => AttributeValue::F32Matrix3x3(f32_matrix_3x3(read)?),
        b"m44f" => AttributeValue::F32Matrix4x4(f32_matrix_4x4(read)?),

        b"preview" => AttributeValue::Preview(preview(read)?),
        b"string" => AttributeValue::Text(ParsedText::parse(i32_sized_text(read, Some(byte_size))?)),
        b"stringvector" => AttributeValue::TextVector(i32_sized_text_vector(read, byte_size)?),
        b"tiledesc" => AttributeValue::TileDescription(tile_description(read)?),

        _ => {
            println!("Unknown attribute type: {:?}", kind.to_string());
            return Err(Error::UnknownAttributeType { bytes_to_skip: byte_size as u32 })
        }
    })
}

// TODO parse lazily, skip size, ...
fn attribute<R: Read + Seek>(read: &mut SeekBufRead<R>) -> Result<Attribute> {
    let name = null_terminated_text(read)?;
    let kind = null_terminated_text(read)?;
    let size = read_i32(read)? as u32; // TODO .checked_cast.ok_or(err:negative)
    let value = attribute_value(read, &kind, size)?;
    Ok(Attribute { name, kind, value, })
}

fn header<R: Seek + Read>(read: &mut SeekBufRead<R>, file_version: Version) -> Result<Header> {
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


    while !skip_null_byte_if_present(read)? {
        match attribute(read) {
            // skip unknown attribute values
            Err(Error::UnknownAttributeType { bytes_to_skip }) => {
                read.seek(SeekFrom::Current(bytes_to_skip as i64))?;
            },

            Err(other_error) => return Err(other_error),

            Ok(attribute) => {
                // save index when a required attribute is encountered
                let index = attributes.len();

                // TODO replace these literals with constants
                use ::image::attributes::required::*;
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

                match attribute.kind.bytes.as_slice() {
                    b"chromaticities" => chromaticities.push(index),
                    _ => {},
                }

                attributes.push(attribute)
            }
        }
    }

    let header = Header {
        attributes,
        indices: AttributeIndices {
            channels: channels.ok_or(Error::Missing("channels"))?,
            compression: compression.ok_or(Error::Missing("compression"))?,
            data_window: data_window.ok_or(Error::Missing("data window"))?,
            display_window: display_window.ok_or(Error::Missing("display window"))?,
            line_order: line_order.ok_or(Error::Missing("line order"))?,
            pixel_aspect: pixel_aspect.ok_or(Error::Missing("pixel aspect ratio"))?,
            screen_window_center: screen_window_center.ok_or(Error::Missing("screen window center"))?,
            screen_window_width: screen_window_width.ok_or(Error::Missing("screen window width"))?,

            chromaticities,

            tiles, name, kind,
            version, chunk_count,
            max_samples_per_pixel,
        },
    };

    header.check_validity(file_version)?;
    Ok(header)
}

fn headers<R: Seek + Read>(read: &mut SeekBufRead<R>, version: Version) -> Result<Headers> {
    Ok({
        if !version.has_multiple_parts {
            SmallVec::from_elem(header(read, version)?, 1)

        } else {
            let mut headers = SmallVec::new();
            while !skip_null_byte_if_present(read)? {
                headers.push(header(read, version)?);
            }

            headers
        }
    })
}

fn offset_table<R: Seek + Read>(
    read: &mut SeekBufRead<R>,
    version: Version, header: &Header
) -> Result<OffsetTable>
{
    let entry_count = {
        if let Some(chunk_count_index) = header.indices.chunk_count {
            if let &AttributeValue::I32(chunk_count) = &header.attributes[chunk_count_index].value {
                chunk_count // TODO will this panic on negative number / invalid data?

            } else {
                return Err(Error::Invalid("chunkCount type"))
            }
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
            data_window.check_validity()?;

            let (data_width, data_height) = data_window.dimensions();

            if let Some(tiles) = header.tiles() {
                let (tile_width, tile_height) = tiles.dimensions();
                let tile_width =  tile_width as i32;
                let tile_height =  tile_height as i32;

                fn tile_count(image_len: i32, tile_len: i32) -> i32 {
                    // round up, because if the image is not evenly divisible by the tiles,
                    // we add another tile at the and that is not fully used
                    RoundingMode::Up.divide(image_len as u32, tile_len as u32) as i32
                }

                let full_res_tile_count = {
                    let tiles_x = tile_count(data_width, tile_width);
                    let tiles_y = tile_count(data_height, tile_height);
                    tiles_x * tiles_y
                };

                use ::image::attributes::LevelMode::*;
                match tiles.level_mode {
                    One => {
                        full_res_tile_count
                    },

                    // I can't believe that this works
                    MipMap => {
                        // TODO simplify the whole calculation
                        let mut line_offset_size = full_res_tile_count;
                        let round = tiles.rounding_mode;

                        let mut mip_map_level_width = data_width;
                        let mut mip_map_level_height = data_height;

                        // add mip maps tiles
                        loop {
                            // is that really how you compute mip map resolution levels?
                            mip_map_level_width = round.divide(mip_map_level_width as u32, 2).max(1) as i32;
                            mip_map_level_height = round.divide(mip_map_level_height as u32, 2).max(1) as i32; // new mip map resulution, never smaller than 1

                            let tiles_x = tile_count(mip_map_level_width, tile_width);
                            let tiles_y = tile_count(mip_map_level_height, tile_height);
                            line_offset_size += tiles_x * tiles_y;

                            if mip_map_level_width == 1 && mip_map_level_height == 1 {
                                break;
                            }
                        }

                        line_offset_size
                    },

                    // I can't believe that this works either
                    RipMap => {
                        // TODO simplify the whole calculation
                        let mut line_offset_size = 0;
                        let round = tiles.rounding_mode;

                        let mut rip_map_level_width = data_width * 2; // x2 to include fullres, because the beginning of the loop divides
                        let mut rip_map_level_height = data_height * 2; // x2 to include fullres, because the beginning of the loop divides

                        // add all rip maps tiles
                        'y: loop {
                            // new rip map height, vertically resized, never smaller than 1
                            rip_map_level_height = round.divide(rip_map_level_height as u32, 2).max(1) as i32;

                            // add all rip maps tiles with that specific outer height
                            'x: loop {
                                // new rip map width, horizontally resized, never smaller than 1
                                rip_map_level_width = round.divide(rip_map_level_width as u32, 2).max(1) as i32;

                                let tiles_x = tile_count(rip_map_level_width, tile_width);
                                let tiles_y = tile_count(rip_map_level_height, tile_height);
                                line_offset_size += tiles_x * tiles_y;

                                if rip_map_level_width == 1 {
                                    rip_map_level_width = data_width * 2; // x2 to include fullres, because the beginning of the loop divides
                                    break 'x;
                                }
                            }

                            if rip_map_level_height == 1 {
                                break 'y;
                            }
                        }

                        line_offset_size
                    }
                }

            } else { // scanlines
                let lines_per_block = compression.scan_lines_per_block() as i32;
                (data_height + lines_per_block) / lines_per_block
            }
        }
    };

    let suspicious_limit = ::std::u16::MAX as i32;
    if entry_count < suspicious_limit {
        let mut offsets = vec![0; entry_count as usize];
        read.read_u64_into::<LittleEndian>(&mut offsets)?;
        Ok(offsets)

    } else {
        // avoid allocating too much memory in fear of an
        // incorrectly decoded entry_count, hoping the end of the file comes comes soon
        let mut offsets = vec![0; suspicious_limit as usize];
        read.read_u64_into::<LittleEndian>(&mut offsets)?;

        for _ in suspicious_limit..entry_count {
            offsets.push(read_u64(read)?);
        }

        Ok(offsets)
    }

}

// TODO offset tables are only for multipart files???
fn offset_tables<R: Seek + Read>(
    read: &mut SeekBufRead<R>,
    version: Version, headers: &Headers,
) -> Result<OffsetTables>
{
    let mut tables = SmallVec::new();

    for i in 0..headers.len() {
        // one offset table for each header
        tables.push(offset_table(read, version, &headers[i])?);
    }

    Ok(tables)
}

fn large_byte_vec<R: Seek + Read>(read: &mut SeekBufRead<R>, data_size: usize, estimated_max: usize) -> Result<Vec<u8>> {
    if data_size < estimated_max {
        let mut data = vec![0; data_size];
        read.read_exact(&mut data)?;
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
            data.push(read_u8(read)?);
        }

        Ok(data)
    }
}

fn i32_sized_byte_vec<R: Seek + Read>(read: &mut SeekBufRead<R>, estimated_max: usize) -> Result<Vec<u8>> {
    let data_size = read_i32(read)? as usize;
    large_byte_vec(read, data_size, estimated_max)
}

fn tile_coordinates<R: Read>(read: &mut R) -> Result<TileCoordinates> {
    Ok(TileCoordinates {
        tile_x: read_i32(read)?,
        tile_y: read_i32(read)?,
        level_x: read_i32(read)?,
        level_y: read_i32(read)?,
    })
}

/// If a block length greater than this number is decoded,
/// it will not try to allocate that much memory, but instead consider
/// that decoding the block length has gone wrong
const MAX_PIXEL_BYTES: usize = 1048576; // 2^20

fn scan_line_block<R: Seek + Read>(read: &mut SeekBufRead<R>) -> Result<ScanLineBlock> {
    let y_coordinate = read_i32(read)?;
    let pixels = FlatPixelData::Compressed(i32_sized_byte_vec(read, MAX_PIXEL_BYTES)?); // TODO maximum scan line size can easily be calculated
    Ok(ScanLineBlock { y_coordinate, pixels })
}

fn tile_block<R: Seek + Read>(
    read: &mut SeekBufRead<R>
) -> Result<TileBlock>
{
    let coordinates = tile_coordinates(read)?;
    let pixels = FlatPixelData::Compressed(i32_sized_byte_vec(read, MAX_PIXEL_BYTES)?);// TODO maximum tile size can easily be calculated
    Ok(TileBlock { coordinates, pixels, })
}

fn deep_scan_line_block<R: Seek + Read>(
    read: &mut SeekBufRead<R>
) -> Result<DeepScanLineBlock>
{
    let y_coordinate = read_i32(read)?;
    let compressed_pixel_offset_table_size = read_i32(read)? as usize;
    let compressed_sample_data_size = read_u64(read)? as usize; // TODO u64 just guessed
    let decompressed_sample_data_size = read_u64(read)?;

    // TODO dont blindly allocate?
    let mut compressed_pixel_offset_table = Vec::with_capacity(compressed_pixel_offset_table_size);
    for _ in 0..compressed_pixel_offset_table_size {
        compressed_pixel_offset_table.push(read_i32(read)?);
    }

    let compressed_sample_data = large_byte_vec(
        read, compressed_sample_data_size, MAX_PIXEL_BYTES
    )?;

    Ok(DeepScanLineBlock {
        y_coordinate,
        decompressed_sample_data_size,
        compressed_pixel_offset_table,
        compressed_sample_data,
    })
}

fn deep_tile_block<R: Seek + Read>(
    read: &mut SeekBufRead<R>
) -> Result<DeepTileBlock>
{
    let coordinates = tile_coordinates(read)?;
    let compressed_pixel_offset_table_size = read_i32(read)? as usize;
    let compressed_sample_data_size = read_u64(read)? as usize; // TODO u64 just guessed
    let decompressed_sample_data_size = read_u64(read)?;

    // TODO dont blindly allocate?
    let mut compressed_pixel_offset_table = Vec::with_capacity(compressed_pixel_offset_table_size);
    for _ in 0..compressed_pixel_offset_table_size {
        compressed_pixel_offset_table.push(read_i32(read)?);
    }

    let compressed_sample_data = large_byte_vec(
        read, compressed_sample_data_size, ::std::u16::MAX as usize
    )?;

    Ok(DeepTileBlock {
        coordinates,
        decompressed_sample_data_size,
        compressed_pixel_offset_table,
        compressed_sample_data,
    })
}

// TODO what about ordering? y-ordering? random? increasing? or only needed for processing?

fn multi_part_chunk<R: Seek + Read>(
    read: &mut SeekBufRead<R>,
    meta_data: &MetaData,
) -> Result<MultiPartChunk>
{
    // decode the index that tells us which header we need to analyze
    let part_number = read_i32(read)?; // documentation says u64, but is i32

    let header = &meta_data.headers.get(part_number as usize)
        .ok_or(Error::Invalid("chunk part number"))?;

    let kind_index = header.indices.kind.ok_or(Error::Missing("multiplart 'type' attribute"))?;
    let kind = &header.attributes[kind_index].value.to_text()
        .ok_or(Error::Invalid("multipart 'type' attribute-type"))?;

    Ok(MultiPartChunk {
        part_number,
        // TODO replace these literals with constants
        block: match kind {
            ParsedText::ScanLine        => MultiPartBlock::ScanLine(scan_line_block(read)?),
            ParsedText::Tile            => MultiPartBlock::Tiled(tile_block(read)?),
            ParsedText::DeepScanLine    => MultiPartBlock::DeepScanLine(Box::new(deep_scan_line_block(read)?)),
            ParsedText::DeepTile        => MultiPartBlock::DeepTile(Box::new(deep_tile_block(read)?)),
            _ => return Err(Error::Invalid("multi-part block type"))
        },
    })
}


fn multi_part_chunks<R: Seek + Read>(
    read: &mut SeekBufRead<R>,
    meta_data: &MetaData,
) -> Result<Vec<MultiPartChunk>>
{
    let mut chunks = Vec::new();
    for offset_table in &meta_data.offset_tables {
        chunks.reserve(offset_table.len());
        for _ in 0..offset_table.len() {
            chunks.push(multi_part_chunk(read, meta_data)?)
        }
    }

    Ok(chunks)
}

fn single_part_chunks<R: Seek + Read>(
    read: &mut SeekBufRead<R>,
    meta_data: &MetaData,
) -> Result<SinglePartChunks>
{
    // single-part files have either scan lines or tiles,
    // but never deep scan lines or deep tiles
    assert!(!meta_data.version.has_deep_data);

    assert_eq!(meta_data.headers.len(), 1, "single_part_chunks called with multiple parts");
    let header = &meta_data.headers[0];

    assert_eq!(meta_data.offset_tables.len(), 1, "single_part_chunks called with multiple parts");
    let offset_table = &meta_data.offset_tables[0];


    // TODO is there a better way to figure out if this image contains tiles?
    let is_tile_image = header.tiles().is_some();

    Ok(if !is_tile_image {
        let mut scan_line_blocks = Vec::with_capacity(offset_table.len());
        for _ in 0..offset_table.len() {
            scan_line_blocks.push(scan_line_block(read)?)
        }

        SinglePartChunks::ScanLine(scan_line_blocks)

    } else {
        let mut tile_blocks = Vec::with_capacity(offset_table.len());
        for _ in 0..offset_table.len() {
            tile_blocks.push(tile_block(read)?)
        }

        SinglePartChunks::Tile(tile_blocks)
    })
}

fn chunks<R: Seek + Read>(
    read: &mut SeekBufRead<R>,
    meta_data: &MetaData,
) -> Result<Chunks>
{
    Ok({
        if meta_data.version.has_multiple_parts {
            Chunks::MultiPart(multi_part_chunks(read, meta_data)?)

        } else {
            Chunks::SinglePart(single_part_chunks(read, meta_data)?)
        }
    })
}

fn meta_data<R: Seek + Read>(read: &mut SeekBufRead<R>) -> Result<MetaData> {
    let version = version(read)?;

    if !version.is_valid() {
        return Err(Error::Invalid("version value combination"))
    }

    let headers = headers(read, version)?;
    let offset_tables = offset_tables(read, version, &headers)?;

    // TODO check if supporting version 2 implies supporting version 1
    Ok(MetaData { version, headers, offset_tables })
}



#[must_use]
pub fn read_file(path: &str) -> Result<RawImage> {
    read(::std::fs::File::open(path)?)
}

/// assumes that the provided reader is not buffered, and will create a buffer for it
#[must_use]
pub fn read<R: Read + Seek>(unbuffered: R) -> Result<RawImage> {
    read_seekable_buffer(&mut SeekBufRead::new(unbuffered))
}

#[must_use]
pub fn read_seekable_buffer<R: Read + Seek>(read: &mut SeekBufRead<R>) -> Result<RawImage> {
    skip_identification_bytes(read)?;
    let meta_data = meta_data(read)?;
    let chunks = chunks(read, &meta_data)?;
    Ok(::file::RawImage { meta_data, chunks, })
}

