
//! Contains the compression attribute definition
//! and methods to compress and decompress data.


// private modules make non-breaking changes easier
mod zip;
mod rle;
mod piz;
mod pxr24;
mod b44;



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
    ZIP1,  // TODO ZIP { individual_lines: bool, compression_level: Option<u8> }  // TODO specify zip compression level?

    /// Uses ZIP compression to compress blocks of 16 lines. Slowly produces small images
    /// which can be read with moderate speed. This compression method is lossless.
    /// Might be slightly slower but smaller than `ZIP1´.
    ZIP16, // TODO collapse with ZIP1

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
    /// This compression method works well for depth
    /// buffers and similar images, where the possible range of values is very large, but
    /// where full 32-bit floating-point accuracy is not necessary. Rounding improves
    /// compression significantly by eliminating the pixels' 8 least significant bits, which
    /// tend to be very noisy, and therefore difficult to compress.
    /// This produces really small image files. Only supported for flat images, not for deep data.
    // After reducing 32-bit floating-point data to 24 bits by rounding (while leaving 16-bit
    // floating-point data unchanged), differences between horizontally adjacent pixels
    // are compressed with zlib, similar to ZIP. PXR24 compression preserves image
    // channels of type HALF and UINT exactly, but the relative error of FLOAT data
    // increases to about ???.
    PXR24, // TODO specify zip compression level?

    /// This is a lossy compression method for f16 images.
    /// It's the predecessor of the `B44A` compression,
    /// which has improved compression rates for uniformly colored areas.
    /// You should probably use `B44A` instead of the plain `B44`.
    ///
    /// Only supported for flat images, not for deep data.
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
    B44, // TODO B44 { optimize_uniform_areas: bool }

    /// This is a lossy compression method for f16 images.
    /// All f32 and u32 channels will be stored without compression.
    /// All the f16 pixels are divided into 4x4 blocks.
    /// Each block is then compressed as a whole.
    ///
    /// The 32 bytes of a block will require only ~14 bytes after compression,
    /// independent of the actual pixel contents. With chroma subsampling,
    /// a block will be compressed to ~7 bytes.
    /// Uniformly colored blocks will be compressed to ~3 bytes.
    ///
    /// The 512 bytes of an f32 block will not be compressed at all.
    ///
    /// Should be fast enough for realtime playback.
    /// Only supported for flat images, not for deep data.
    B44A, // TODO collapse with B44

    /// __This lossy compression is not yet supported by this implementation.__
    // lossy DCT based compression, in blocks
    // of 32 scanlines. More efficient for partial buffer access.
    DWAA(Option<f32>), // TODO does this have a default value? make this non optional? default Compression Level setting is 45.0

    /// __This lossy compression is not yet supported by this implementation.__
    // lossy DCT based compression, in blocks
    // of 256 scanlines. More efficient space
    // wise and faster to decode full frames
    // than DWAA_COMPRESSION.
    DWAB(Option<f32>), // TODO collapse with B44. default Compression Level setting is 45.0
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
            Compression::DWAB(_) => "dwab",
            Compression::PIZ => "piz",
            Compression::PXR24 => "pxr24",
        })
    }
}



impl Compression {

    /// Compress the image section of bytes.
    pub fn compress_image_section(self, header: &Header, uncompressed_native_endian: ByteVec, pixel_section: IntegerBounds) -> Result<ByteVec> {
        let max_tile_size = header.max_block_pixel_size();

        assert!(pixel_section.validate(Some(max_tile_size)).is_ok(), "decompress tile coordinate bug");
        if header.deep { assert!(self.supports_deep_data()) }

        use self::Compression::*;
        let compressed_little_endian = match self {
            Uncompressed => Ok(convert_current_to_little_endian(&uncompressed_native_endian, &header.channels, pixel_section)),
            ZIP16 => zip::compress_bytes(&header.channels, &uncompressed_native_endian, pixel_section),
            ZIP1 => zip::compress_bytes(&header.channels, &uncompressed_native_endian, pixel_section),
            RLE => rle::compress_bytes(&header.channels, &uncompressed_native_endian, pixel_section),
            PIZ => piz::compress(&header.channels, &uncompressed_native_endian, pixel_section),
            PXR24 => pxr24::compress(&header.channels, &uncompressed_native_endian, pixel_section),
            B44 => b44::compress(&header.channels, &uncompressed_native_endian, pixel_section, false),
            B44A => b44::compress(&header.channels, &uncompressed_native_endian, pixel_section, true),
            _ => return Err(Error::unsupported(format!("yet unimplemented compression method: {}", self)))
        };

        let compressed_little_endian = compressed_little_endian.map_err(|_|
            Error::invalid(format!("pixels cannot be compressed ({})", self))
        )?;

        if self == Uncompressed || compressed_little_endian.len() < uncompressed_native_endian.len() {
            // only write compressed if it actually is smaller than raw, or no compression is used
            Ok(compressed_little_endian)
        }
        else {
            // if we do not use compression, manually convert uncompressed data
            Ok(convert_current_to_little_endian(&uncompressed_native_endian, &header.channels, pixel_section))
        }
    }

    /// Decompress the image section of bytes.
    pub fn decompress_image_section(self, header: &Header, compressed: ByteVec, pixel_section: IntegerBounds, pedantic: bool) -> Result<ByteVec> {
        let max_tile_size = header.max_block_pixel_size();

        assert!(pixel_section.validate(Some(max_tile_size)).is_ok(), "decompress tile coordinate bug");
        if header.deep { assert!(self.supports_deep_data()) }

        let expected_byte_size = pixel_section.size.area() * header.channels.bytes_per_pixel; // FIXME this needs to account for subsampling anywhere

        // note: always true where self == Uncompressed
        if compressed.len() == expected_byte_size {
            // the compressed data was larger than the raw data, so the small raw data has been written
            Ok(convert_little_endian_to_current(&compressed, &header.channels, pixel_section))
        }
        else {
            use self::Compression::*;
            let bytes = match self {
                Uncompressed => Ok(convert_little_endian_to_current(&compressed, &header.channels, pixel_section)),
                ZIP16 => zip::decompress_bytes(&header.channels, compressed, pixel_section, expected_byte_size, pedantic),
                ZIP1 => zip::decompress_bytes(&header.channels, compressed, pixel_section, expected_byte_size, pedantic),
                RLE => rle::decompress_bytes(&header.channels, compressed, pixel_section, expected_byte_size, pedantic),
                PIZ => piz::decompress(&header.channels, compressed, pixel_section, expected_byte_size, pedantic),
                PXR24 => pxr24::decompress(&header.channels, compressed, pixel_section, expected_byte_size, pedantic),
                B44 | B44A => b44::decompress(&header.channels, compressed, pixel_section, expected_byte_size, pedantic),
                _ => return Err(Error::unsupported(format!("yet unimplemented compression method: {}", self)))
            };

            // map all errors to compression errors
            let bytes = bytes
                .map_err(|decompression_error| match decompression_error {
                    Error::NotSupported(message) =>
                        Error::unsupported(format!("yet unimplemented compression special case ({})", message)),

                    error => Error::invalid(format!(
                        "compressed {:?} data ({})",
                        self, error.to_string()
                    )),
                })?;

            if bytes.len() != expected_byte_size {
                Err(Error::invalid("decompressed data"))
            }

            else { Ok(bytes) }
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
            DWAB(_)                        => 256,
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
            DWAB(_) | DWAA(_) => false,
        }
    }

    /// Most compression methods will reconstruct the exact pixel bytes,
    /// but some might throw away unimportant data in some cases.
    pub fn may_loose_data(self) -> bool {
        use self::Compression::*;
        match self {
            Uncompressed | RLE | ZIP1 | ZIP16 | PIZ => false,
            PXR24 | B44 | B44A | DWAB(_) | DWAA(_)  => true,
        }
    }

    /// Most compression methods will reconstruct the exact pixel bytes,
    /// but some might replace NaN with zeroes.
    pub fn supports_nan(self) -> bool {
        use self::Compression::*;
        match self {
            B44 | B44A | DWAB(_) | DWAA(_) => false, // TODO dwa might support it?
            _ => true
        }
    }

}

// see https://github.com/AcademySoftwareFoundation/openexr/blob/6a9f8af6e89547bcd370ae3cec2b12849eee0b54/OpenEXR/IlmImf/ImfMisc.cpp#L1456-L1541
// FIXME this should really be done inside each compression method

#[allow(unused)]
fn convert_current_to_little_endian(bytes: Bytes<'_>, channels: &ChannelList, rectangle: IntegerBounds) -> ByteVec { // TODO is this really not already somewhere else?
    #[cfg(target = "big_endian")] {
        use lebe::prelude::*;

        // FIXME do this in-place
        let mut little = Vec::with_capacity(bytes.len());
        let mut native = bytes;

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

    /*fn convert_big_to_little_endian(
        mut bytes: ByteVec, channels: &ChannelList,
        rectangle: IntegerBounds
    ) -> ByteVec {
        use lebe::prelude::*;

        let remaining_bytes = bytes.as_slice();

        for y in rectangle.position.y() .. rectangle.end().y() {
            for channel in &channels.list {
                if mod_p(y, usize_to_i32(channel.sampling.y())) != 0 { continue; }

                // FIXME do not match on every value

                //for _x in 0 .. rectangle.size.width() / channel.sampling.x() {
                    match channel.sample_type {
                        SampleType::F16 => {
                            let values: &mut [::half::f16] = remaining_bytes[..len].read_from_native_endian_mut()
                                .expect("memory read failed");

                            values.convert_current_to_little_endian();
                        }
                        // SampleType::F16 => little.write_as_little_endian(&u16::read_from_native_endian(&mut native).expect("read from in-memory buffer failed")),
                        // SampleType::F32 => little.write_as_little_endian(&f32::read_from_native_endian(&mut native).expect("read from in-memory buffer failed")),
                        // SampleType::U32 => little.write_as_little_endian(&u32::read_from_native_endian(&mut native).expect("read from in-memory buffer failed")),
                    }.expect("write to in-memory buffer failed");
                    remaining_bytes = remaining_bytes[len..];
                //}
            }
        }
    }*/

    bytes.to_vec()
}

#[allow(unused)]
fn convert_little_endian_to_current(bytes: Bytes<'_>, channels: &ChannelList, rectangle: IntegerBounds) -> ByteVec { // TODO is this really not already somewhere else?
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

    bytes.to_vec()
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
    pub fn differences_to_samples(buffer: &mut [u8]) {
        // The naive implementation is very simple:
        //
        // for index in 1..buffer.len() {
        //    buffer[index] = (buffer[index - 1] as i32 + buffer[index] as i32 - 128) as u8;
        // }
        //
        // But we process elements in pairs to take advantage of instruction-level parallelism.
        // When computations within a pair do not depend on each other, they can be processed in parallel.
        // Since this function is responsible for a very large chunk of execution time,
        // this tweak alone improves decoding performance of RLE images by 20%.
        let mut previous = buffer[0] as i16;
        for chunk in &mut buffer[1..].chunks_exact_mut(2) {
            // no bounds checks here due to indices and chunk size being constant
            let diff0 = chunk[0] as i16;
            let diff1 = chunk[1] as i16;
            let sample0 = (previous + diff0 - 128) as u8;
            let sample1 = (previous + diff0 + diff1 - 128 * 2) as u8;
            chunk[0] = sample0;
            chunk[1] = sample1;
            previous = sample1 as i16;
        }
        // handle the remaining elements at the end not processed by the loop over pairs, if present
        if (buffer.len() % 1 == 1) && buffer.len() > 1 {
            let diff = buffer.last_mut().unwrap();
            *diff = (previous + *diff as i16 - 128) as u8;
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


#[cfg(test)]
pub mod test {
    use super::*;
    use crate::meta::attribute::ChannelDescription;
    use crate::block::samples::IntoNativeSample;

    #[test]
    fn roundtrip_endianness_mixed_channels(){
        let a32 = ChannelDescription::new("A", SampleType::F32, true);
        let y16 = ChannelDescription::new("Y", SampleType::F16, true);
        let channels = ChannelList::new(smallvec![ a32, y16 ]);

        let data = vec![
            23582740683_f32.to_ne_bytes().as_slice(),
            35827420683_f32.to_ne_bytes().as_slice(),
            27406832358_f32.to_f16().to_ne_bytes().as_slice(),
            74062358283_f32.to_f16().to_ne_bytes().as_slice(),

            52582740683_f32.to_ne_bytes().as_slice(),
            45827420683_f32.to_ne_bytes().as_slice(),
            15406832358_f32.to_f16().to_ne_bytes().as_slice(),
            65062358283_f32.to_f16().to_ne_bytes().as_slice(),
        ].into_iter().flatten().map(|x| *x).collect();

        roundtrip_convert_endianness(
            data, &channels,
            IntegerBounds::from_dimensions((2, 2))
        );
    }

    fn roundtrip_convert_endianness(
        current_endian: ByteVec, channels: &ChannelList, rectangle: IntegerBounds
    ){
        let little_endian = convert_current_to_little_endian(
            &current_endian, channels, rectangle
        );

        let current_endian_decoded = convert_little_endian_to_current(
            &little_endian, channels, rectangle
        );

        assert_eq!(current_endian, current_endian_decoded, "endianness conversion failed");
    }
}