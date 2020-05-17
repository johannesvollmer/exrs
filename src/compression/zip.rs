
// see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfCompressor.cpp


use super::*;
use super::optimize_bytes::*;

use std::io;
use crate::error::Result;
use deflate::write::ZlibEncoder;
use inflate::inflate_bytes_zlib;

// scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot


pub fn decompress_bytes(data: Bytes<'_>) -> Result<ByteVec> {
    let mut decompressed = inflate_bytes_zlib(data)
        .map_err(|msg| Error::invalid(msg))?;

    differences_to_samples(&mut decompressed);
    interleave_byte_blocks(&mut decompressed);
    Ok(decompressed)
}

pub fn compress_bytes(packed: Bytes<'_>) -> Result<ByteVec> {
    let mut packed = Vec::from(packed); // TODO no alloc
    separate_bytes_fragments(&mut packed);
    samples_to_differences(&mut packed);

    {
        // TODO fine-tune compression options
        let mut compressor = ZlibEncoder::new(
            Vec::with_capacity(packed.len()),
            deflate::Compression::Fast
        );

        io::copy(&mut packed.as_slice(), &mut compressor)?;
        Ok(compressor.finish()?)
    }
}
