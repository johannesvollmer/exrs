

pub type Result<T> = std::result::Result<T, Error>;
pub type PassiveResult = Result<()>;

pub use std::io::Error as IoError;
pub use std::io::Result as IoResult;


#[derive(Debug)] // TODO derive Display?
pub enum Error {
    NotSupported(String),
    Invalid(String),

    /// This error can also occur when reading invalid files,
    /// where the number of bytes to read does not match the input stream length.
    // TODO this can be detected in the IO module!
    Io(IoError),
}


impl Error {
    pub fn invalid(message: impl Into<String>) -> Self {
        Error::Invalid(message.into())
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Error::NotSupported(message.into())
    }
}


/// Enable using the `?` operator on io::Result
impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        Error::Io(error)
    }
}
