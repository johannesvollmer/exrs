

pub use ::std::io::{Read, Write};
use half::slice::{HalfFloatSliceExt};
use lebe::prelude::*;
use ::half::f16;
use crate::error::{Error, Result, PassiveResult, IoResult};


pub fn skip_bytes(read: &mut impl Read, count: u64) -> PassiveResult {
    let skipped = std::io::copy(
        &mut read.by_ref().take(count),
        &mut std::io::sink()
    )?;

    debug_assert_eq!(skipped, count);
    Ok(())
}

#[inline]
pub fn positive_i32(value: i32, name: &'static str) -> Result<u32> {
    if value < 0 { Err(Error::invalid(name)) }
    else { Ok(value as u32) }
}


pub struct PeekRead<T> {
    inner: T,
    peeked: Option<IoResult<u8>>,
}

impl<T: Read> PeekRead<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, peeked: None }
    }

    pub fn peek_u8(&mut self) -> &IoResult<u8> {
        self.peeked = self.peeked.take().or_else(|| Some(u8::read_from_little_endian(&mut self.inner)));
        self.peeked.as_ref().unwrap()
    }

    pub fn skip_if_eq(&mut self, value: u8) -> IoResult<bool> {
        match self.peek_u8() {
            Ok(peeked) if *peeked == value =>  {
                u8::read_from_little_endian(self).unwrap(); // skip, will be Ok(value)
                Ok(true)
            },

            Ok(_) => Ok(false),
            Err(_) => Err(u8::read_from_little_endian(self).err().unwrap())
        }
    }
}


impl<T: Read> Read for PeekRead<T> {
    fn read(&mut self, target_buffer: &mut [u8]) -> IoResult<usize> {
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

/// extension trait for primitive types like numbers and arrays
pub trait Data: Sized + Default + Clone {
    #[inline]
    fn read(read: &mut impl Read) -> Result<Self>;

    #[inline]
    fn read_slice(read: &mut impl Read, slice: &mut[Self]) -> PassiveResult;

    #[inline]
    fn read_vec(read: &mut impl Read, data_size: usize, estimated_max: usize, abort_on_max: bool) -> Result<Vec<Self>> {
        let mut vec = Vec::new();
        Self::read_into_vec(read, &mut vec, data_size, estimated_max, abort_on_max)?;
        Ok(vec)
    }

    #[inline]
    fn write(self, write: &mut impl Write) -> PassiveResult;

    #[inline]
    fn write_slice(write: &mut impl Write, slice: &[Self]) -> PassiveResult;

    const BYTE_SIZE: usize = ::std::mem::size_of::<Self>();

    /// If a block length greater than this number is decoded,
    /// it will not try to allocate that much memory, but instead consider
    /// that decoding the block length has gone wrong
    #[inline]
    fn read_into_vec(read: &mut impl Read, data: &mut Vec<Self>, data_size: usize, max: usize, abort_on_max: bool) -> PassiveResult {
        let start = data.len();
        let end = start + data_size;
        let max_end = start + max;

        debug_assert!(max <= 24 * std::u16::MAX as usize, "dangerously large max value ({}), was it read from an invalid file?", max);
        debug_assert!(data_size <= max, "suspiciously large data size: {} (max: {})", data_size, max);

        if data_size <= max {
            data.resize(end, Self::default());
            Self::read_slice(read, &mut data[start .. end])
        }
        else {
            if abort_on_max {
                return Err(Error::invalid("content size"))
            }

            println!("suspiciously large data size: {}, estimated max: {}", data_size, max);

            data.resize(max_end, Self::default());
            Self::read_slice(read, &mut data[start .. max_end])?;

            for _ in max..data_size {
                data.push(Self::read(read)?);
            }

            Ok(())
        }
    }

    #[inline]
    fn write_i32_sized_slice<W: Write>(write: &mut W, slice: &[Self]) -> PassiveResult {
        (slice.len() as i32).write(write)?;
        Self::write_slice(write, slice)
    }

    #[inline]
    fn read_i32_sized_vec(read: &mut impl Read, estimated_max: usize, abort_on_max: bool) -> Result<Vec<Self>> {
        let size = i32::read(read)?;
        debug_assert!(size >= 0);

        if size < 0 { Err(Error::invalid("negative array size")) }
        else { Self::read_vec(read, size as usize, estimated_max, abort_on_max) }
    }
}


macro_rules! implement_data_for_primitive {
    ($kind: ident) => {
        impl Data for $kind {
            fn read(read: &mut impl Read) -> Result<Self> {
                Ok(read.read_from_little_endian()?)
            }

            fn write(self, write: &mut impl Write) -> Result<()> {
                write.write_as_little_endian(&self)?;
                Ok(())
            }

            fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> Result<()> {
                read.read_from_little_endian_into(slice)?;
                Ok(())
            }

            fn write_slice(write: &mut impl Write, slice: &[Self]) -> Result<()> {
                write.write_as_little_endian(slice)?;
                Ok(())
            }
        }
    };
}

implement_data_for_primitive!(u8);
implement_data_for_primitive!(i8);
implement_data_for_primitive!(i16);
implement_data_for_primitive!(u16);
implement_data_for_primitive!(u32);
implement_data_for_primitive!(i32);
implement_data_for_primitive!(i64);
implement_data_for_primitive!(u64);
implement_data_for_primitive!(f32);
implement_data_for_primitive!(f64);


impl Data for f16 {
    fn read(read: &mut impl Read) -> Result<Self> {
        u16::read(read).map(f16::from_bits)
    }

    fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> Result<()> {
        let bits = slice.reinterpret_cast_mut();
        u16::read_slice(read, bits)
    }

    fn write(self, write: &mut impl Write) -> Result<()> {
        self.to_bits().write(write)
    }

    fn write_slice(write: &mut impl Write, slice: &[Self]) -> Result<()> {
        let bits = slice.reinterpret_cast();
        u16::write_slice(write, bits)
    }
}


#[cfg(test)]
mod test {
    use crate::io::PeekRead;
    use std::io::Read;

    #[test]
    fn peek(){
        use lebe::prelude::*;
        let buffer: &[u8] = &[0,1,2,3];
        let mut peek = PeekRead::new(buffer);

        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(u8::read_from_little_endian(&mut peek).unwrap(), 0_u8); // TODO rename to "read u8 from little endian"?

        assert_eq!(peek.read(&mut [0,0]).unwrap(), 2);

        assert_eq!(peek.peek_u8().as_ref().unwrap(), &3);
        assert_eq!(u8::read_from_little_endian(&mut peek).unwrap(), 3_u8);

        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());

        assert!(u8::read_from_little_endian(&mut peek).is_err());
    }
}


