
// see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfCompressor.cpp


use super::*;
use super::optimize_bytes::*;
use crate::error::Result;

// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot


pub fn decompress_bytes(data: Bytes<'_>, _expected_byte_size: usize) -> Result<ByteVec> {
    let mut decompressed = miniz_oxide::inflate
        // TODO ::decompress_to_vec_zlib_with_limit(data, expected_byte_size)
        ::decompress_to_vec_zlib(data)
        .map_err(|_| Error::invalid("zlib-compressed data malformed"))?;

    differences_to_samples(&mut decompressed);
    interleave_byte_blocks(&mut decompressed);
    Ok(decompressed)
}

pub fn compress_bytes(packed: Bytes<'_>) -> Result<ByteVec> {
    let mut packed = Vec::from(packed); // TODO no alloc
    separate_bytes_fragments(&mut packed);
    samples_to_differences(&mut packed);

    Ok(miniz_oxide::deflate::compress_to_vec_zlib(packed.as_slice(), 4))
}
