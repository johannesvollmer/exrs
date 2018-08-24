use ::std::io::Write;
use super::RawImage;
use image::attributes::*;
use ::bit_field::*;
use ::byteorder::{WriteBytesExt, LittleEndian};
use file::*;

pub type Result = ::std::result::Result<(), Error>;


#[derive(Debug)]
pub enum Error {
    NotSupported(&'static str),
    IoError(::std::io::Error),
    Invalid(Invalid),
}

/// enable using the `?` operator on io errors
impl From<::std::io::Error> for Error {
    fn from(err: ::std::io::Error) -> Self {
        Error::IoError(err)
    }
}

/// Enable using the `?` operator on Validity
impl From<Invalid> for Error {
    fn from(err: Invalid) -> Self {
        Error::Invalid(err)
    }
}




fn identify_exr<W: Write>(write: &mut W) -> Result {
    write.write(&super::MAGIC_NUMBER)?;
    Ok(())
}

pub fn version<W: Write>(write: &mut W, version: Version) -> Result {
    version.validate()?;

    // the 8 least significant bits contain the file format version number
    // and the flags are set to 0
    let mut version_and_flags = version.file_format_version as u32;

    // the 24 most significant bits are treated as a set of boolean flags
    version_and_flags.set_bit(9, version.is_single_tile);
    version_and_flags.set_bit(10, version.has_long_names);
    version_and_flags.set_bit(11, version.has_deep_data);
    version_and_flags.set_bit(12, version.has_multiple_parts);
    // all remaining bits except 9, 10, 11 and 12 are reserved and should be 0

    write_u32(write, version_and_flags)
}


fn sequence_end<W: Write>(write: &mut W) -> Result {
    write_u8(write, 0)
}



fn write_i32<W: WriteBytesExt>(write: &mut W, n: i32) -> Result {
    write.write_i32::<LittleEndian>(n).map_err(Error::from)
}

fn write_u8<W: WriteBytesExt>(write: &mut W, n: u8) -> Result {
    write.write_u8(n).map_err(Error::from)
}

fn write_u32<W: WriteBytesExt>(write: &mut W, n: u32) -> Result {
    write.write_u32::<LittleEndian>(n).map_err(Error::from)
}

fn write_u64<W: WriteBytesExt>(write: &mut W, n: u64) -> Result {
    write.write_u64::<LittleEndian>(n).map_err(Error::from)
}

fn write_f32<W: WriteBytesExt>(write: &mut W, n: f32) -> Result {
    write.write_f32::<LittleEndian>(n).map_err(Error::from)
}

fn write_f64<W: WriteBytesExt>(write: &mut W, n: f64) -> Result {
    write.write_f64::<LittleEndian>(n).map_err(Error::from)
}

fn write_u8_array<W: Write>(write: &mut W, bytes: &[u8]) -> Result {
    write.write_all(bytes).map_err(Error::from)
}

// TODO test
fn write_f32_array<W: Write>(write: &mut W, array: &[f32]) -> Result {
    // reinterpret the f32 array as bytes, in order to write it
    let as_u8 = unsafe {
        ::std::slice::from_raw_parts(
            array.as_ptr() as *const u8,
            array.len() * ::std::mem::size_of::<f32>()
        )
    };

    write_u8_array(write, as_u8)
}

// TODO test
fn write_i8_array<W: Write>(write: &mut W, array: &[i8]) -> Result {
    // reinterpret the i8 array as bytes, in order to write it
    let as_u8 = unsafe {
        ::std::slice::from_raw_parts(
            array.as_ptr() as *const u8,
            array.len()
        )
    };

    write_u8_array(write, as_u8)
}


fn validate_string_length(text: &[u8], allow_long: Option<bool>) -> Result {
    let is_valid = !text.is_empty() && match allow_long {
        Some(false) => text.len() < 32,
        Some(true) => text.len() < 256,
        None => true,
    };

    if is_valid { Ok(()) } else {
        if text.is_empty() {
            Err(Invalid::Content(Value::Text, Required::Min(1)).into())
        }
        else if allow_long.unwrap() {
            Err(Invalid::Content(Value::Text, Required::Max(255)).into())

        } else {
            Err(Invalid::Content(Value::Text, Required::Max(31)).into())
        }
    }
}

fn null_terminated_text_bytes<W: Write>(write: &mut W, text: &[u8], allow_long: Option<bool>) -> Result {
    validate_string_length(text, allow_long).and_then(|()| {
        write_u8_array(write, text)?;
        sequence_end(write)
    })
}

fn null_terminated_text<W: Write>(write: &mut W, text: &Text, allow_long: Option<bool>) -> Result {
    null_terminated_text_bytes(write, text.bytes.as_slice(), allow_long)
}

fn i32_sized_text<W: Write>(write: &mut W, text: &Text, allow_long: Option<bool>) -> Result {
    validate_string_length(text.bytes.as_slice(), allow_long).and_then(|()| {
        i32_sized_u8_array(write, text.bytes.as_slice())
    })
}


fn i32_box_2<W: Write>(write: &mut W, ibox: I32Box2) -> Result {
    write_i32(write, ibox.x_min)?;
    write_i32(write, ibox.y_min)?;
    write_i32(write, ibox.x_max)?;
    write_i32(write, ibox.y_max)
}

fn f32_box_2<W: Write>(write: &mut W, fbox: F32Box2) -> Result {
    write_f32(write, fbox.x_min)?;
    write_f32(write, fbox.y_min)?;
    write_f32(write, fbox.x_max)?;
    write_f32(write, fbox.y_max)
}

fn channel<W: Write>(write: &mut W, channel: &Channel, long_names: bool) -> Result {
    null_terminated_text(write, &channel.name, Some(long_names))?;

    // there's definitely going to be more than 255 different pixel types
    // in the future, when exr is still used
    write_i32(write, match channel.pixel_type {
        PixelType::U32 => 0,
        PixelType::F16 => 1,
        PixelType::F32 => 2,
    })?;

    write_u8(write, match channel.is_linear {
        false => 0,
        true => 1,
    })?;

    write_i8_array(write, &channel.reserved)?;
    write_i32(write, channel.x_sampling)?;
    write_i32(write, channel.y_sampling)
}

fn channel_list<W: Write>(write: &mut W, channels: &ChannelList, long_names: bool) -> Result {
    for chan in channels {
        channel(write, chan, long_names)?;
    }

    sequence_end(write)
}

fn chromaticities<W: Write>(write: &mut W, chroma: &Chromaticities) -> Result {
    write_f32(write, chroma.red_x)?;
    write_f32(write, chroma.red_y)?;
    write_f32(write, chroma.green_x)?;
    write_f32(write, chroma.green_y)?;
    write_f32(write, chroma.blue_x)?;
    write_f32(write, chroma.blue_y)?;
    write_f32(write, chroma.white_x)?;
    write_f32(write, chroma.white_y)
}

fn compression<W: Write>(write: &mut W, compression: Compression) -> Result {
    use self::Compression::*;
    write_u8(write, match compression {
        None => 0,
        RLE => 1,
        ZIPSingle => 2,
        ZIP => 3,
        PIZ => 4,
        PXR24 => 5,
        B44 => 6,
        B44A => 7,
    })
}

fn environment_map<W: Write>(write: &mut W, env_map: EnvironmentMap) -> Result {
    use self::EnvironmentMap::*;
    write_u8(write, match env_map {
        LatitudeLongitude => 0,
        Cube => 1
    })
}

fn key_code<W: Write>(write: &mut W, code: KeyCode) -> Result {
    write_i32(write, code.film_manufacturer_code)?;
    write_i32(write, code.film_type)?;
    write_i32(write, code.film_roll_prefix)?;
    write_i32(write, code.count)?;
    write_i32(write, code.perforation_offset)?;
    write_i32(write, code.perforations_per_count)
}

fn line_order<W: Write>(write: &mut W, order: LineOrder) -> Result {
    use self::LineOrder::*;
    write_u8(write, match order {
        IncreasingY => 0,
        DecreasingY => 1,
        RandomY => 2,
    })
}

/// allows any text length since it is only used for attribute values,
/// but not attribute names, attribute type names, or channel names
fn vec_of_i32_sized_texts<W: Write>(write: &mut W, texts: &[Text]) -> Result {
    for text in texts {
        i32_sized_text(write, text, None)?;
    }
    Ok(()) // length of the text-vector can be inferred from attribute size
}

fn i32_sized_u8_array<W: Write>(write: &mut W, bytes: &[u8]) -> Result {
    write_i32(write, bytes.len() as i32)?; // bit is wasted and sign checks are now necessary
    write_u8_array(write, bytes)
}

fn preview<W: Write>(write: &mut W, preview: &Preview) -> Result {
    write_u32(write, preview.width)?;
    write_u32(write, preview.height)?;
    write_i8_array(write, &preview.pixel_data)
}

fn tile_description<W: Write>(write: &mut W, tiles: &TileDescription) -> Result {
    write_u32(write, tiles.x_size)?;
    write_u32(write, tiles.y_size)?;

    let level_mode = match tiles.level_mode {
        LevelMode::One => 0,
        LevelMode::MipMap => 1,
        LevelMode::RipMap => 2,
    };

    let rounding_mode = match tiles.rounding_mode {
        RoundingMode::Down => 0,
        RoundingMode::Up => 1,
    };

    let mode = level_mode + (rounding_mode * 16);
    write_u8(write, mode)
}

fn attribute_value<W: Write>(write: &mut W, value: &AttributeValue, long_names: bool) -> Result {
    use self::AttributeValue::*;
    match *value {
        I32Box2(value) => i32_box_2(write, value),
        F32Box2(value) => f32_box_2(write, value),

        I32(value) => write_i32(write, value),
        F32(value) => write_f32(write, value),
        F64(value) => write_f64(write, value),

        Rational(a, b) => { write_i32(write, a)?; write_u32(write, b) },
        TimeCode(a, b) => { write_u32(write, a)?; write_u32(write, b) },

        I32Vec2(x, y) => { write_i32(write, x)?; write_i32(write, y) },
        F32Vec2(x, y) => { write_f32(write, x)?; write_f32(write, y) },
        I32Vec3(x, y, z) => { write_i32(write, x)?; write_i32(write, y)?; write_i32(write, z) },
        F32Vec3(x, y, z) => { write_f32(write, x)?; write_f32(write, y)?; write_f32(write, z) },

        ChannelList(ref channels) => channel_list(write, channels, long_names),
        Chromaticities(ref chroma) => chromaticities(write, chroma),
        Compression(value) => compression(write, value),
        EnvironmentMap(value) => environment_map(write, value),

        KeyCode(value) => key_code(write, value),
        LineOrder(value) => line_order(write, value),

        F32Matrix3x3(ref value) => write_f32_array(write, value),
        F32Matrix4x4(ref value) => write_f32_array(write, value),

        Preview(ref value) => preview(write, value),

        // attribute value texts never have limited size
        // also, don't serialize size, as it can be inferred from attribute size
        Text(ref value) => write_u8_array(write, value.to_text_bytes()),

        TextVector(ref value) => vec_of_i32_sized_texts(write, value), // TODO check length 31 or 255
        TileDescription(ref value) => tile_description(write, value),
    }
}


fn attribute<W: Write>(write: &mut W, attribute: &Attribute, long_names: bool) -> Result {
    null_terminated_text(write, &attribute.name, Some(long_names))?;
    null_terminated_text_bytes(write, attribute.value.kind_name(), Some(long_names))?;
    write_i32(write, attribute.value.byte_size() as i32)?;
    attribute_value(write, &attribute.value, long_names)
}

/// throws if number of parts is not as declared in version
fn headers<W: Write>(write: &mut W, headers: &Headers, version: Version) -> Result {
    let has_multiple_headers = headers.len() != 1;
    if headers.is_empty() || version.has_multiple_parts != has_multiple_headers {
        // TODO return combination?
        return Err(Invalid::Content(Value::Part("headers count"), Required::Exact("1")).into());
    }

    for header in headers {
        header.validate(version)?;

        for attrib in &header.attributes {
            attribute(write, attrib, version.has_long_names)?;
        }
        sequence_end(write)?;

    }
    sequence_end(write)?;

    Ok(())
}


/*fn f32_matrix_3x3<W: WriteBytesExt>(write: &mut W, matrix: &[f32; 9]) -> Result {
    write_f32_array(write, matrix)
}

fn f32_matrix_4x4<W: WriteBytesExt>(write: &mut W, matrix: &[f32; 16]) -> Result {
    write_f32_array(write, matrix)
}*/








pub fn meta_data<W: Write>(write: &mut W, meta: &MetaData) -> Result {
    meta.validate()?;
    version(write, meta.version)?;
    headers(write, &meta.headers, meta.version)?;
    Err(Error::NotSupported("offset_tables"))
//    offset_tables(write, &meta.offset_tables);
//    Ok(())
}


#[must_use]
pub fn write_file(path: &str, image: &RawImage) -> Result {
    write(&mut ::std::fs::File::open(path)?, image)
}

#[must_use]
pub fn write<W: Write>(write: &mut W, image: &RawImage) -> Result {
    identify_exr(write)?;
    meta_data(write, &image.meta_data)?;
    Err(Error::NotSupported("pixel content"))
}
