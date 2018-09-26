
// see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfCompressor.cpp


/// compresses 16 scan lines at once or
/// compresses 1 single scan line at once
// TODO don't instantiate a new decoder for every block?
use super::*;
use super::optimize_bytes::*;

use std::io::{self, Read};
use ::libflate::zlib::{Encoder, Decoder};




// TODO
// for scanline decompression routine, see https://github.com/openexr/openexr/blob/master/OpenEXR/IlmImf/ImfScanLineInputFile.cpp
// 1. Uncompress the data, if necessary (If the line is uncompressed, it's in XDR format, regardless of the compressor's output format.)
// 2. consider line_order?
// 3. Convert one scan line's worth of pixel data back from the machine-independent representation
// 4. Fill the frame buffer with pixel data, respective to sampling and whatnot


pub fn decompress_bytes(target: UncompressedData, data: &CompressedData, line_size: usize) -> Result<UncompressedData> {
    let mut decompressed = Vec::with_capacity(data.len());

    {// decompress
        let mut decompressor = Decoder::new(data.as_slice())
            .expect("io error when reading from in-memory vec");

        decompressor.read_to_end(&mut decompressed)?;
    };

    differences_to_samples(&mut decompressed);
    decompressed = interleave_byte_blocks(&decompressed);
    super::uncompressed::unpack(target, &decompressed, line_size) // convert to machine-dependent endianess
}

pub fn compress_bytes(data: &UncompressedData) -> Result<CompressedData> {
    let mut packed = super::uncompressed::pack(data)?; // convert from machine-dependent endianess
    packed = separate_bytes_fragments(&packed);
    samples_to_differences(&mut packed);

    {// compress
        let mut compressor = Encoder::new(Vec::with_capacity(packed.len()))
            .expect("io error when writing to in-memory vec");

        io::copy(&mut packed.as_slice(), &mut compressor).expect("io error when writing to in-memory vec");
        Ok(compressor.finish().into_result().expect("io error when writing to in-memory vec"))
    }
}
