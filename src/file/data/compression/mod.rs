pub mod zip;
pub mod rle;

use super::uncompressed::*;

#[derive(Debug)]
pub enum Error {
    /// includes zip compression errors
    IO(::std::io::Error),

    RLEMaxLengthExceeded,
}

impl From<::std::io::Error> for Error {
    fn from(io: ::std::io::Error) -> Self {
        Error::IO(io)
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
            ZIP16 => zip::compress_bytes(data),
            ZIP1 => zip::compress_bytes(data),
            RLE => rle::compress_bytes(data),
            compr => unimplemented!("compressing {:?}", compr),
        }
    }

    pub fn decompress(
        self,
        target: UncompressedData,
        data: &CompressedData,
        line_size: usize,
    )
        -> Result<UncompressedData>
    {
        use self::Compression::*;
        match self {
            None => uncompressed::unpack(target, data, line_size),
            ZIP16 => zip::decompress_bytes(target, data, line_size),
            ZIP1 => zip::decompress_bytes(target, data, line_size),
            RLE => rle::decompress_bytes(target, data, line_size),
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

    pub fn unpack(mut target: UncompressedData, data: &CompressedData, line_size: usize) -> Result<UncompressedData> {
        match &mut target {
            DataBlock::ScanLine(ref mut scan_line_channels) => {
                // TODO assert channels are in alphabetical order?
                let mut remaining_bytes = data.as_slice();

                // for each line, extract all channels
                while !remaining_bytes.is_empty() {

                    // for each channel, read all pixels in this single line
                    for ref mut channel in scan_line_channels.iter_mut() {
                        match channel {
                            Array::U32(ref mut channel) => {
                                // TODO without separate allocation: channel must have zeroes and from io read into subslice

                                let line = ::file::io::read_u32_vec(
                                    &mut remaining_bytes, line_size, ::std::u16::MAX as usize
                                ).expect("io err when reading from in-memory vec");;

                                channel.extend_from_slice(&line);
                            },

                            Array::F16(ref mut channel) => {
                                // TODO don't allocate
                                let line = ::file::io::read_f16_vec(
                                    &mut remaining_bytes, line_size, ::std::u16::MAX as usize
                                ).expect("io err when reading from in-memory vec");
//
                                channel.extend_from_slice(&line);
                            },

                            Array::F32(ref mut channel) => {
                                // TODO without separate allocation
                                let line = ::file::io::read_f32_vec(
                                    &mut remaining_bytes, line_size, ::std::u16::MAX as usize
                                ).expect("io err when reading from in-memory vec");;

                                channel.extend_from_slice(&line);
                            },
                        }
                    }
                }
            },

            DataBlock::Tile(ref mut tile_channels) => {
                // TODO assert channels are in alphabetical order?
                let mut remaining_bytes = data.as_slice();

                // for each line, extract all channels
                while !remaining_bytes.is_empty() {

                    // for each channel, read all pixels in this single line
                    for ref mut channel in tile_channels.iter_mut() {
                        match channel {
                            Array::U32(ref mut channel) => {
                                // TODO without separate allocation
                                let line = ::file::io::read_u32_vec(
                                    &mut remaining_bytes, line_size, ::std::u16::MAX as usize
                                ).expect("io err when reading from in-memory vec");

                                channel.extend_from_slice(&line);
                            },

                            Array::F16(ref mut channel) => {
                                // TODO don't allocate
                                let line = ::file::io::read_f16_vec(
                                    &mut remaining_bytes, line_size, ::std::u16::MAX as usize
                                ).expect("io err when reading from in-memory vec");
//
                                channel.extend_from_slice(&line);
                            },

                            Array::F32(ref mut channel) => {
                                // TODO without separate allocation
                                let line = ::file::io::read_f32_vec(
                                    &mut remaining_bytes, line_size, ::std::u16::MAX as usize
                                ).expect("io err when reading from in-memory vec");

                                channel.extend_from_slice(&line);
                            },
                        }
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



pub mod optimize_bytes {

    // inspired by https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfZip.cpp

    /// "Predictor."
    pub fn differences_to_samples(buffer: &mut [u8]){
        for index in 1..buffer.len() {
            buffer[index] = (buffer[index-1] as i32 + buffer[index] as i32 - 128) as u8;
        }
    }

    /// "Predictor."
    pub fn samples_to_differences(buffer: &mut [u8]){
        for index in 1..buffer.len() {
            buffer[index] = (buffer[index] as i32 - buffer[index-1] as i32 + 128 + 256) as u8;
        }
    }

    /// de-"interleave"
    pub fn separate_bytes_fragments(source: &[u8]) -> Vec<u8> {
        // TODO without extra allocation?
        let mut first_half = Vec::with_capacity(source.len() / 2);
        let mut second_half = Vec::with_capacity(source.len() / 2);
        let mut interleaved_index = 0;

        // TODO rustify!
        loop {
            if interleaved_index < source.len() {
                first_half.push(source[interleaved_index]);
                interleaved_index += 1;

            } else { break; }

            if interleaved_index < source.len() {
                second_half.push(source[interleaved_index]);
                interleaved_index += 1;

            } else { break; }
        }

        let mut result = first_half;
        result.append(&mut second_half);
        result
    }

    /// "interleave"
    pub fn interleave_byte_blocks(separated: &[u8]) -> Vec<u8> {
        // TODO rustify
        let mut interleaved = Vec::with_capacity(separated.len());
        let (first_half, second_half) = separated
            .split_at((separated.len() + 1) / 2);

        let mut second_half_index = 0;
        let mut first_half_index = 0;

        loop {
            if interleaved.len() < separated.len() {
                interleaved.push(first_half[first_half_index]);
                first_half_index += 1;
            } else { break; }

            if interleaved.len() < separated.len() {
                interleaved.push(second_half[second_half_index]);
                second_half_index += 1;
            } else { break; }
        }

        interleaved
    }
}
