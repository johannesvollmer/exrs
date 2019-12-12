

pub use ::std::io::{Read, Write};
use half::slice::{HalfFloatSliceExt};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use ::half::f16;

use crate::error::*;


// TODO DRY !!!!!!! the whole module
pub struct PeekRead<T> {
    inner: T,
    peeked: Option<std::io::Result<u8>>,
}

impl<T: Read> PeekRead<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, peeked: None }
    }

    pub fn peek_u8(&mut self) -> &std::io::Result<u8> {
        self.peeked = self.peeked.take().or_else(|| Some(self.inner.read_u8()));
        self.peeked.as_ref().unwrap()
    }

    pub fn skip_if_eq(&mut self, value: u8) -> std::io::Result<bool> {
        match self.peek_u8() {
            Ok(peeked) if *peeked == value =>  {
                self.read_u8().unwrap(); // skip, will be Ok(value)
                Ok(true)
            },

            Ok(_) => Ok(false),
            Err(_) => Err(self.read_u8().err().unwrap())
        }
    }
}

impl<T: Read> Read for PeekRead<T> {
    fn read(&mut self, target_buffer: &mut [u8]) -> std::io::Result<usize> {
        if target_buffer.is_empty() {
            return Ok(0)
        }

        match self.peeked.take() {
            None => self.inner.read(target_buffer),
            Some(peeked) => {
                target_buffer[0] = peeked?;
                Ok(1 + self.inner.read(&mut target_buffer[1..])?)
            }
        }
    }
}





// will be inlined
/// extension trait for primitive types like numbers and arrays
pub trait Data: Sized + Default + Clone {

    fn read(read: &mut impl Read) -> ReadResult<Self>;
    fn read_slice(read: &mut impl Read, slice: &mut[Self]) -> ReadResult<()>;

    fn read_vec(read: &mut impl Read, data_size: usize, estimated_max: usize) -> ReadResult<Vec<Self>> {
        let mut vec = Vec::new();
        Self::read_into_vec(read, &mut vec, data_size, estimated_max)?;
        Ok(vec)
    }

    fn write(self, write: &mut impl Write) -> WriteResult;
    fn write_slice(write: &mut impl Write, slice: &[Self]) -> WriteResult;

    fn byte_size(self) -> usize {
        ::std::mem::size_of::<Self>()
    }

    fn read_into_vec(read: &mut impl Read, data: &mut Vec<Self>, data_size: usize, estimated_max: usize) -> ReadResult<()> {
        let start = data.len();
        let end = start + data_size;
        let max_end = start + estimated_max;

        if data_size < estimated_max {
            data.resize(end, Self::default());
            Self::read_slice(read, &mut data[start .. end])
        }
        else {
            println!("suspiciously large data size: {}, estimated max: {}", data_size, estimated_max);

            data.resize(max_end, Self::default());
            Self::read_slice(read, &mut data[start .. max_end])?;

            for _ in estimated_max..data_size {
                data.push(Self::read(read)?);
            }

            Ok(())
        }
    }

    fn write_i32_sized_slice<W: Write>(write: &mut W, slice: &[Self]) -> WriteResult {
        (slice.len() as i32).write(write)?;
        Self::write_slice(write, slice)
    }

    fn read_i32_sized_vec(read: &mut impl Read, estimated_max: usize) -> ReadResult<Vec<Self>> {
        let size = i32::read(read)?;
        if size < 0 { return Err(unimplemented!()) }
        Self::read_vec(read, size as usize, estimated_max)
    }
}


macro_rules! implement_data_for_primitive {
    ($kind: ident, $read: ident, $read_into: ident, $write: ident, $to_little_endian: ident) => {
        impl Data for $kind {
            fn read(read: &mut impl Read) -> ReadResult<Self> {
                read. $read ::<LittleEndian> ().map_err(ReadError::from)
            }

            fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> ReadResult<()> {
                read. $read_into ::<LittleEndian> (slice).map_err(ReadError::from)
            }

            fn write(self, write: &mut impl Write) -> WriteResult {
                write. $write ::<LittleEndian> (self).map(|_| ()).map_err(WriteError::from)
            }

            fn write_slice(write: &mut impl Write, slice: &[Self]) -> WriteResult {
                // TODO without allocation?!?!?!
                let mut mutable = slice.to_owned();
                LittleEndian:: $to_little_endian (mutable.as_mut_slice()); // convert data to little endian

                u8::write_slice(write, unsafe {
                    std::slice::from_raw_parts(
                        mutable.as_ptr() as *const u8,
                        mutable.len() * std::mem::size_of::<$kind>()
                    )
                }).map(|_| ()).map_err(WriteError::from)
            }
        }
    };
}

use byteorder::ByteOrder;
implement_data_for_primitive!(u16, read_u16, read_u16_into, write_u16, from_slice_u16);
implement_data_for_primitive!(u32, read_u32, read_u32_into, write_u32, from_slice_u32);
implement_data_for_primitive!(i32, read_i32, read_i32_into, write_i32, from_slice_i32);
implement_data_for_primitive!(i64, read_i64, read_i64_into, write_i64, from_slice_i64);
implement_data_for_primitive!(u64, read_u64, read_u64_into, write_u64, from_slice_u64);
implement_data_for_primitive!(i16, read_i16, read_i16_into, write_i16, from_slice_i16);
implement_data_for_primitive!(f32, read_f32, read_f32_into, write_f32, from_slice_f32);
implement_data_for_primitive!(f64, read_f64, read_f64_into, write_f64, from_slice_f64);


impl Data for u8 {
    fn read(read: &mut impl Read) -> ReadResult<Self> {
        read.read_u8().map_err(ReadError::from)
    }

    fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> ReadResult<()> {
        read.read_exact(slice).map_err(ReadError::from)
    }

    fn write(self, write: &mut impl Write) -> WriteResult {
        write.write_u8(self).map_err(WriteError::from)
    }

    fn write_slice(write: &mut impl Write, slice: &[Self]) -> WriteResult {
        write.write_all(slice).map_err(WriteError::from)
    }
}

impl Data for f16 {
    fn read(read: &mut impl Read) -> ReadResult<Self> {
        read.read_u16::<LittleEndian>().map(f16::from_bits).map_err(ReadError::from)
    }

    fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> ReadResult<()> {
        let bits = slice.reinterpret_cast_mut();
        u16::read_slice(read, bits)
    }

    fn write(self, write: &mut impl Write) -> WriteResult {
        self.to_bits().write(write)
    }

    fn write_slice(write: &mut impl Write, slice: &[Self]) -> WriteResult {
        let bits = slice.reinterpret_cast();
        u16::write_slice(write, bits)
    }
}

impl Data for i8 {
    fn read(read: &mut impl Read) -> ReadResult<Self> {
        read.read_i8().map_err(ReadError::from)
    }

    fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> ReadResult<()> {
        let as_u8 = unsafe {
            std::slice::from_raw_parts_mut(
                slice.as_mut_ptr() as *mut u8,
                slice.len()
            )
        };

        u8::read_slice(read, as_u8)
    }

    fn write(self, write: &mut impl Write) -> WriteResult {
        write.write_i8(self).map_err(WriteError::from)
    }

    fn write_slice(write: &mut impl Write, slice: &[Self]) -> WriteResult {
        // single bytes don't need shuffling to little endian
        // reinterpret the i8 array as bytes, in order to write it
        u8::write_slice(write, unsafe {
            ::std::slice::from_raw_parts(
                slice.as_ptr() as *const u8,
                slice.len()
            )
        })
    }
}





pub struct MagicNumber;
impl MagicNumber {
    pub const BYTES: [u8; 4] = [0x76, 0x2f, 0x31, 0x01];
}

impl MagicNumber {
    pub fn write<W: Write>(write: &mut W) -> WriteResult {
        u8::write_slice(write, &Self::BYTES)
    }

    pub fn is_exr<R: Read>(read: &mut R) -> ReadResult<bool> {
        let mut magic_num = [0; 4];
        u8::read_slice(read, &mut magic_num)?;
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

    pub fn has_come(read: &mut PeekRead<impl Read>) -> ReadResult<bool> {
        read.skip_if_eq(0).map_err(ReadError::IoError)
    }
}


#[cfg(test)]
mod test {
    use crate::file::io::PeekRead;
    use byteorder::ReadBytesExt;
    use std::io::Read;

    #[test]
    fn peek(){
        let buffer: &[u8] = &[0,1,2,3];
        let mut peek = PeekRead::new(buffer);

        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.read_u8().unwrap(), 0);

        assert_eq!(peek.read(&mut [0,0]).unwrap(), 2);

        assert_eq!(peek.peek_u8().as_ref().unwrap(), &3);
        assert_eq!(peek.read_u8().unwrap(), 3);

        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());

        assert!(peek.read_u8().is_err());
    }
}


