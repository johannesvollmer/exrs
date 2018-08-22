use ::std::io::Write;
use super::RawImage;

pub type Result = ::std::result::Result<(), Error>;

#[derive(Debug, Clone)]
pub enum Error {
    IoError(::std::io::Error),
}

/// enable using the `?` operator on io errors
impl From<::std::io::Error> for Error {
    fn from(err: ::std::io::Error) -> Self {
        Error::IoError(err)
    }
}


fn identify_exr<W: Write>(write: &mut W) -> Result {
    write.write(&super::MAGIC_NUMBER)?;
    Ok(())
}


#[must_use]
pub fn write_file(path: &str, image: &RawImage) -> Result {
    write(::std::fs::File::open(path)?, image)
}

#[must_use]
pub fn write<W: Write>(write: &mut W, image: &RawImage) -> Result {
    identify_exr(write)?;

    Ok(())
}
