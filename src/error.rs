//! Error type definitions.

use std::borrow::Cow;
use std::convert::TryFrom;
use std::error;
use std::fmt;
pub use std::io::Error as IoError;
use std::io::ErrorKind;
pub use std::io::Result as IoResult;
use std::num::TryFromIntError;

// Export types

/// A result that may contain an exr error.
pub type Result<T> = std::result::Result<T, Error>;

/// A result that, if ok, contains nothing, and otherwise contains an exr error.
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
    Aborted, // FIXME remove?? is not used really?

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
        Self::Invalid(message.into())
    }

    /// Create an error of the variant `NotSupported`.
    pub(crate) fn unsupported(message: impl Into<Cow<'static, str>>) -> Self {
        Self::NotSupported(message.into())
    }
}

/// Enable using the `?` operator on `std::io::Result`.
impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        if error.kind() == ErrorKind::UnexpectedEof {
            Self::invalid("reference to missing bytes")
        } else {
            Self::Io(error)
        }
    }
}

// TODO use `usize::try_from(x)?` everywhere
// Note: This provides a generic error message for integer conversion failures.
// For better error messages, prefer using `.map_err(|_| Error::invalid(format!("specific context")))
// instead of relying on this From impl.
impl From<TryFromIntError> for Error {
    fn from(_: TryFromIntError) -> Self {
        Self::invalid("integer conversion failed: value out of range")
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            Self::Io(ref err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => err.fmt(formatter),
            Self::NotSupported(message) => write!(formatter, "not supported: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid: {message}"),
            Self::Aborted => write!(formatter, "cancelled"),
        }
    }
}

/// Typically used during decoding. Return error on invalid range.
#[inline]
pub(crate) fn i32_to_usize(value: i32, error_message: &'static str) -> Result<usize> {
    usize::try_from(value).map_err(|_| {
        if value < 0 {
            Error::invalid(error_message)
        } else {
            Error::unsupported(error_message)
        }
    })
}

/// Typically used during encoding. Return error on overflow.
#[inline]
pub(crate) fn usize_to_u32(value: usize, error_message: &'static str) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::invalid(error_message))
}

/// Typically used during encoding. Return error on invalid range.
#[inline]
pub(crate) fn usize_to_u16(value: usize, error_message: &'static str) -> Result<u16> {
    u16::try_from(value).map_err(|_| Error::invalid(error_message))
}

/// Typically used during decoding. Return error on invalid range.
#[inline]
pub(crate) fn u64_to_usize(value: u64, error_message: &'static str) -> Result<usize> {
    usize::try_from(value).map_err(|_| Error::unsupported(error_message))
}

/// Typically used during decoding. Return error on invalid range.
#[inline]
pub(crate) fn u32_to_usize(value: u32, error_message: &'static str) -> Result<usize> {
    usize::try_from(value).map_err(|_| Error::unsupported(error_message))
}

/// Typically used during encoding. Return error on invalid range.
#[inline]
pub(crate) fn usize_to_i32(value: usize, error_message: &'static str) -> Result<i32> {
    i32::try_from(value).map_err(|_| Error::invalid(error_message))
}

/// Typically used during encoding. Return error on invalid range.
#[inline]
pub(crate) fn usize_to_u64(value: usize, error_message: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| Error::invalid(error_message))
}
