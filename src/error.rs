
use std::borrow::Cow;
use std::io::ErrorKind;



pub type Result<T> = std::result::Result<T, Error>;
pub type PassiveResult = Result<()>;

pub use std::io::Error as IoError;
pub use std::io::Result as IoResult;

#[derive(Debug)] // TODO derive Display?
pub enum Error {
    /// The contents of the file are not supported by this implementation of open exr
    NotSupported(Cow<'static, str>),

    /// The contents of the file are corrupt or insufficient
    Invalid(Cow<'static, str>),

    /// The underlying byte stream could not be read correctly
    Io(IoError),
}


impl Error {
    pub fn invalid(message: impl Into<Cow<'static, str>>) -> Self {
        Error::Invalid(message.into())
    }

    pub fn unsupported(message: impl Into<Cow<'static, str>>) -> Self {
        Error::NotSupported(message.into())
    }
}


/// Enable using the `?` operator on io::Result
impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        if error.kind() == ErrorKind::UnexpectedEof {
            Error::invalid("content size")
        }

        else {
            Error::Io(error)
        }
    }
}


#[inline]
pub(crate) fn i32_to_usize(value: i32) -> Result<usize> {
    if value < 0 { Err(Error::invalid("number sign")) }
    else { Ok(value as usize) }
}

#[inline]
pub(crate) fn i32_to_u32(value: i32) -> Result<u32> {
    if value < 0 { Err(Error::invalid("number sign")) }
    else { Ok(value as u32) }
}

/*#[inline]
pub(crate) fn i32_to_usize_at(value: i32, context: &'static str) -> Result<usize> {
    if value < 0 { Err(Error::invalid(context)) }
    else { Ok(value as usize) }
}*/

#[inline]
pub(crate) fn i32_to_u32_at(value: i32, context: &'static str) -> Result<u32> {
    if value < 0 { Err(Error::invalid(context)) }
    else { Ok(value as u32) }
}