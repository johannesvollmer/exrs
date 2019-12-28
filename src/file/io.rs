

pub use ::std::io::{Read, Write};
use half::slice::{HalfFloatSliceExt};
use lebe::prelude::*;
use ::half::f16;



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
        self.peeked = self.peeked.take().or_else(|| Some(self.inner.read_u8_from_little_endian()));
        self.peeked.as_ref().unwrap()
    }

    pub fn skip_if_eq(&mut self, value: u8) -> std::io::Result<bool> {
        match self.peek_u8() {
            Ok(peeked) if *peeked == value =>  {
                self.read_u8_from_little_endian().unwrap(); // skip, will be Ok(value)
                Ok(true)
            },

            Ok(_) => Ok(false),
            Err(_) => Err(self.read_u8_from_little_endian().err().unwrap())
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

    fn read(read: &mut impl Read) -> std::io::Result<Self>;
    fn read_slice(read: &mut impl Read, slice: &mut[Self]) -> std::io::Result<()>;

    fn read_vec(read: &mut impl Read, data_size: usize, estimated_max: usize) -> std::io::Result<Vec<Self>> {
        let mut vec = Vec::new();
        Self::read_into_vec(read, &mut vec, data_size, estimated_max)?;
        Ok(vec)
    }

    fn write(self, write: &mut impl Write) -> std::io::Result<()>;
    fn write_slice(write: &mut impl Write, slice: &[Self]) -> std::io::Result<()>;

    fn byte_size(self) -> usize {
        ::std::mem::size_of::<Self>()
    }

    fn read_into_vec(read: &mut impl Read, data: &mut Vec<Self>, data_size: usize, estimated_max: usize) -> std::io::Result<()> {
        let start = data.len();
        let end = start + data_size;
        let max_end = start + estimated_max;

        debug_assert!(data_size < estimated_max, "suspiciously large data size: {}", data_size);

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

    fn write_i32_sized_slice<W: Write>(write: &mut W, slice: &[Self]) -> std::io::Result<()> {
        (slice.len() as i32).write(write)?;
        Self::write_slice(write, slice)
    }

    fn read_i32_sized_vec(read: &mut impl Read, estimated_max: usize) -> std::io::Result<Vec<Self>> {
        let size = i32::read(read)?;
        if size < 0 { unimplemented!() }
        Self::read_vec(read, size as usize, estimated_max)
    }
}


macro_rules! implement_data_for_primitive {
    ($kind: ident) => {
        impl Data for $kind {
            fn read(read: &mut impl Read) -> std::io::Result<Self> {
                read.read_from_little_endian()
            }

            fn write(self, write: &mut impl Write) -> std::io::Result<()> {
                write.write_as_little_endian(&self)
            }

            fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> std::io::Result<()> {
                read.read_from_little_endian_into(slice)
            }

            fn write_slice(write: &mut impl Write, slice: &[Self]) -> std::io::Result<()> {
                write.write_as_little_endian(slice)
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
    fn read(read: &mut impl Read) -> std::io::Result<Self> {
        u16::read(read).map(f16::from_bits)
    }

    fn read_slice(read: &mut impl Read, slice: &mut [Self]) -> std::io::Result<()> {
        let bits = slice.reinterpret_cast_mut();
        u16::read_slice(read, bits)
    }

    fn write(self, write: &mut impl Write) -> std::io::Result<()> {
        self.to_bits().write(write)
    }

    fn write_slice(write: &mut impl Write, slice: &[Self]) -> std::io::Result<()> {
        let bits = slice.reinterpret_cast();
        u16::write_slice(write, bits)
    }
}


#[cfg(test)]
mod test {
    use crate::file::io::PeekRead;
    use std::io::Read;

    #[test]
    fn peek(){
        use lebe::prelude::*;
        let buffer: &[u8] = &[0,1,2,3];
        let mut peek = PeekRead::new(buffer);

        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.peek_u8().as_ref().unwrap(), &0);
        assert_eq!(peek.read_u8_from_little_endian().unwrap(), 0_u8); // TODO rename to "read u8 from little endian"?

        assert_eq!(peek.read(&mut [0,0]).unwrap(), 2);

        assert_eq!(peek.peek_u8().as_ref().unwrap(), &3);
        assert_eq!(peek.read_u8_from_little_endian().unwrap(), 3_u8);

        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());
        assert!(peek.peek_u8().is_err());

        assert!(peek.read_u8_from_little_endian().is_err());
    }
}


