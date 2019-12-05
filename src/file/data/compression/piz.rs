use super::*;
use super::Result;
use crate::file::meta::attributes::{I32Box2};
use crate::file::meta::Header;

// inspired by  https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfPizCompressor.cpp


pub fn decompress_bytes(
    compressed: ByteVec,
//    tile_window: I32Box2
) -> Result<Vec<u8>> {
    let header: &Header = unimplemented!();
    let line_size = header.data_window.dimensions().0 as usize;
    let tile_window = I32Box2 {
        x_min: 0,
        y_min: 0,
        x_max: 0,
        y_max: 0
    };

//    int
//    PizCompressor::uncompress (const char *inPtr,
//    int inSize,
//    IMATH_NAMESPACE::Box2i range,
//    const char *&outPtr)
//    {
//        //
//        // This is the cunompress function which is used by both the tiled and
//        // scanline decompression routines.
//        //
//
//        //
//        // Special case - empty input buffer
//        //
//
//        if (inSize == 0)
//        {
//            outPtr = _outBuffer;
//            return 0;
//        }
    if line_size == 0 {
        return Ok(vec![])
    }

//        //
//        // Determine the layout of the compressed pixel data
//        //
//
//        int minX = range.min.x;
//        int maxX = range.max.x;
//        int minY = range.min.y;
//        int maxY = range.max.y;
//
//        if (maxY > _maxY)
//        maxY = _maxY;
//
//        if (maxX > _maxX)
//        maxX = _maxX;


    let min_x = tile_window.x_min as usize;
    let min_y = header.data_window.y_min as usize;

    let max_x = header.data_window.x_max.min(tile_window.x_max) as usize;
    let max_y = header.data_window.y_max.min(tile_window.y_max) as usize;

//
//        unsigned short *tmpBufferEnd = _tmpBuffer;
//        int i = 0;
//
//        for (ChannelList::ConstIterator c = _channels.begin(); c != _channels.end(); ++c, ++i) {
//            ChannelData &cd = _channelData[i];
//
//            cd.start = tmpBufferEnd;
//            cd.end = cd.start;
//
//            cd.nx = numSamples (c.channel().xSampling, minX, maxX);
//            cd.ny = numSamples (c.channel().ySampling, minY, maxY);
//            cd.ys = c.channel().ySampling;
//
//            cd.size = pixelTypeSize (c.channel().type) / pixelTypeSize (HALF);
//
//            tmpBufferEnd += cd.nx * cd.ny * cd.size;
//        }

    for channel in &header.channels.list {


    }

//
//        //
//        // Read range compression data
//        //
//
//        unsigned short minNonZero;
//        unsigned short maxNonZero;
//
//        AutoArray <unsigned char, BITMAP_SIZE> bitmap;
//        memset (bitmap, 0, sizeof (unsigned char) * BITMAP_SIZE);
//
//        Xdr::read <CharPtrIO> (inPtr, minNonZero);
//        Xdr::read <CharPtrIO> (inPtr, maxNonZero);
//
//        if (maxNonZero >= BITMAP_SIZE)
//        {
//            throw InputExc ("Error in header for PIZ-compressed data "
//            "(invalid bitmap size).");
//        }
//
//        if (minNonZero <= maxNonZero)
//        {
//            Xdr::read <CharPtrIO> (inPtr, (char *) &bitmap[0] + minNonZero,
//                                   maxNonZero - minNonZero + 1);
//        }
//
//        AutoArray <unsigned short, USHORT_RANGE> lut;
//        unsigned short maxValue = reverseLutFromBitmap (bitmap, lut);
//
//        //
//        // Huffman decoding
//        //
//
//        int length;
//        Xdr::read <CharPtrIO> (inPtr, length);
//
//        if (length > inSize)
//        {
//            throw InputExc ("Error in header for PIZ-compressed data "
//            "(invalid array length).");
//        }
//
//        hufUncompress (inPtr, length, _tmpBuffer, tmpBufferEnd - _tmpBuffer);
//
//        //
//        // Wavelet decoding
//        //
//
//        for (int i = 0; i < _numChans; ++i)
//        {
//            ChannelData &cd = _channelData[i];
//
//            for (int j = 0; j < cd.size; ++j)
//            {
//                wav2Decode (cd.start + j,
//                            cd.nx, cd.size,
//                            cd.ny, cd.nx * cd.size,
//                            maxValue);
//            }
//        }
//
//        //
//        // Expand the pixel data to their original range
//        //
//
//        applyLut (lut, _tmpBuffer, tmpBufferEnd - _tmpBuffer);
//
//        //
//        // Rearrange the pixel data into the format expected by the caller.
//        //
//
//        char *outEnd = _outBuffer;
//
//        if (_format == XDR)
//        {
//            //
//            // Machine-independent (Xdr) data format
//            //
//
//            for (int y = minY; y <= maxY; ++y)
//            {
//                for (int i = 0; i < _numChans; ++i)
//                {
//                    ChannelData &cd = _channelData[i];
//
//                    if (modp (y, cd.ys) != 0)
//                    continue;
//
//                    for (int x = cd.nx * cd.size; x > 0; --x)
//                    {
//                        Xdr::write <CharPtrIO> (outEnd, *cd.end);
//                        ++cd.end;
//                    }
//                }
//            }
//        }
//        else
//        {
//            //
//            // Native, machine-dependent data format
//            //
//
//            for (int y = minY; y <= maxY; ++y)
//            {
//                for (int i = 0; i < _numChans; ++i)
//                {
//                    ChannelData &cd = _channelData[i];
//
//                    if (modp (y, cd.ys) != 0)
//                    continue;
//
//                    int n = cd.nx * cd.size;
//                    memcpy (outEnd, cd.end, n * sizeof (unsigned short));
//                    outEnd += n * sizeof (unsigned short);
//                    cd.end += n;
//                }
//            }
//        }
//
//        #if defined (DEBUG)
//
//        for (int i = 1; i < _numChans; ++i)
//        assert (_channelData[i-1].end == _channelData[i].start);
//
//        assert (_channelData[_numChans-1].end == tmpBufferEnd);
//
//        #endif
//
//        outPtr = _outBuffer;
//        return outEnd - _outBuffer;
//    }


    unimplemented!()
    // differences_to_samples(&mut decompressed);
    // decompressed = interleave_byte_blocks(&decompressed);
    // super::uncompressed::unpack(target, &decompressed, line_size) // convert to machine-dependent endianess
}

pub fn compress_bytes(packed: Bytes) -> Result<ByteVec> {
    unimplemented!();
}
