
//! Error type definitions.

use std::borrow::Cow;
use std::io::ErrorKind;

pub type Result<T> = std::result::Result<T, Error>;
pub type PassiveResult = Result<()>;

pub use std::io::Error as IoError;
pub use std::io::Result as IoResult;
use std::convert::TryFrom;
pub type StrLiteral = &'static str;

/// An error that may happen while reading or writing an exr file.
/// Distinguishes between three types of errors:
/// unsupported features, invalid data, and file system errors.
#[derive(Debug)]
pub enum Error {

    /// The contents of the file are not supported by
    /// this specific implementation of open exr,
    /// even though the data may be valid.
    NotSupported(Cow<'static, str>),

    /// The contents of the image are contradicting or insufficient.
    /// Also returned for `ErrorKind::UnexpectedEof` errors.
    Invalid(Cow<'static, str>),

    /// The underlying byte stream could not be read successfully,
    /// probably due to file system related errors.
    Io(IoError),
}


impl Error {
    /// Create an error of the variant `Invalid`.
    pub(crate) fn invalid(message: impl Into<Cow<'static, str>>) -> Self {
        Error::Invalid(message.into())
    }

    /// Create an error of the variant `NotSupported`.
    pub(crate) fn unsupported(message: impl Into<Cow<'static, str>>) -> Self {
        Error::NotSupported(message.into())
    }
}

/// Enable using the `?` operator on `exr::io::Result`.
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


/// Return error on invalid range.
#[inline]
pub(crate) fn i32_to_usize(value: i32, error_message: StrLiteral) -> Result<usize> {
    usize::try_from(value).map_err(|_| Error::invalid(error_message))
}

/// Panic on overflow.
#[inline]
pub(crate) fn u64_to_usize(value: u64) -> usize {
    usize::try_from(value).expect("(u64 as usize) overflowed")
}

/// Panic on overflow.
#[inline]
pub(crate) fn usize_to_i32(value: usize) -> i32 {
    i32::try_from(value).expect("(usize as i32) overflowed")
}


/*#[inline]
pub(crate) fn i32_to_u32(value: i32, error_message: StrLiteral) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::invalid(error_message))
}*/
