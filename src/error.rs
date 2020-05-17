
//! Error type definitions.

use std::borrow::Cow;
use std::io::ErrorKind;
pub use std::io::Error as IoError;
pub use std::io::Result as IoResult;
use std::convert::TryFrom;
use std::error;
use std::fmt;

// Export types

/// A result that may contain an exr error.
pub type Result<T> = std::result::Result<T, Error>;

/// A that, if ok, contains nothing, and otherwise contains an exr error.
pub type UnitResult = Result<()>;


/// An error that may happen while reading or writing an exr file.
/// Distinguishes between three types of errors:
/// unsupported features, invalid data, and file system errors.
#[derive(Debug)]
pub enum Error {

    /// Reading or Writing the file has been aborted by the caller.
    /// This error will never be triggered by this crate itself,
    /// only by users of this library.
    /// It exists to be returned from a progress callback.
    Aborted,

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

/// Enable using the `?` operator on `std::io::Result`.
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

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Error::Io(ref err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => err.fmt(formatter),
            Error::NotSupported(message) => write!(formatter, "not supported: {}", message),
            Error::Invalid(message) => write!(formatter, "invalid: {}", message),
            Error::Aborted => write!(formatter, "cancelled"),
        }
    }
}

/// Return error on invalid range.
#[inline]
pub(crate) fn i32_to_usize(value: i32, error_message: &'static str) -> Result<usize> {
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
