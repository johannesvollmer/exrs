
#[derive(Debug, Clone)]
pub enum CompressionError {
    ZipError(String),
}

pub type Result = ::std::result::Result<Data, CompressionError>;
pub type Data = Vec<u8>;



#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Compression {
    /// store uncompressed values
    /// (loading and writing may be faster than any compression, but file is larger)
    None,

    /// run-length-encode horizontal differences one line at a time
    RLE,

    /// zip horizontal differences one line at a time
    ZIPS,

    /// zip horizontal differences of 16 lines
    ZIP,

    /// wavelet??
    PIZ,

    /// lossy!
    PXR24,

    /// lossy!
    B44,

    /// lossy!
    B44A,

    /* TODO: DWAA & DWAB */
}


// needs ownership to return immediately in case of Compression::None
pub fn compress(method: Compression, data: Data) -> Result {
    use self::Compression::*;
    match method {
        None => Ok(data),
        ZIP => zip::compress(data),
        ZIPS => zip::compress(data),
        _ => unimplemented!()
    }
}

// needs ownership to return immediately in case of Compression::None
pub fn decompress(method: Compression, data: Data, uncompressed_size: Option<usize>) -> Result {
    use self::Compression::*;
    match method {
        None => Ok(data),
        ZIP => zip::decompress(data, uncompressed_size),
        ZIPS => zip::decompress(data, uncompressed_size),
        RLE => unimplemented!(),
        _ => unimplemented!()
    }
}

impl Compression {
    /// For scan line images and deep scan line images, one or more scan lines may be
    /// stored together as a scan line block. The number of scan lines per block
    /// depends on how the pixel data are compressed
    pub fn scan_lines_per_block(self) -> usize {
        use self::Compression::*;
        match self {
            None | RLE   | ZIPS => 1,
            ZIP  | PXR24        => 16,
            PIZ  | B44   | B44A => 32,
            /* TODO: DWAA & DWAB */
        }
    }

    pub fn supports_deep_data(self) -> bool {
        use self::Compression::*;
        match self {
            None | RLE | ZIPS | ZIP => true,
            _ => false,
        }
    }
}


/// compresses 16 scan lines at once or
/// compresses 1 single scan line at once
pub mod zip {
    use super::*;
    use std::io::{self, Read};
    use ::libflate::zlib::{Encoder, Decoder};

    pub fn decompress(data: Data, uncompressed_size: Option<usize>) -> Result {
        let mut decoder = Decoder::new(data.as_slice())
            .expect("io error when reading from in-memory vec");

        let mut decompressed = Vec::with_capacity(uncompressed_size.unwrap_or(32));
        decoder.read_to_end(&mut decompressed).expect("io error when reading from in-memory vec");
        // sum up because we encoded the first derivative
        unimplemented!("needs to sum up u32 / F16 /F32")
    }

    pub fn compress(data: Data) -> Result {
        unimplemented!("needs to encode differences of pixels as u32 / F16 /F32");
        let mut encoder = Encoder::new(Vec::with_capacity(data.len() / 2))
            .expect("io error when writing to in-memory vec");

        io::copy(&mut data.as_slice(), &mut encoder).expect("io error when writing to in-memory vec");
        Ok(encoder.finish().into_result().expect("io error when writing to in-memory vec"))
    }
}
