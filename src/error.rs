
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
