pub mod zip;
pub mod rle;
pub mod piz;

use super::uncompressed::*;
use crate::file::meta::Header;

#[derive(Debug)]
pub enum Error {
    /// includes zip decompression errors, and wrong-length decoding errors
    Read(crate::file::io::ReadError),
    InvalidData,
}

impl From<::std::io::Error> for Error {
    fn from(io: ::std::io::Error) -> Self {
        Error::from(crate::file::io::ReadError::IoError(io))
    }
}
impl From<crate::file::io::ReadError> for Error {
    fn from(io: crate::file::io::ReadError) -> Self {
        Error::Read(io)
    }
}

pub type Result<T> = ::std::result::Result<T, Error>;
pub type CompressedBytes = Vec<u8>;
pub type UncompressedChannels = PerChannel<Array>;





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

    /// lossy DCT based compression, in blocks
    /// of 32 scanlines. More efficient for partial
    /// buffer access.Like B44, except for blocks of four by four pixels where all pixels have the same
    /// value, which are packed into 3 instead of 14 bytes. For images with large uniform
    /// areas, B44A produces smaller files than B44 compression.
    /// B44A compression is only supported for flat images.
    DWAA,

    /// lossy DCT based compression, in blocks
    /// of 256 scanlines. More efficient space
    /// wise and faster to decode full frames
    /// than DWAA_COMPRESSION.
    DWAB,
}



impl Compression {
    pub fn compress(self, data: &UncompressedChannels) -> Result<CompressedBytes> {
        use self::Compression::*;
        match self {
            None => uncompressed::pack(data),
            ZIP16 => zip::compress_bytes(data),
            ZIP1 => zip::compress_bytes(data),
            RLE => rle::compress_bytes(data),
            compr => unimplemented!("compressing {:?}", compr),
        }
    }

    pub fn decompress(self, header: &Header, data: &CompressedBytes, dimensions: (usize, usize)) -> Result<UncompressedChannels> {
        use self::Compression::*;
        match self {
            None => uncompressed::unpack(header, data, dimensions),
            ZIP16 => zip::decompress_bytes(header, data, dimensions),
            ZIP1 => zip::decompress_bytes(header, data, dimensions),
            RLE => rle::decompress_bytes(header, data, dimensions),
            PIZ => piz::decompress_bytes(header, data, dimensions),
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


// TODO FIXME avoid all intermediate buffers and use iterators/readers exclusively?
pub mod uncompressed {
    use super::*;
    use crate::file::meta::Header;
    use crate::file::meta::attributes::{ParsedText, PixelType};
    use crate::file::io::read_u32_array;

    pub fn unpack(header: &Header, data: &CompressedBytes, dimensions: (usize, usize)) -> Result<UncompressedChannels> {

        // TODO do not rely on kind because it is only required for (multiplart or non-image) data
        match header.kind {
            None | Some(ParsedText::ScanLine) | Some(ParsedText::Tile) => {
                let mut result_channels: PerChannel<Array> = header.channels.iter()
                    .map(|channel|{
                        let size = channel.subsampled_pixels(dimensions.0, dimensions.1);

                        match channel.pixel_type {
                            PixelType::U32 => Array::U32(Vec::with_capacity(size)),
                            PixelType::F16 => Array::F16(Vec::with_capacity(size)),
                            PixelType::F32 => Array::F32(Vec::with_capacity(size)),
                        }
                    })
                    .collect();

                // TODO asserts channels are in alphabetical order?
                let mut remaining_bytes = data.as_slice();

                // for each line, extract all channels
//                for _ in 0..dimensions.1  TODO
                while !remaining_bytes.is_empty() {
                    // for each channel, read all pixels in this single line
                    for channel in &mut result_channels {
                        match channel {
                            Array::U32(ref mut channel) => crate::file::io::read_into_u32_vec(
                                &mut remaining_bytes, channel, dimensions.0, 1024*1024
                            ),

                            Array::F16(ref mut channel) => crate::file::io::read_into_f16_vec(
                                &mut remaining_bytes, channel, dimensions.0, 1024*1024
                            ),

                            Array::F32(ref mut channel) => crate::file::io::read_into_f32_vec(
                                &mut remaining_bytes, channel, dimensions.0, 1024*1024
                            ),
                        };
                    }
                }

                Ok(result_channels)
            },

            _ => {
                unimplemented!()
            }
        }
    }

    pub fn pack(_data: &UncompressedChannels) -> Result<CompressedBytes> {
        unimplemented!()
    }
}



pub mod optimize_bytes {
//    pub trait Bytes: Iterator<Item = u8> {}


    // TODO: use readers and measure improvement

    /*use ::std::io::Read;
    use ::std::io::Result;

    pub struct DifferencesToSamples<D: Read> {
        pub differences: D,
    }

    impl<I: Read> Read for DifferencesToSamples<I> {
        fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
            let result = self.differences.read(buffer);
            unimplemented!("only works if everything is read at once, because first byte is used as starting point");
            if let Ok(len) = result {
                for index in 1..len {
                    buffer[index] = (buffer[index-1] as i32 + buffer[index] as i32 - 128) as u8;
                }
            }

            result
        }
    }*/

    /*
    AS ITERATOR:
    self.differences.next().map(|next_difference|{
        if let Some(prev_sample) = self.previous_sample {
            let next_sample = (prev_sample as i32 + next_difference as i32 - 128) as u8;
            self.previous_sample = Some(next_sample);
            next_sample

        } else { // initial sample is the difference itself
            self.previous_sample = Some(next_difference);
            next_difference
        }
    })
    */

    /*pub fn differences_to_samples<R: Read>(differences: R) -> DifferencesToSamples<R> {
        DifferencesToSamples { differences, previous_sample: None }
    }



    pub struct SamplesToDifferences<I: Bytes> {
        samples: I,
        previous_sample: Option<u8>,
    }

    impl<I: Bytes> Iterator for SamplesToDifferences<I> {
        type Item = u8;

        fn next(&mut self) -> Option<u8> {
            self.samples.next().map(|next_sample|{
                if let Some(prev_sample) = self.previous_sample {
                    let next_sample = (prev_sample as i32 + next_difference as i32 - 128) as u8;
                    self.previous_sample = Some(next_sample);
                    next_sample

                } else { // initial difference is the sample itself
                    self.previous_sample = Some(next_sample);
                    next_sample
                }
            })
        }
    }

    pub fn samples_to_differences_iter<I: Bytes>(samples: I) -> SamplesToDifferences<I> {
        SamplesToDifferences { samples, previous_sample: None }
    }*/


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

    // TODO: use readers and measure improvement
    /*pub struct InterleaveSeparated<I: Bytes> {
        first: I,
        second: I,
        use_first: bool,
    }

    impl<I: Bytes> Iterator for InterleaveSeparated<I> {
        type Item = u8;

        fn next(&mut self) -> Option<u8> {
            self.use_first = !self.use_first;
            if self.use_first { &self.first } else { &self.second }.next()
        }
    }

    pub fn interleave_separated<I: Bytes>(first: I, second: I) -> InterleaveSeparated<I> {
        InterleaveSeparated {
            first, second, use_first: false, // will be flipped before every `.next()` call
        }
    }



    pub struct SeparateInterleavedSlice<'s> {
        interleaved: &'s [u8],
        first_iteration: bool,
        index: usize,
    }

    impl<'s> Iterator for SeparateInterleavedSlice<'s> {
        type Item = u8;

        fn next(&mut self) -> Option<u8> {
            if self.first_iteration {
                if self.index >= self.interleaved.len() - 1 {
                    self.first_iteration = false;
                    self.index = 1;
                }
            };

            let value = self.interleaved.get(self.index);
            self.index += 2;
            value
        }
    }

    pub fn separate_interleaved_slice<'s>(interleaved: &'s [u8]) -> SeparateInterleavedSlice<'s> {
        SeparateInterleavedSlice { interleaved, first_iteration: true, index: 0, }
    }*/

    // TODO make iterator
    /// "interleave"
    pub fn interleave_byte_blocks(separated: &[u8]) -> Vec<u8> {
        // TODO rustify
        // TODO without extra allocation!
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
}
