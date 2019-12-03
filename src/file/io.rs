
use super::*;
use ::byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt, ByteOrder};

pub use ::std::io::{Read, Write, Seek, SeekFrom};
//pub use super::io::{ReadResult, ReadError, WriteResult, WriteError};


use self::validity::*;


pub type WriteResult = ::std::result::Result<(), WriteError>;

#[derive(Debug)]
pub enum WriteError {
    CompressionError(data::compression::Error),
    IoError(::std::io::Error),
    Invalid(Invalid),
}



pub type ReadResult<T> = ::std::result::Result<T, ReadError>;

// TODO implement Display for all errors
#[derive(Debug)]
pub enum ReadError {
    NotEXR,
    Invalid(Invalid),
//    UnknownAttributeType { bytes_to_skip: u32 },

    IoError(::std::io::Error),
    CompressionError(Box<data::compression::Error>),
}


/// Enable using the `?` operator on io::Result
impl From<::std::io::Error> for ReadError {
    fn from(io_err: ::std::io::Error) -> Self {
        ReadError::IoError(io_err)
    }
}

/// Enable using the `?` operator on compress::Result
impl From<data::compression::Error> for ReadError {
    fn from(compress_err: data::compression::Error) -> Self {
        ReadError::CompressionError(Box::new(compress_err))
    }
}

/// Enable using the `?` operator on Validity
impl From<Invalid> for ReadError {
    fn from(err: Invalid) -> Self {
        ReadError::Invalid(err)
    }
}


/// enable using the `?` operator on io errors
impl From<::std::io::Error> for WriteError {
    fn from(err: ::std::io::Error) -> Self {
        WriteError::IoError(err)
    }
}

/// Enable using the `?` operator on Validity
impl From<Invalid> for WriteError {
    fn from(err: Invalid) -> Self {
        WriteError::Invalid(err)
    }
}


// TODO DRY


// will be inlined
/// extension trait for primitive types like numbers and arrays
pub trait Data: Sized {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult;
    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self>;

    // TODO make static
    fn byte_size(self) -> usize { ::std::mem::size_of::<Self>() }
}

impl Data for u8 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_u8(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_u8().map_err(ReadError::from)
    }
}

impl Data for u32 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_u32::<LittleEndian>(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_u32::<LittleEndian>().map_err(ReadError::from)
    }
}

impl Data for u64 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_u64::<LittleEndian>(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_u64::<LittleEndian>().map_err(ReadError::from)
    }
}

impl Data for i64 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_i64::<LittleEndian>(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_i64::<LittleEndian>().map_err(ReadError::from)
    }
}

impl Data for u16 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_u16::<LittleEndian>(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_u16::<LittleEndian>().map_err(ReadError::from)
    }
}

impl Data for i8 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_i8(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_i8().map_err(ReadError::from)
    }
}

impl Data for i32 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_i32::<LittleEndian>(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_i32::<LittleEndian>().map_err(ReadError::from)
    }
}

impl Data for f32 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_f32::<LittleEndian>(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_f32::<LittleEndian>().map_err(ReadError::from)
    }
}

impl Data for f64 {
    fn write<W: WriteBytesExt>(self, write: &mut W) -> WriteResult {
        write.write_f64::<LittleEndian>(self).map_err(WriteError::from)
    }

    fn read<R: ReadBytesExt>(read: &mut R) -> ReadResult<Self> {
        read.read_f64::<LittleEndian>().map_err(ReadError::from)
    }
}


// TODO make these instance functions?

pub fn write_u8_array<W: Write>(write: &mut W, bytes: &[u8]) -> WriteResult {
    write.write_all(bytes).map_err(WriteError::from)
}

pub fn write_i32_sized_u8_array<W: Write>(write: &mut W, bytes: &[u8]) -> WriteResult {
    (bytes.len() as i32).write(write)?;
    write_u8_array(write, bytes)
}

// TODO test
pub fn write_f32_array<W: WriteBytesExt>(write: &mut W, array: &mut [f32]) -> WriteResult {
    LittleEndian::from_slice_f32(array); // convert data to little endian
    write_u8_array(write, unsafe {
        ::std::slice::from_raw_parts(
            array.as_ptr() as *const u8,
            array.len() * ::std::mem::size_of::<f32>()
        )
    })
}

// TODO test
pub fn write_i32_array<W: Write>(write: &mut W, array: &mut [i32]) -> WriteResult {
    LittleEndian::from_slice_i32(array); // convert data to little endian
    write_u8_array(write, unsafe {
        ::std::slice::from_raw_parts(
            array.as_ptr() as *const u8,
            array.len() * ::std::mem::size_of::<i32>()
        )
    })
}

pub fn write_u32_array<W: Write>(write: &mut W, array: &mut [u32]) -> WriteResult {
    LittleEndian::from_slice_u32(array); // convert data to little endian
    write_u8_array(write, unsafe {
        ::std::slice::from_raw_parts(
            array.as_ptr() as *const u8,
            array.len() * ::std::mem::size_of::<u32>()
        )
    })
}

// TODO test
pub fn write_u64_array<W: Write>(write: &mut W, array: &mut [u64]) -> WriteResult {
    LittleEndian::from_slice_u64(array); // convert data to little endian
    write_u8_array(write, unsafe {
        ::std::slice::from_raw_parts(
            array.as_ptr() as *const u8,
            array.len() * ::std::mem::size_of::<u64>()
        )
    })
}

// TODO test
pub fn write_i8_array<W: Write>(write: &mut W, array: &[i8]) -> WriteResult {
    // single bytes don't need shuffling to little endian
    // reinterpret the i8 array as bytes, in order to write it
    write_u8_array(write, unsafe {
        ::std::slice::from_raw_parts(
            array.as_ptr() as *const u8,
            array.len()
        )
    })
}

// TODO DRY

pub fn read_u8_array<R: Read>(read: &mut R, array: &mut [u8]) -> ReadResult<()> {
    read.read_exact(array).map_err(ReadError::from)
}

// TODO test
pub fn read_i8_array<R: Read>(read: &mut R, array: &mut [i8]) -> ReadResult<()> {
    let as_u8 = unsafe {
        ::std::slice::from_raw_parts_mut(
            array.as_mut_ptr() as *mut u8,
            array.len()
        )
    };

    read.read_exact(as_u8).map_err(ReadError::from)
}

pub fn read_f32_array<R: ReadBytesExt>(read: &mut R, array: &mut [f32]) -> ReadResult<()> {
    read.read_f32_into::<LittleEndian>(array).map_err(ReadError::from)
}
pub fn read_u32_array<R: ReadBytesExt>(read: &mut R, array: &mut [u32]) -> ReadResult<()> {
    read.read_u32_into::<LittleEndian>(array).map_err(ReadError::from)
}



pub fn read_i32_vec<R: ReadBytesExt>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<i32>> {
    if data_size < estimated_max {
        let mut data = vec![0; data_size];
        read.read_i32_into::<LittleEndian>(&mut data)?;
        data.shrink_to_fit();
        Ok(data)

    } else {
        println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

        // be careful for suspiciously large data,
        // as reading the pixel_data_size could have gone wrong
        // (read byte by byte to avoid allocating too much memory at once,
        // assuming that it will fail soon, when the file ends)
        let mut data = vec![0; estimated_max];
        read.read_i32_into::<LittleEndian>(&mut data)?;

        for _ in estimated_max..data_size {
            data.push(i32::read(read)?);
        }

        data.shrink_to_fit();
        Ok(data)
    }
}


pub fn read_f32_vec<R: ReadBytesExt>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<f32>> {
    if data_size < estimated_max {
        let mut data = vec![0.0; data_size];
        read.read_f32_into::<LittleEndian>(&mut data)?;
        data.shrink_to_fit();
        Ok(data)

    } else {
        println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

        // be careful for suspiciously large data,
        // as reading the pixel_data_size could have gone wrong
        // (read byte by byte to avoid allocating too much memory at once,
        // assuming that it will fail soon, when the file ends)
        let mut data = vec![0.0; estimated_max];
        read.read_f32_into::<LittleEndian>(&mut data)?;

        for _ in estimated_max..data_size {
            data.push(f32::read(read)?);
        }

        data.shrink_to_fit();
        Ok(data)
    }
}

use ::half::f16;

/// The representation of 16-bit floating-point numbers is analogous to IEEE 754,
/// but with 5 exponent bits and 10 bits for the fraction
// reads an u16 array first and then interprets it as f16
pub fn read_f16_vec<R: ReadBytesExt>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<f16>> {
    if data_size < estimated_max {
        let mut data = vec![0; data_size];
        read.read_u16_into::<LittleEndian>(&mut data)?;
        data.shrink_to_fit();
        Ok(::half::vec::from_bits(data))

    } else {
        println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

        // be careful for suspiciously large data,
        // as reading the pixel_data_size could have gone wrong
        // (read byte by byte to avoid allocating too much memory at once,
        // assuming that it will fail soon, when the file ends)
        let mut data = vec![0; estimated_max];
        read.read_u16_into::<LittleEndian>(&mut data)?;

        for _ in estimated_max..data_size {
            data.push(u16::read(read)?);
        }

        data.shrink_to_fit();
        Ok(::half::vec::from_bits(data))
    }
}

pub fn read_u32_vec<R: ReadBytesExt>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<u32>> {
    if data_size < estimated_max {
        let mut data = vec![0; data_size];
        read.read_u32_into::<LittleEndian>(&mut data)?;
        data.shrink_to_fit();
        Ok(data)

    } else {
        println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

        // be careful for suspiciously large data,
        // as reading the pixel_data_size could have gone wrong
        // (read byte by byte to avoid allocating too much memory at once,
        // assuming that it will fail soon, when the file ends)
        let mut data = vec![0; estimated_max];
        read.read_u32_into::<LittleEndian>(&mut data)?;

        for _ in estimated_max..data_size {
            data.push(u32::read(read)?);
        }

        data.shrink_to_fit();
        Ok(data)
    }
}

pub fn read_u64_vec<R: ReadBytesExt>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<u64>> {
    if data_size < estimated_max {
        let mut data = vec![0; data_size];
        read.read_u64_into::<LittleEndian>(&mut data)?;
        data.shrink_to_fit();
        Ok(data)

    } else {
        println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

        // be careful for suspiciously large data,
        // as reading the pixel_data_size could have gone wrong
        // (read byte by byte to avoid allocating too much memory at once,
        // assuming that it will fail soon, when the file ends)
        let mut data = vec![0; estimated_max];
        read.read_u64_into::<LittleEndian>(&mut data)?;

        for _ in estimated_max..data_size {
            data.push(u64::read(read)?);
        }

        data.shrink_to_fit();
        Ok(data)
    }
}

pub fn read_i8_vec<R: Read>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<i8>> {
    if data_size < estimated_max {
        let mut data = vec![0; data_size];
        read_i8_array(read, &mut data)?;
        data.shrink_to_fit();
        Ok(data)

    } else {
        println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

        // be careful for suspiciously large data,
        // as reading the pixel_data_size could have gone wrong
        // (read byte by byte to avoid allocating too much memory at once,
        // assuming that it will fail soon, when the file ends)
        let mut data = vec![0; estimated_max];
        read_i8_array(read, &mut data)?;

        for _ in estimated_max..data_size {
            data.push(i8::read(read)?);
        }

        data.shrink_to_fit();
        Ok(data)
    }
}

/// reuses the allocated buffer, does not shrink to fit
pub fn reuse_read_u8_vec<R: Read>(read: &mut R, mut data: Vec<u8>, data_size: usize, estimated_max: usize) -> ReadResult<Vec<u8>> {
    if data_size < estimated_max {
        data.resize(data_size, 0);
        read_u8_array(read, &mut data)?;
        Ok(data)

    } else {
        println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

        // be careful for suspiciously large data,
        // as reading the pixel_data_size could have gone wrong
        // (read byte by byte to avoid allocating too much memory at once,
        // assuming that it will fail soon, when the file ends)
        data.resize(estimated_max, 0);
        read.read_exact(&mut data)?;

        for _ in estimated_max..data_size {
            data.push(u8::read(read)?);
        }

        Ok(data)
    }
}

pub fn read_u8_vec<R: Read>(read: &mut R, data_size: usize, estimated_max: usize) -> ReadResult<Vec<u8>> {
    if data_size < estimated_max {
        let mut data = vec![0; data_size];
        read_u8_array(read, &mut data)?;
        data.shrink_to_fit();
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
            data.push(u8::read(read)?);
        }

        data.shrink_to_fit();
        Ok(data)
    }
}

pub fn read_i32_sized_u8_vec<R: Read>(read: &mut R, estimated_max: usize) -> ReadResult<Vec<u8>> {
    let data_size = i32::read(read)? as usize;
    read_u8_vec(read, data_size, estimated_max)
}





pub struct MagicNumber;
impl MagicNumber {
    pub const BYTES: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];
}

impl MagicNumber {
    pub fn write<W: Write>(write: &mut W) -> WriteResult {
        write_u8_array(write, &Self::BYTES)
    }

    pub fn is_exr<R: Read>(read: &mut R) -> ReadResult<bool> {
        let mut magic_num = [0; 4];
        read_u8_array(read, &mut magic_num)?;
        Ok(magic_num == Self::BYTES)
    }

    pub fn validate_exr<R: Read>(read: &mut R) -> ReadResult<()> {
        if Self::is_exr(read)? {
            Ok(())

        } else {
            Err(ReadError::NotEXR)
        }
    }
}


pub struct SequenceEnd;
impl SequenceEnd {
    pub fn byte_size() -> usize {
        1
    }

    pub fn write<W: Write>(write: &mut W) -> WriteResult {
        0_u8.write(write)
    }

    pub fn has_come<R: Read + Seek>(read: &mut R) -> ReadResult<bool> {
        if u8::read(read)? == 0 {
            Ok(true)

        } else {
            // go back that wasted byte because its not 0
            // TODO benchmark peeking the buffer performance
            read.seek(SeekFrom::Current(-1))?;
            Ok(false)
        }
    }
}


