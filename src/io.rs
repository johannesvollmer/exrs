
//! Specialized binary input and output.
//! Uses the error handling for this crate.

pub use ::std::io::{Read, Write};
use half::slice::{HalfFloatSliceExt};
use lebe::prelude::*;
use ::half::f16;
use crate::error::{Error, Result, PassiveResult, IoResult};
use std::io::{Seek, SeekFrom};

/// Skip reading uninteresting bytes without allocating.
#[inline]
pub fn skip_bytes(read: &mut impl Read, count: usize) -> IoResult<()> {
    let skipped = std::io::copy(
        &mut read.by_ref().take(count as u64),
        &mut std::io::sink()
    )?;

    debug_assert_eq!(skipped, count as u64);
    Ok(())
}

/// Peek a single byte without consuming it.
#[derive(Debug)]
pub struct PeekRead<T> {
    /// Cannot be exposed as it will not contain peeked values anymore.
    inner: T,

    peeked: Option<IoResult<u8>>,
}

impl<T: Read> PeekRead<T> {

    #[inline]
    pub fn new(inner: T) -> Self {
        Self { inner, peeked: None }
    }

    /// Read a single byte and return that without consuming it.
    /// The next `read` call will include that byte.
    #[inline]
    pub fn peek_u8(&mut self) -> &IoResult<u8> {
        self.peeked = self.peeked.take().or_else(|| Some(u8::read_from_little_endian(&mut self.inner)));
        self.peeked.as_ref().unwrap() // unwrap cannot fail because we just set it
    }

    /// Skip a single byte if it equals the specified value.
    /// Returns whether the value was found.
    /// Consumes the peeked result if an error occurred.
    #[inline]
    pub fn skip_if_eq(&mut self, value: u8) -> IoResult<bool> {
        match self.peek_u8() {
            Ok(peeked) if *peeked == value =>  {
                self.peeked = None; // consume the byte
                Ok(true)
            },

            Ok(_) => Ok(false),

            // return the error otherwise.
            // unwrap is safe because this branch cannot be reached otherwise.
            // we need to take() from self because io errors cannot be cloned.
            Err(_) => Err(self.peeked.take().unwrap().err().unwrap())
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

                // indexing [1..] is safe because an empty buffer already returned ok
                Ok(1 + self.inner.read(&mut target_buffer[1..])?)
            }
        }
    }
}

impl<T: Read + Seek> PeekRead<Tracking<T>> {
    /// Seek this read to the specified byte position.
    /// Discards any previously peeked value.
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

    /// Do not expose to prevent seeking without updating position
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

    /// If `inner` is a reference, if must never be seeked directly,
    /// but only through this `Tracking` instance.
    pub fn new(inner: T) -> Self {
        Tracking { inner, position: 0 }
    }

    /// Current number of bytes written or read.
    pub fn byte_position(&self) -> usize {
        self.position
    }
}

impl<T: Read + Seek> Tracking<T> {

    /// Set the reader to the specified byte position.
    /// If it is only a couple of bytes, no seek system call is performed.
    pub fn seek_read_to(&mut self, target_position: usize) -> std::io::Result<()> {
        let delta = target_position as i64 - self.position as i64;

        if delta > 0 && delta < 16 { // TODO profile that this is indeed faster than a syscall! (should be because of bufread buffer discard)
            skip_bytes(self, delta as usize)?;
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

    /// Move the writing cursor to the specified target byte index.
    /// If seeking forward, this will write zeroes.
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


/// Generic trait that defines common binary operations such as reading and writing for this type.
pub trait Data: Sized + Default + Clone {
    const BYTE_SIZE: usize = ::std::mem::size_of::<Self>();

    /// Read a value of type `Self`.
    fn read(read: &mut impl Read) -> Result<Self>;

    /// Read as many values of type `Self` as fit into the specified slice.
    /// If the slice cannot be filled completely, returns `Error::Invalid`.
    fn read_slice(read: &mut impl Read, slice: &mut[Self]) -> PassiveResult;

    /// Read as many values of type `Self` as specified with `data_size`.
    ///
    /// This method will not allocate more memory than `soft_max` at once.
    /// If `hard_max` is specified, it will never read any more than that.
    /// Returns `Error::Invalid` if reader does not contain the desired number of elements.
    #[inline]
    fn read_vec(read: &mut impl Read, data_size: usize, soft_max: usize, hard_max: Option<usize>) -> Result<Vec<Self>> {
        let mut vec = Vec::new();
        Self::read_into_vec(read, &mut vec, data_size, soft_max, hard_max)?;
        Ok(vec)
    }

    /// Write this value to the writer.
    fn write(self, write: &mut impl Write) -> PassiveResult;

    /// Write all values of that slice to the writer.
    fn write_slice(write: &mut impl Write, slice: &[Self]) -> PassiveResult;


    /// Read as many values of type `Self` as specified with `data_size` into the provided vector.
    ///
    /// This method will not allocate more memory than `soft_max` at once.
    /// If `hard_max` is specified, it will never read any more than that.
    /// Returns `Error::Invalid` if reader does not contain the desired number of elements.
    #[inline]
    fn read_into_vec(read: &mut impl Read, data: &mut Vec<Self>, data_size: usize, soft_max: usize, hard_max: Option<usize>) -> PassiveResult {
        if let Some(max) = hard_max {
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
            Self::read_slice(read, &mut data[chunk_start .. chunk_end])?; // safe because of `min(data_size)``
        }

        Ok(())
    }

    /// Write the length of the slice and then its contents.
    #[inline]
    fn write_i32_sized_slice<W: Write>(write: &mut W, slice: &[Self]) -> PassiveResult {
        i32::write(slice.len() as i32, write)?;
        Self::write_slice(write, slice)
    }

    /// Read the desired element count and then read that many items into a vector.
    ///
    /// This method will not allocate more memory than `soft_max` at once.
    /// If `hard_max` is specified, it will never read any more than that.
    /// Returns `Error::Invalid` if reader does not contain the desired number of elements.
    #[inline]
    fn read_i32_sized_vec(read: &mut impl Read, soft_max: usize, hard_max: Option<usize>) -> Result<Vec<Self>> {
        let size = i32::read(read)?;
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


