
#[derive(Debug, Clone)]
pub enum Error {
    ZipError(String)
}

pub type Result = ::std::result::Result<Vec<u8>, Error>;
pub type Data = Vec<u8>;



#[derive(Debug, Clone, Copy)]
pub enum Compression {
    None, RLE, ZIPSingle,
    ZIP, PIZ, PXR24,
    B44, B44A,
}


pub fn compress(method: Compression, data: Data) -> Result {
    use self::Compression::*;
    match method {
        None => Ok(data),
        ZIP => zip::compress(data),
        ZIPSingle => zip_single::compress(data),
        _ => unimplemented!()
    }
}

pub fn decompress(method: Compression, data: Data, uncompressed_size: Option<usize>) -> Result {
    use self::Compression::*;
    match method {
        None => Ok(data),
        ZIP => zip::decompress(data, uncompressed_size),
        ZIPSingle => zip_single::decompress(data, uncompressed_size),
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
            None | RLE   | ZIPSingle    => 1,
            ZIP  | PXR24                => 16,
            PIZ  | B44   | B44A /* TODO: DWAA & DWAB */ => 32,
        }
    }

    pub fn supports_deep_data(self) -> bool {
        use self::Compression::*;
        match self {
            None | RLE | ZIPSingle | ZIP => true,
            _ => false,
        }
    }
}


/// compresses 16 scan lines at once
pub mod zip {
    use super::*;

    pub fn decompress(_data: Data, _uncompressed_size: Option<usize>) -> Result {
        unimplemented!()
    }

    pub fn compress(data: Data) -> Result {
        use ::compression::prelude::*;

        data.into_iter()
            .encode(&mut BZip2Encoder::new(9), Action::Finish)
            .collect::<::std::result::Result<Vec<_>, _>>()
            .map_err(|cerr| Error::ZipError(cerr.to_string()))
    }
}

/// compresses 1 single scan line at once
pub mod zip_single {
    use super::*;

    pub fn decompress(_data: Data, _uncompressed_size: Option<usize>) -> Result {
        unimplemented!()
    }

    pub fn compress(_data: Data) -> Result {
        unimplemented!()
    }
}
