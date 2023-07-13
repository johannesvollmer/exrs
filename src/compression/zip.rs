
// see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfCompressor.cpp


use super::*;
use super::optimize_bytes::*;
use crate::error::Result;

// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot


pub fn decompress_bytes(
    channels: &ChannelList,
    data: ByteVec,
    rectangle: IntegerBounds,
    expected_byte_size: usize,
    _pedantic: bool,
) -> Result<ByteVec> {
    let options = zune_inflate::DeflateOptions::default().set_limit(expected_byte_size).set_size_hint(expected_byte_size);
    let mut decoder = zune_inflate::DeflateDecoder::new_with_options(&data, options);
    let mut decompressed = decoder.decode_zlib()
        .map_err(|_| Error::invalid("zlib-compressed data malformed"))?;

    differences_to_samples(&mut decompressed);
    interleave_byte_blocks(&mut decompressed);

    Ok(super::convert_little_endian_to_current(decompressed, channels, rectangle))// TODO no alloc
}

pub fn compress_bytes(channels: &ChannelList, uncompressed: ByteVec, rectangle: IntegerBounds) -> Result<ByteVec> {
    // see https://github.com/AcademySoftwareFoundation/openexr/blob/3bd93f85bcb74c77255f28cdbb913fdbfbb39dfe/OpenEXR/IlmImf/ImfTiledOutputFile.cpp#L750-L842
    let mut packed = convert_current_to_little_endian(uncompressed, channels, rectangle);

    separate_bytes_fragments(&mut packed);
    samples_to_differences(&mut packed);

    Ok(miniz_oxide::deflate::compress_to_vec_zlib(packed.as_slice(), 4))
}
