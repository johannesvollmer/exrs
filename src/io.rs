

pub use ::std::io::{Read, Write};
use half::slice::{HalfFloatSliceExt};
use lebe::prelude::*;
use ::half::f16;
use crate::error::{Error, Result, PassiveResult, IoResult};
use std::io::{Seek, SeekFrom};

/// Skip reading uninteresting bytes without allocating.
#[inline]
pub fn skip_bytes(read: &mut impl Read, count: u64) -> IoResult<()> {
    let skipped = std::io::copy(
        &mut read.by_ref().take(count),
        &mut std::io::sink()
    )?;

    debug_assert_eq!(skipped, count);
    Ok(())
}

/// Peek a single byte without consuming it.
#[derive(Debug)]
pub struct PeekRead<T> {
    inner: T,
    peeked: Option<IoResult<u8>>,
}

impl<T: Read> PeekRead<T> {
    #[inline]
    pub fn new(inner: T) -> Self {
        Self { inner, peeked: None }
    }

    /// Read a single byte and return that without consuming it.
    #[inline]
    pub fn peek_u8(&mut self) -> &IoResult<u8> {
        self.peeked = self.peeked.take().or_else(|| Some(u8::read_from_little_endian(&mut self.inner)));
        self.peeked.as_ref().unwrap()
    }

    /// Skip a single byte if it equals the specified value.
    /// Returns whether the value was found.
    #[inline]
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

impl<T: Read + Seek> PeekRead<Tracking<T>> {
    pub fn skip_to(&mut self, position: usize) -> std::io::Result<()> {
        self.inner.seek_read_to(position)?;
        self.peeked = None;
        Ok(())
    }
}

/// Keep track of what byte we are at.
/// Used to skip back to a previous place after writing some information.
#[derive(Debug)]
pub struct Tracking<T> {
    inner: T,
    position: usize,
}

impl<T: Read> Read for Tracking<T> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.position += count;
        Ok(count)
    }
}

impl<T: Write> Write for Tracking<T> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let count = self.inner.write(buffer)?;
        self.position += count;
        Ok(count)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl<T> Tracking<T> {
    pub fn new(inner: T) -> Self {
        Tracking { inner, position: 0 }
    }

    pub fn byte_position(&self) -> usize {
        self.position
    }
}

impl<T: Read + Seek> Tracking<T> {
    pub fn seek_read_to(&mut self, target_position: usize) -> std::io::Result<()> {
        let delta = target_position as i64 - self.position as i64;

        if delta > 0 && delta < 16 { // TODO profile that this is indeed faster than a syscall! (should be because of bufread buffer discard)
            skip_bytes(self, delta as u64)?;
            self.position += delta as usize;
        }
        else if delta != 0 {
            self.inner.seek(SeekFrom::Start(target_position as u64))?;
            self.position = target_position;
        }

        Ok(())
    }
}

impl<T: Write + Seek> Tracking<T> {
    pub fn seek_write_to(&mut self, target_position: usize) -> std::io::Result<()> {
        if target_position < self.position {
            self.inner.seek(SeekFrom::Start(target_position as u64))?;
        }
        else if target_position > self.position {
            std::io::copy(
                &mut std::io::repeat(0).take((target_position - self.position) as u64),
                self
            )?;
        }

        self.position = target_position;
        Ok(())
    }
}


/// extension trait for primitive types like numbers and arrays
pub trait Data: Sized + Default + Clone {
    const BYTE_SIZE: usize = ::std::mem::size_of::<Self>();

    fn read(read: &mut impl Read) -> Result<Self>;

    fn read_slice(read: &mut impl Read, slice: &mut[Self]) -> PassiveResult;

    /// If a block length greater than this number is decoded,
    /// it will not try to allocate that much memory, but instead consider
    /// that decoding the block length has gone wrong.
    #[inline]
    fn read_vec(read: &mut impl Read, data_size: usize, soft_max: usize, hard_max: Option<usize>) -> Result<Vec<Self>> {
        let mut vec = Vec::new();
        Self::read_into_vec(read, &mut vec, data_size, soft_max, hard_max)?;
        Ok(vec)
    }

    fn write(self, write: &mut impl Write) -> PassiveResult;

    fn write_slice(write: &mut impl Write, slice: &[Self]) -> PassiveResult;


    /// If a block length greater than this number is decoded,
    /// it will not try to allocate that much memory, but instead consider
    /// that decoding the block length has gone wrong.
    #[inline]
    fn read_into_vec(read: &mut impl Read, data: &mut Vec<Self>, data_size: usize, soft_max: usize, hard_max: Option<usize>) -> PassiveResult {
        if let Some(max) = hard_max {
            debug_assert!(data_size <= max, "large data: {} (max {})", data_size, max);

            if data_size > max {
                return Err(Error::invalid("content size"))
            }
        }

        let soft_max = hard_max.unwrap_or(soft_max).min(soft_max);
        let end = data.len() + data_size;

        // do not allocate more than $chunks memory at once
        // (most of the time, this loop will run only once)
        while data.len() < end {
            let chunk_start = data.len();
            let chunk_end = (chunk_start + soft_max).min(data_size);

            data.resize(chunk_end, Self::default());
            Self::read_slice(read, &mut data[chunk_start .. chunk_end])?;
        }

        Ok(())
    }

    #[inline]
    fn write_i32_sized_slice<W: Write>(write: &mut W, slice: &[Self]) -> PassiveResult {
        (slice.len() as i32).write(write)?;
        Self::write_slice(write, slice)
    }

    #[inline]
    fn read_i32_sized_vec(read: &mut impl Read, soft_max: usize, hard_max: Option<usize>) -> Result<Vec<Self>> {
        let size = i32::read(read)?;
        debug_assert!(size >= 0);

        if size < 0 { Err(Error::invalid("negative array size")) }
        else { Self::read_vec(read, size as usize, soft_max, hard_max) }
    }
}


macro_rules! implement_data_for_primitive {
    ($kind: ident) => {
        impl Data for $kind {
            #[inline]
            fn read(read: &mut impl Read) -> Result<Self> {
                Ok(read.read_from_little_endian()?)
            }

            #[inline]
            fn write(self, write: &mut impl Write) -> Result<()> {
                write.write_as_little_endian(&self)?;
                Ok(())
            }

            #[inline]
            fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> Result<()> {
                read.read_from_little_endian_into(slice)?;
                Ok(())
            }

            #[inline]
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
    #[inline]
    fn read(read: &mut impl Read) -> Result<Self> {
        u16::read(read).map(f16::from_bits)
    }

    #[inline]
    fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> Result<()> {
        let bits = slice.reinterpret_cast_mut();
        u16::read_slice(read, bits)
    }

    #[inline]
    fn write(self, write: &mut impl Write) -> Result<()> {
        self.to_bits().write(write)
    }

    #[inline]
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


