
//! Contains the compression attribute definition
//! and methods to compress and decompress data.


// private modules make non-breaking changes easier
mod zip;
mod rle;
mod piz;
mod pxr24;



use crate::meta::attribute::{IntegerBounds, SampleType, ChannelList};
use crate::error::{Result, Error, usize_to_i32};
use crate::meta::header::Header;


/// A byte vector.
pub type ByteVec = Vec<u8>;

/// A byte slice.
pub type Bytes<'s> = &'s [u8];

/// Specifies which compression method to use.
/// Use uncompressed data for fastest loading and writing speeds.
/// Use RLE compression for fast loading and writing with slight memory savings.
/// Use ZIP compression for slow processing with large memory savings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Compression {

    /// Store uncompressed values.
    /// Produces large files that can be read and written very quickly.
    /// Consider using RLE instead, as it provides some compression with almost equivalent speed.
    Uncompressed,

    /// Produces slightly smaller files
    /// that can still be read and written rather quickly.
    /// The compressed file size is usually between 60 and 75 percent of the uncompressed size.
    /// Works best for images with large flat areas, such as masks and abstract graphics.
    /// This compression method is lossless.
    RLE,

    /// Uses ZIP compression to compress each line. Slowly produces small images
    /// which can be read with moderate speed. This compression method is lossless.
    /// Might be slightly faster but larger than `ZIP16´.
    ZIP1, // TODO specify zip compression level?

    /// Uses ZIP compression to compress blocks of 16 lines. Slowly produces small images
    /// which can be read with moderate speed. This compression method is lossless.
    /// Might be slightly slower but smaller than `ZIP1´.
    ZIP16, // TODO specify zip compression level?

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

    /// Like `ZIP1`, but reduces precision of `f32` images to `f24`.
    /// Therefore, this is lossless compression for `f16` and `u32` data, lossy compression for `f32` data.
    /// This produces really small image files. Only supported for flat images, not for deep data.
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
    PXR24, // TODO specify zip compression level?

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
    DWAA(Option<f32>), // TODO does this have a default value? make this non optional?

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
            Compression::DWAA(_) => "dwaa",
            Compression::DWAB => "dwab",
            Compression::PIZ => "piz",
            Compression::PXR24 => "pxr24",
        })
    }
}



impl Compression {

    // FIXME this conversion should be done inside each compression algorithm! not inside the general compression.
    fn native_format(self, header: &Header) -> bool {
        let has_only_f16_channels = header.channels.uniform_sample_type == Some(SampleType::F16);

        match self {
            Compression::Uncompressed => true,
            Compression::RLE => true, // false, // FIXME false in original library???
            Compression::ZIP1 => false,
            Compression::ZIP16 => false,
            Compression::PIZ => has_only_f16_channels, // TODO DRY and compute only once??
            Compression::PXR24 => false, //FIXME true in original library  // true, // what??? i thought this is zip?!?!?!
            Compression::B44 | Compression::B44A => has_only_f16_channels,
            Compression::DWAA(_) | Compression::DWAB => {
                cfg!(target_endian = "little") // native if little endian?!
                // FIXME so... this should always return true, as files are also always stored in little endian???
            },
        }
    }

    /// Compress the image section of bytes.
    pub fn compress_image_section(self, header: &Header, mut uncompressed: ByteVec, pixel_section: IntegerBounds) -> Result<ByteVec> {
        let max_tile_size = header.max_block_pixel_size();

        assert!(pixel_section.validate(Some(max_tile_size)).is_ok(), "decompress tile coordinate bug");
        if header.deep { assert!(self.supports_deep_data()) }

        // convert data if compression method expects native format
        // see https://github.com/AcademySoftwareFoundation/openexr/blob/3bd93f85bcb74c77255f28cdbb913fdbfbb39dfe/OpenEXR/IlmImf/ImfTiledOutputFile.cpp#L750-L842
        if !self.native_format(header) {
            uncompressed = convert_current_to_little_endian(uncompressed, &header.channels, pixel_section);
        }

        use self::Compression::*;
        let compressed = match self {
            Uncompressed => Ok(uncompressed.clone()), // TODO no clone!
            ZIP16 => zip::compress_bytes(&uncompressed),
            ZIP1 => zip::compress_bytes(&uncompressed),
            RLE => rle::compress_bytes(&uncompressed),
            PIZ => piz::compress(&header.channels, &uncompressed, pixel_section),
            PXR24 => pxr24::compress(&header.channels, &uncompressed, pixel_section),
            _ => return Err(Error::unsupported(format!("yet unimplemented compression method: {}", self)))
        };

        let compressed = compressed.map_err(|_|
            Error::invalid(format!("pixels cannot be compressed ({})", self))
        )?;

        if compressed.len() < uncompressed.len() {
            // only write compressed if it actually is smaller than raw
            Ok(compressed)
        }
        else {
            // manually convert uncompressed data
            Ok(convert_current_to_little_endian(uncompressed, &header.channels, pixel_section))
        }
    }

    /// Decompress the image section of bytes.
    pub fn decompress_image_section(self, header: &Header, compressed: ByteVec, pixel_section: IntegerBounds, pedantic: bool) -> Result<ByteVec> {
        let max_tile_size = header.max_block_pixel_size();

        assert!(pixel_section.validate(Some(max_tile_size)).is_ok(), "decompress tile coordinate bug");
        if header.deep { assert!(self.supports_deep_data()) }

        let expected_byte_size = pixel_section.size.area() * header.channels.bytes_per_pixel; // FIXME this needs to account for subsampling anywhere

        if compressed.len() == expected_byte_size {
            Ok(convert_little_endian_to_current(compressed, &header.channels, pixel_section)) // the compressed data was larger than the raw data, so the raw data has been written
        }
        else {
            use self::Compression::*;
            let bytes = match self {
                Uncompressed => Ok(compressed),
                ZIP16 => zip::decompress_bytes(&compressed),
                ZIP1 => zip::decompress_bytes(&compressed),
                RLE => rle::decompress_bytes(&compressed, expected_byte_size, pedantic),
                PIZ => piz::decompress(&header.channels, compressed, pixel_section, expected_byte_size, pedantic),
                PXR24 => pxr24::decompress(&header.channels, &compressed, pixel_section, expected_byte_size, pedantic),
                _ => return Err(Error::unsupported(format!("yet unimplemented compression method: {}", self)))
            };

            // map all errors to compression errors
            let bytes = bytes
                .map_err(|_| Error::invalid(format!("compressed data ({:?})", self)))?;

            if bytes.len() != expected_byte_size {
                Err(Error::invalid("decompressed data"))
            }

            else {
                // convert data if compression method has output native format
                if !self.native_format(header) {
                    Ok(convert_little_endian_to_current(bytes, &header.channels, pixel_section))
                }

                else { Ok(bytes) }
            }
        }
    }

    /// For scan line images and deep scan line images, one or more scan lines may be
    /// stored together as a scan line block. The number of scan lines per block
    /// depends on how the pixel data are compressed.
    pub fn scan_lines_per_block(self) -> usize {
        use self::Compression::*;
        match self {
            Uncompressed | RLE   | ZIP1    => 1,
            ZIP16 | PXR24                  => 16,
            PIZ   | B44   | B44A | DWAA(_) => 32,
            DWAB                           => 256,
        }
    }

    /// Deep data can only be compressed using RLE or ZIP compression.
    pub fn supports_deep_data(self) -> bool {
        use self::Compression::*;
        match self {
            Uncompressed | RLE | ZIP1 => true,
            _ => false,
        }
    }

    /// Most compression methods will reconstruct the exact pixel bytes,
    /// but some might throw away unimportant data for specific types of samples.
    pub fn is_lossless_for(self, sample_type: SampleType) -> bool {
        use self::Compression::*;
        match self {
            PXR24 => sample_type != SampleType::F32, // pxr reduces f32 to f24
            B44 | B44A => sample_type != SampleType::F16, // b44 only compresses f16 values, others are left uncompressed
            Uncompressed | RLE | ZIP1 | ZIP16 | PIZ => true,
            DWAB | DWAA(_) => false,
        }
    }

    /// Most compression methods will reconstruct the exact pixel bytes,
    /// but some might throw away unimportant data in some cases.
    pub fn may_loose_data(self) -> bool {
        use self::Compression::*;
        match self {
            Uncompressed | RLE | ZIP1 | ZIP16 | PIZ => false,
            PXR24 | B44 | B44A | DWAB | DWAA(_) => true,
        }
    }

    /// Most compression methods will reconstruct the exact pixel bytes,
    /// but some might replace NaN with zeroes.
    pub fn supports_nan(self) -> bool {
        use self::Compression::*;
        match self {
            B44 | B44A | DWAB | DWAA(_) => false, // TODO dwa might support it?
            _ => true
        }
    }

}

// see https://github.com/AcademySoftwareFoundation/openexr/blob/6a9f8af6e89547bcd370ae3cec2b12849eee0b54/OpenEXR/IlmImf/ImfMisc.cpp#L1456-L1541
// FIXME this should really be done inside each compression method

#[allow(unused)]
fn convert_current_to_little_endian(bytes: ByteVec, channels: &ChannelList, rectangle: IntegerBounds) -> ByteVec { // TODO is this really not already somewhere else?
    #[cfg(target = "big_endian")] {
        use lebe::prelude::*;

        // FIXME do this in-place
        let mut little = Vec::with_capacity(bytes.len());
        let mut native = bytes.as_slice();

        for y in rectangle.position.y() .. rectangle.end().y() {
            for channel in &channels.list {
                if mod_p(y, usize_to_i32(channel.sampling.y())) != 0 { continue; }

                // FIXME do not match on every value
                for _x in 0 .. rectangle.size.width() / channel.sampling.x() {
                    match channel.sample_type {
                        SampleType::F16 => little.write_as_little_endian(&u16::read_from_native_endian(&mut native).expect("read from in-memory buffer failed")),
                        SampleType::F32 => little.write_as_little_endian(&f32::read_from_native_endian(&mut native).expect("read from in-memory buffer failed")),
                        SampleType::U32 => little.write_as_little_endian(&u32::read_from_native_endian(&mut native).expect("read from in-memory buffer failed")),
                    }.expect("write to in-memory buffer failed");
                }
            }
        }

        return little;
    }

    bytes
}

#[allow(unused)]
fn convert_little_endian_to_current(bytes: ByteVec, channels: &ChannelList, rectangle: IntegerBounds) -> ByteVec { // TODO is this really not already somewhere else?
    #[cfg(target = "big_endian")] {
        use lebe::prelude::*;

        // FIXME do this in-place
        let mut native = Vec::with_capacity(bytes.len());
        let mut little = bytes.as_slice();

        for y in rectangle.position.y() .. rectangle.end().y() {
            for channel in &channels.list {
                if mod_p(y, usize_to_i32(channel.sampling.y())) != 0 { continue; }

                // FIXME do not match on every value
                for _x in 0 .. rectangle.size.width() / channel.sampling.x() {
                    match channel.sample_type {
                        SampleType::F16 => native.write_as_native_endian(&u16::read_from_little_endian(&mut little).expect("read from in-memory buffer failed")),
                        SampleType::F32 => native.write_as_native_endian(&f32::read_from_little_endian(&mut little).expect("read from in-memory buffer failed")),
                        SampleType::U32 => native.write_as_native_endian(&u32::read_from_little_endian(&mut little).expect("read from in-memory buffer failed")),
                    }.expect("write to in-memory buffer failed");
                }
            }
        }

        return native;
    }

    bytes
}


fn div_p (x: i32, y: i32) -> i32 {
    if x >= 0 {
        if y >= 0 { x  / y }
        else { -(x  / -y) }
    }
    else {
        if y >= 0 { -((y-1-x) / y) }
        else { (-y-1-x) / -y }
    }
}

fn mod_p(x: i32, y: i32) -> i32 {
    x - y * div_p(x, y)
}

/// A collection of functions used to prepare data for compression.
mod optimize_bytes {

    /// Integrate over all differences to the previous value in order to reconstruct sample values.
    pub fn differences_to_samples(buffer: &mut [u8]){
        for index in 1..buffer.len() {
            buffer[index] = (buffer[index - 1] as i32 + buffer[index] as i32 - 128) as u8; // index unsafe but handled with care and unit-tested
        }
    }

    /// Derive over all values in order to produce differences to the previous value.
    pub fn samples_to_differences(buffer: &mut [u8]){
        for index in (1..buffer.len()).rev() {
            buffer[index] = (buffer[index] as i32 - buffer[index - 1] as i32 + 128) as u8; // index unsafe but handled with care and unit-tested
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
                interleaved.push(first_half[first_half_index]); // index unsafe but handled with care and unit-tested
                first_half_index += 1;
            } else { break; }

            if interleaved.len() < separated.len() {
                interleaved.push(second_half[second_half_index]); // index unsafe but handled with care and unit-tested
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
                first_half.push(source[interleaved_index]); // index unsafe but handled with care and unit-tested
                interleaved_index += 1;

            } else { break; }

            if interleaved_index < source.len() {
                second_half.push(source[interleaved_index]); // index unsafe but handled with care and unit-tested
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

            super::differences_to_samples(&mut modified);

            assert_eq!(source, modified);
        }
    }
}
