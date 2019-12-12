use self::validity::*;
use crate::file::data::compression::{Error as CompressionError};

pub type WriteResult = ::std::result::Result<(), WriteError>;
pub type ReadResult<T> = ::std::result::Result<T, ReadError>;


#[derive(Debug)]
pub enum WriteError {
    CompressionError(CompressionError),
    IoError(::std::io::Error),
    Invalid(Invalid),
}


// TODO implement Display for all errors
#[derive(Debug)]
pub enum ReadError {
    NotEXR,
    Invalid(Invalid),
//    UnknownAttributeType { bytes_to_skip: u32 },

    IoError(::std::io::Error),
    CompressionError(Box<CompressionError>),
}


/// Enable using the `?` operator on io::Result
impl From<::std::io::Error> for ReadError {
    fn from(io_err: ::std::io::Error) -> Self {
        ReadError::IoError(io_err)
    }
}

/// Enable using the `?` operator on compress::Result
impl From<CompressionError> for ReadError {
    fn from(compress_err: CompressionError) -> Self {
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

pub mod validity {
    // TODO put validation into own module
    pub type Validity = Result<(), Invalid>;

    #[derive(Debug, Clone, Copy)]
    pub enum Invalid {
        Missing(Value),
        NotSupported(&'static str),
        Combination(&'static [Value]),
        Content(Value, Required),
        Type(Required),
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Value {
        Attribute(&'static str),
        Version(&'static str),
        Chunk(&'static str),
        Type(&'static str),
        Part(&'static str),
        Enum(&'static str),
        Text,
        MapLevel,
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Required {
        Max(usize),
        Min(usize),
        Exact(&'static str),
        OneOf(&'static [&'static str]),
        Range {
            /// inclusive
            min: usize,

            /// inclusive
            max: usize
        },
    }
}
