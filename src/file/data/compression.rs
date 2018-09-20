use super::uncompressed::*;

#[derive(Debug)]
pub enum Error {
    Compression(::std::io::Error),
}

impl From<::std::io::Error> for Error {
    fn from(io: ::std::io::Error) -> Self {
        Error::Compression(io)
    }
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
    /// Differences between horizontally adjacent pixels are run-length encoded. This
    /// method is fast, and works well for images with large flat areas, but for photographic
    /// images, the compressed file size is usually between 60 and 75 percent of the
    /// uncompressed size.
    RLE,

    /// "ZIPS": zip horizontal differences, one line at a time
    ZIP1,

    /// "ZIP": zip horizontal differences, 16 lines at a time
    /// Differences between horizontally adjacent pixels are compressed using the open-
    /// source zlib library. ZIP decompression is faster than PIZ decompression, but ZIP
    /// compression is significantly slower. Photographic images tend to shrink to between
    /// 45 and 55 percent of their uncompressed size.
    /// Multi-resolution files are often used as texture maps for 3D renderers. For this
    /// application, fast read accesses are usually more important than fast writes, or
    /// maximum compression. For texture maps, ZIP is probably the best compression
    /// method.
    /// In scan-line based files, 16 rows of pixels are accumulated and compressed
    /// together as a single block.
    ZIP16,

    ///  piz-based wavelet compression
    /// A wavelet transform is applied to the pixel data, and the result is Huffman-
    /// encoded. This scheme tends to provide the best compression ratio for the types of
    /// images that are typically processed at Industrial Light & Magic. Files are
    /// compressed and decompressed at roughly the same speed. For photographic
    /// images with film grain, the files are reduced to between 35 and 55 percent of their
    /// uncompressed size.
    /// PIZ compression works well for scan-line based files, and also for tiled files with
    /// large tiles, but small tiles do not shrink much. (PIZ-compressed data start with a
    /// relatively long header; if the input to the compressor is short, adding the header
    /// tends to offset any size reduction of the input.)
    /// PIZ compression is only supported for flat images.
    PIZ,

    /// lossy!  lossy 24-bit float compression
    /// After reducing 32-bit floating-point data to 24 bits by rounding (while leaving 16-bit
    /// floating-point data unchanged), differences between horizontally adjacent pixels
    /// are compressed with zlib, similar to ZIP. PXR24 compression preserves image
    /// channels of type HALF and UINT exactly, but the relative error of FLOAT data
    /// increases to about
    /// . This compression method works well for depth
    /// buffers and similar images, where the possible range of values is very large, but
    /// where full 32-bit floating-point accuracy is not necessary. Rounding improves
    /// compression significantly by eliminating the pixels' 8 least significant bits, which
    /// tend to be very noisy, and therefore difficult to compress.
    /// PXR24 compression is only supported for flat images.
    PXR24,

    /// lossy!
    /// lossy 4-by-4 pixel block compression,
    /// fixed compression rate
    B44,

    /// lossy!
    /// lossy 4-by-4 pixel block compression,
    /// flat fields are compressed more
    ///
    /// Channels of type HALF are split into blocks of four by four pixels or 32 bytes. Each
    /// block is then packed into 14 bytes, reducing the data to 44 percent of their
    /// uncompressed size. When B44 compression is applied to RGB images in
    /// combination with luminance/chroma encoding (see below), the size of the
    /// compressed pixels is about 22 percent of the size of the original RGB data.
    /// Channels of type UINT or FLOAT are not compressed.
    /// Decoding is fast enough to allow real-time playback of B44-compressed OpenEXR
    /// image sequences on commodity hardware.
    /// The size of a B44-compressed file depends on the number of pixels in the image,
    /// but not on the data in the pixels. All images with the same resolution and the same
    /// set of channels have the same size. This can be advantageous for systems that
    /// support real-time playback of image sequences; the predictable file size makes it
    /// easier to allocate space on storage media efficiently.
    /// B44 compression is only supported for flat images.
    B44A,

    // lossy DCT based compression, in blocks
    // of 32 scanlines. More efficient for partial
// buffer access.Like B44, except for blocks of four by four pixels where all pixels have the same
//value, which are packed into 3 instead of 14 bytes. For images with large uniform
//areas, B44A produces smaller files than B44 compression.
//B44A compression is only supported for flat images.
    DWAA,

    // lossy DCT based compression, in blocks
    // of 256 scanlines. More efficient space
    // wise and faster to decode full frames
// than DWAA_COMPRESSION.
    DWAB,

    /* TODO: DWAA & DWAB */
}



impl Compression {
    pub fn compress(self, data: &UncompressedData) -> Result<CompressedData> {
        use self::Compression::*;
        match self {
            None => uncompressed::pack(data),
            ZIP16 => zip::compress(data),
            ZIP1 => zip::compress(data),
            _ => unimplemented!()
        }
    }

    pub fn decompress(
        self,
        target: UncompressedData,
        // block_description: BlockDescription,

        data: &CompressedData,
        uncompressed_size: Option<usize>,
    )
        -> Result<UncompressedData>
    {
        use self::Compression::*;
        match self {
            None => uncompressed::unpack(target, data),
            ZIP16 => zip::decompress(target, data, uncompressed_size),
            ZIP1 => zip::decompress(target, data, uncompressed_size),
            compr => unimplemented!("decompressing {:?}", compr),
        }
    }

    /// For scan line images and deep scan line images, one or more scan lines may be
    /// stored together as a scan line block. The number of scan lines per block
    /// depends on how the pixel data are compressed
    pub fn scan_lines_per_block(self) -> usize {
        use self::Compression::*;
        match self {
            None  | RLE   | ZIP1        => 1,
            ZIP16 | PXR24               => 16,
            PIZ   | B44   | B44A | DWAA => 32,
            DWAB                        => 256,
        }
    }

    pub fn supports_deep_data(self) -> bool {
        use self::Compression::*;
        match self {
            None | RLE | ZIP1 | ZIP16 => true,
            _ => false,
        }
    }
}

pub mod uncompressed {
    use super::*;

    pub fn unpack(mut target: UncompressedData, data: &CompressedData) -> Result<UncompressedData> {
        match &mut target {
            DataBlock::ScanLine(ref mut scan_line_channels) => {
                for ref mut channel in scan_line_channels.iter_mut() {
                    match channel {
                        Array::U32(ref mut channel) => {
                            ::file::io::read_u32_array(&mut data.as_slice(), channel.as_mut_slice())
                                .expect("io err when reading from in-memory vec");
                        },
                        Array::F16(ref mut channel) => {
                            // TODO don't allocate
                            // TODO cast mut f16 slice as u16 and read u16 array
                            let allocated_vec = ::file::io::read_f16_vec(
                                &mut data.as_slice(), channel.len(), ::std::usize::MAX
                            ).expect("io err when reading from in-memory vec");
                            channel.copy_from_slice(allocated_vec.as_slice());
                        },
                        Array::F32(ref mut channel) => {
                            ::file::io::read_f32_array(&mut data.as_slice(), channel.as_mut_slice())
                                .expect("io err when reading from in-memory vec");
                        },
                    }
                }
            },

            DataBlock::Tile(ref mut tile_channels) => {
                for ref mut channel in tile_channels.iter_mut() {
                    match channel {
                        Array::U32(ref mut channel) => {
                            ::file::io::read_u32_array(&mut data.as_slice(), channel.as_mut_slice())
                                .expect("io err when reading from in-memory vec");
                        },
                        Array::F16(ref mut channel) => {
                            // TODO don't allocate
                            // TODO cast mut f16 slice as u16 and read u16 array
                            let allocated_vec = ::file::io::read_f16_vec(
                                &mut data.as_slice(), channel.len(), ::std::usize::MAX
                            ).expect("io err when reading from in-memory vec");
                            channel.copy_from_slice(allocated_vec.as_slice());
                        },
                        Array::F32(ref mut channel) => {
                            ::file::io::read_f32_array(&mut data.as_slice(), channel.as_mut_slice())
                                .expect("io err when reading from in-memory vec");
                        },
                    }
                }
            },

            _ => unimplemented!()
        }

        Ok(target)
    }

    pub fn pack(_data: &UncompressedData) -> Result<CompressedData> {
        unimplemented!()
    }
}




// see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfCompressor.cpp


/// compresses 16 scan lines at once or
/// compresses 1 single scan line at once
// TODO don't instantiate a new decoder for every block?
pub mod zip {
    use super::*;
    use std::io::{self, Read};
    use ::libflate::zlib::{Encoder, Decoder};


    // inspired by https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfZip.cpp

    pub fn integrate(buffer: &mut [u8]){
//        unsigned char *t    = (unsigned char *) buf + 1;
//        unsigned char *stop = (unsigned char *) buf + outSize;
//        while (t < stop){
//            int d = int (t[-1]) + int (t[0]) - 128;
//            t[0] = d;
//            ++t;
//        }

        // TODO rustify
        for index in 1..buffer.len() {
            buffer[index] = (buffer[index-1] as i32 + buffer[index] as i32 - 128) as u8;
        }
    }

    pub fn derive(buffer: &mut [u8]){
//        unsigned char *t    = (unsigned char *) _tmpBuffer + 1;
//        unsigned char *stop = (unsigned char *) _tmpBuffer + rawSize;
//        int p = t[-1];
//
//        while (t < stop){
//            int d = int (t[0]) - p + (128 + 256);
//            p = t[0];
//            t[0] = d;
//            ++t;
//        }

        // TODO rustify
        let mut p = buffer[0] as i32; // TODO handle empty buffers
        for index in 1..buffer.len() {
            let d = (buffer[index] as i32 - p as i32 + 128 + 256) % 256;
            p = buffer[index] as i32;
            buffer[index] = d as u8;
        }
    }


    pub fn decompress(target: UncompressedData, data: &CompressedData, uncompressed_size: Option<usize>) -> Result<UncompressedData> {
        let mut decoder = Decoder::new(data.as_slice())
            .expect("io error when reading from in-memory vec");

        let mut decompressed = Vec::with_capacity(uncompressed_size.unwrap_or(32));
        decoder.read_to_end(&mut decompressed)?;

        integrate(&mut decompressed);

        /*match &mut target {
            DataBlock::ScanLine(ref mut scan_line_channels) => {
                for ref mut channel in scan_line_channels.iter_mut() {
                    match channel {
                        Array::U32(ref mut channel) => {

                            unimplemented!("sum updifferences");
                        },
                        Array::F16(ref mut channel) => {
                            unimplemented!("sum updifferences");
                        },
                        Array::F32(ref mut channel) => {
                            unimplemented!("sum updifferences");
                        },
                    }
                }
            },

            DataBlock::Tile(ref mut tile_channels) => {
                for ref mut channel in tile_channels.iter_mut() {
                    match channel {
                        Array::U32(ref mut channel) => {
                            unimplemented!("sum updifferences");
                        },
                        Array::F16(ref mut channel) => {
                            unimplemented!("sum updifferences");
                        },
                        Array::F32(ref mut channel) => {
                            unimplemented!("sum updifferences");
                        },
                    }
                }
            },

            _ => unimplemented!()
        }*/

        super::uncompressed::unpack(target, &decompressed)
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
