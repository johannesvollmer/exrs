use ::file::meta::attributes::PixelType;
use ::smallvec::SmallVec;
use super::uncompressed::*;


#[derive(Debug, Clone)]
pub enum Error {
}

pub type Result<T> = ::std::result::Result<T, Error>;
pub type CompressedData = Vec<u8>;
pub type UncompressedData = DataBlock;







#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Compression {
    /// store uncompressed values
    /// (loading and writing may be faster than any compression, but file is larger)
    None,

    /// run-length-encode horizontal differences one line at a time
    RLE,

    /// zip horizontal differences one line at a time
    ZIPS,

    /// zip horizontal differences of 16 lines at once
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


pub fn compress(method: Compression, data: &UncompressedData) -> Result<CompressedData> {
    use self::Compression::*;
    match method {
        None => uncompressed::pack(data),
        ZIP => zip::compress(data),
        ZIPS => zip::compress(data),
        _ => unimplemented!()
    }
}

pub fn decompress(
    method: Compression,
    block_description: BlockDescription,

    data: &CompressedData,
    uncompressed_size: Option<usize>,
)
    -> Result<UncompressedData>
{
    use self::Compression::*;
    match method {
        None => uncompressed::unpack(block_description, data),
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

pub mod uncompressed {
    use super::*;

    pub fn unpack(block_description: BlockDescription, data: &CompressedData) -> Result<UncompressedData> {
        match block_description.kind {
            BlockKind::ScanLine => {
                let mut per_channel_data = PerChannel::new();
                let lines_per_block = Compression::None.scan_lines_per_block();
                let map_level_x = unimplemented!("are mip map levels only for tiles?");
                let map_level_y = unimplemented!();


                for channel in block_description.channels {
                    let sampling_y = channel.sampling_y;
                    let sampling_x = channel.sampling_x;

                    let size = unimplemented!("calculate size based on tile size / scan line, taking care of edge cases, channel subsampling, and mip / rip map levels");

                    match channel.pixel_type {
                        PixelType::U32 => {
                            per_channel_data.push(Array::U32(
                                ::file::io::read_u32_vec(&mut data.as_slice(), size, ::std::u16::MAX as usize)
                                    .expect("io err when reading from in-memory vec")
                                    .into_boxed_slice()
                            ));
                        },
                        PixelType::F16 => {
                            per_channel_data.push(Array::F16(
                                ::file::io::read_f16_array(&mut data.as_slice(), size, ::std::u16::MAX as usize)
                                    .expect("io err when reading from in-memory vec")
                                    .into_boxed_slice()
                            ));
                        },
                        PixelType::F32 => {
                            per_channel_data.push(Array::F32(
                                ::file::io::read_f32_vec(&mut data.as_slice(), size, ::std::u16::MAX as usize)
                                    .expect("io err when reading from in-memory vec")
                                    .into_boxed_slice()
                            ));
                        },
                    }
                }

                Ok(DataBlock::ScanLine(per_channel_data))

            },
            BlockKind::Tile => {
                unimplemented!()
            },
            BlockKind::DeepScanLine => {
                unimplemented!()
            },
            BlockKind::DeepTile => {
                unimplemented!()
            }
        }
    }

    pub fn pack(_data: &UncompressedData) -> Result<CompressedData> {
        unimplemented!()
    }
}


/// compresses 16 scan lines at once or
/// compresses 1 single scan line at once
pub mod zip {
    use super::*;
    use std::io::{self, Read};
    use ::libflate::zlib::{Encoder, Decoder};

    pub fn decompress(data: &CompressedData, uncompressed_size: Option<usize>) -> Result<UncompressedData> {
        let mut decoder = Decoder::new(data.as_slice())
            .expect("io error when reading from in-memory vec");

        let mut decompressed = Vec::with_capacity(uncompressed_size.unwrap_or(32));
        decoder.read_to_end(&mut decompressed).expect("io error when reading from in-memory vec");
        unimplemented!("sum up because we encoded the first derivative");
//        super::uncompressed::unpack(decompressed)
    }

    pub fn compress(data: &UncompressedData) -> Result<CompressedData> {
        unimplemented!("encode the first derivative");
        let mut encoder = Encoder::new(Vec::with_capacity(128))
            .expect("io error when writing to in-memory vec");

        let packed = super::uncompressed::pack(data)?;
        io::copy(&mut packed.as_slice(), &mut encoder).expect("io error when writing to in-memory vec");
        Ok(encoder.finish().into_result().expect("io error when writing to in-memory vec"))
    }
}
