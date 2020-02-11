
///! Contains the compression attribute definition
///! and methods to compress and decompress data.

mod zip;
mod rle;
mod piz;


use crate::meta::Header;
use crate::meta::attributes::Box2I32;
use crate::error::{Result, Error};



pub type ByteVec = Vec<u8>;
pub type Bytes<'s> = &'s [u8];

/// Specifies which compression method to use.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Compression {

    /// Store uncompressed values. Produces large files that can be read and written quickly.
    /// This compression method is lossless.
    Uncompressed,

    /// Produces slightly smaller files
    /// that can still be read and written rather quickly.
    /// The compressed file size is usually between 60 and 75 percent of the uncompressed size.
    /// Works best for images with large flat areas, such as masks and abstract graphics.
    /// This compression method is lossless.
    RLE,

    /// Uses ZIP compression to compress each line. Slowly produces small images
    /// which can be read with moderate speed. This compression method is lossless.
    ZIP1,

    /// Uses ZIP compression to compress blocks of 16 lines. Slowly produces small images
    /// which can be read with moderate speed. This compression method is lossless.
    ZIP16,

    /// __PIZ compression is not yet supported by this implementation.__
    ///
    /// PIZ compression works well for noisy and natural images. Works better with larger tiles.
    /// Only supported for flat images, but not for deep data.
    /// This compression method is lossless.
    // A wavelet transform is applied to the pixel data, and the result is Huffman-
    // encoded. This scheme tends to provide the best compression ratio for the types of
    // images that are typically processed at Industrial Light & Magic. Files are
    // compressed and decompressed at roughly the same speed. For photographic
    // images with film grain, the files are reduced to between 35 and 55 percent of their
    // uncompressed size.
    // PIZ compression works well for scan-line based files, and also for tiled files with
    // large tiles, but small tiles do not shrink much. (PIZ-compressed data start with a
    // relatively long header; if the input to the compressor is short, adding the header
    // tends to offset any size reduction of the input.)
    PIZ,

    /// __This lossy compression is not yet supported by this implementation.__
    // After reducing 32-bit floating-point data to 24 bits by rounding (while leaving 16-bit
    // floating-point data unchanged), differences between horizontally adjacent pixels
    // are compressed with zlib, similar to ZIP. PXR24 compression preserves image
    // channels of type HALF and UINT exactly, but the relative error of FLOAT data
    // increases to about
    // . This compression method works well for depth
    // buffers and similar images, where the possible range of values is very large, but
    // where full 32-bit floating-point accuracy is not necessary. Rounding improves
    // compression significantly by eliminating the pixels' 8 least significant bits, which
    // tend to be very noisy, and therefore difficult to compress.
    // PXR24 compression is only supported for flat images.
    PXR24,

    /// __This lossy compression is not yet supported by this implementation.__
    // lossy 4-by-4 pixel block compression,
    // fixed compression rate
    B44,

    /// __This lossy compression is not yet supported by this implementation.__
    // lossy 4-by-4 pixel block compression,
    // flat fields are compressed more
    // Channels of type HALF are split into blocks of four by four pixels or 32 bytes. Each
    // block is then packed into 14 bytes, reducing the data to 44 percent of their
    // uncompressed size. When B44 compression is applied to RGB images in
    // combination with luminance/chroma encoding (see below), the size of the
    // compressed pixels is about 22 percent of the size of the original RGB data.
    // Channels of type UINT or FLOAT are not compressed.
    // Decoding is fast enough to allow real-time playback of B44-compressed OpenEXR
    // image sequences on commodity hardware.
    // The size of a B44-compressed file depends on the number of pixels in the image,
    // but not on the data in the pixels. All images with the same resolution and the same
    // set of channels have the same size. This can be advantageous for systems that
    // support real-time playback of image sequences; the predictable file size makes it
    // easier to allocate space on storage media efficiently.
    // B44 compression is only supported for flat images.
    B44A,

    /// __This lossy compression is not yet supported by this implementation.__
    // lossy DCT based compression, in blocks
    // of 32 scanlines. More efficient for partial
    // buffer access.Like B44, except for blocks of four by four pixels where all pixels have the same
    // value, which are packed into 3 instead of 14 bytes. For images with large uniform
    // areas, B44A produces smaller files than B44 compression.
    // B44A compression is only supported for flat images.
    DWAA,

    /// __This lossy compression is not yet supported by this implementation.__
    // lossy DCT based compression, in blocks
    // of 256 scanlines. More efficient space
    // wise and faster to decode full frames
    // than DWAA_COMPRESSION.
    DWAB,
}

impl std::fmt::Display for Compression {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} compression", match self {
            Compression::Uncompressed => "no",
            Compression::RLE => "rle",
            Compression::ZIP1 => "zip line",
            Compression::ZIP16 => "zip block",
            Compression::B44 => "b44",
            Compression::B44A => "b44a",
            Compression::DWAA=> "dwaa",
            Compression::DWAB => "dwab",
            Compression::PIZ => "piz",
            Compression::PXR24 => "pxr24",
        })
    }
}



impl Compression {

    pub fn compress_image_section(self, packed: ByteVec) -> Result<ByteVec> {
        use self::Compression::*;

        let compressed = match self {
            Uncompressed => return Ok(packed),
            ZIP16 => zip::compress_bytes(&packed),
            ZIP1 => zip::compress_bytes(&packed),
            RLE => rle::compress_bytes(&packed),
//            PIZ => piz::compress_bytes(packed)?,
            _ => return Err(Error::unsupported(format!("yet unimplemented compression method: {}", self)))
        };

        let compressed = compressed
            .map_err(|_| Error::invalid("compressed content"))?;

        if compressed.len() < packed.len() {
            Ok(compressed)
        }
        else {
            Ok(packed)
        }
    }

    /// Panics for invalid tile coordinates.
    pub fn decompress_image_section(self, header: &Header, data: ByteVec, tile: Box2I32) -> Result<ByteVec> {
        let dimensions = tile.size;
        debug_assert!(tile.validate(dimensions).is_ok());

        let expected_byte_size = (dimensions.0 * dimensions.1 * header.channels.bytes_per_pixel) as usize;

        if data.len() == expected_byte_size {
            Ok(data) // the raw data was smaller than the compressed data, so the raw data has been written
        }

        else {
            use self::Compression::*;
            let bytes = match self {
                Uncompressed => Ok(data),
                ZIP16 => zip::decompress_bytes(&data, expected_byte_size),
                ZIP1 => zip::decompress_bytes(&data, expected_byte_size),
                RLE => rle::decompress_bytes(&data, expected_byte_size),
//                PIZ => piz::decompress_bytes(header, data, tile, expected_byte_size),
                _ => return Err(Error::unsupported(format!("yet unimplemented compression method: {}", self)))
            };

            // map all errors to compression errors
            let bytes = bytes
                .map_err(|_| Error::invalid(format!("compressed data ({:?})", self)))?;

//            debug_assert_eq!(
//                bytes.len(), expected_byte_size,
//                "compression size mismatch: expected {}, found {}", expected_byte_size, bytes.len()
//            );

            if bytes.len() != expected_byte_size {
                Err(Error::invalid("decompressed data"))
            }

            else {
                Ok(bytes)
            }
        }
    }

    // used for deep data
    /*pub fn decompress_bytes(self, data: ByteVec, expected_byte_size: usize) -> Result<ByteVec> {
        if data.len() == expected_byte_size {
            Ok(data)
        }

        else {
            use self::Compression::*;
            let result = match self {
                Uncompressed => Ok(data),
                ZIP16 => zip::decompress_bytes(&data, expected_byte_size),
                ZIP1 => zip::decompress_bytes(&data, expected_byte_size),
                RLE => rle::decompress_bytes(&data, expected_byte_size),
                _ => return Err(Error::unsupported(format!("deep data compression method: {}", self)))
            };

            // map all errors to compression errors
            result.map_err(|_| Error::invalid("compressed content"))
        }
    }*/

    /// For scan line images and deep scan line images, one or more scan lines may be
    /// stored together as a scan line block. The number of scan lines per block
    /// depends on how the pixel data are compressed.
    pub fn scan_lines_per_block(self) -> u32 {
        use self::Compression::*;
        match self {
            Uncompressed | RLE   | ZIP1  => 1,
            ZIP16 | PXR24                => 16,
            PIZ   | B44   | B44A | DWAA  => 32,
            DWAB                         => 256,
        }
    }

    pub fn supports_deep_data(self) -> bool {
        use self::Compression::*;
        match self {
            Uncompressed | RLE | ZIP1 | ZIP16 => true,
            _ => false,
        }
    }
}


/// A collection of functions used to prepare data for compression.
mod optimize_bytes {

    /// Integrate over all differences to the previous value in order to reconstruct sample values.
    pub fn differences_to_samples(buffer: &mut [u8]){
        for index in 1..buffer.len() {
            buffer[index] = (buffer[index - 1] as i32 + buffer[index] as i32 - 128) as u8;
        }
    }

    /// Derive over all values in order to produce differences to the previous value.
    pub fn samples_to_differences(buffer: &mut [u8]){
        for index in (1..buffer.len()).rev() {
            buffer[index] = (buffer[index] as i32 - buffer[index - 1] as i32 + 128) as u8;
        }
    }

    /// Interleave the bytes such that the second halv of the array is each other byte.
    pub fn interleave_byte_blocks(separated: &mut [u8]) {
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

        separated.copy_from_slice(interleaved.as_slice())
    }

    /// Separate the bytes such that the second half contains each other byte.
    pub fn separate_bytes_fragments(source: &mut [u8]) {
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
        source.copy_from_slice(result.as_slice());
    }


    #[cfg(test)]
    pub mod test {

        #[test]
        fn roundtrip_interleave(){
            let source = vec![ 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10 ];
            let mut modified = source.clone();

            super::separate_bytes_fragments(&mut modified);
            super::interleave_byte_blocks(&mut modified);

            assert_eq!(source, modified);
        }

        #[test]
        fn roundtrip_derive(){
            let source = vec![ 0, 1, 2, 7, 4, 5, 6, 7, 13, 9, 10 ];
            let mut modified = source.clone();

            super::samples_to_differences(&mut modified);
            println!("differences {:?}", modified);

            super::differences_to_samples(&mut modified);

            assert_eq!(source, modified);
        }
    }
}
