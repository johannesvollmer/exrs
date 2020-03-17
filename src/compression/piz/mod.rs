
//! The PIZ compression method is a wavelet compression,
//! based on the PIZ image format, customized for OpenEXR.
// inspired by  https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfPizCompressor.cpp

mod huffman;
mod wavelet;


use super::*;
use super::Result;
use crate::meta::attributes::{IntRect, SampleType};
use crate::meta::{Header};
use crate::io::Data;
use crate::math::Vec2;


const U16_RANGE: i32 = (1 << 16);
const BITMAP_SIZE: i32  = (U16_RANGE >> 3);

#[derive(Eq, PartialEq, Ord, PartialOrd, Debug, Copy, Clone)]
enum Format {
    Independent,
    Native
}

pub fn decompress_bytes(
    header: &Header,
    compressed: ByteVec,
    rectangle: IntRect,
    expected_byte_size: usize,
) -> Result<Vec<u8>>
{

    struct ChannelData {
        start_index: usize,
        end_index: usize,
        number_samples: Vec2<usize>,
        y_samples: usize,
        size: usize,
    }

    let Vec2(max_scan_line_size, scan_line_count) = header.default_block_pixel_size();

//    PizCompressor::PizCompressor
//        (const Header &hdr,
//    size_t maxScanLineSize,
//    size_t numScanLines)
//    :
//    Compressor (hdr),
//    _maxScanLineSize (maxScanLineSize),
//    _format (XDR),
//    _numScanLines (numScanLines),
//    _tmpBuffer (0),
//    _outBuffer (0),
//    _numChans (0),
//    _channels (hdr.channels()),
//    _channelData (0)
//    {
//        (void) _maxScanLineSize;
//        size_t tmpBufferSize = uiMult (maxScanLineSize, numScanLines) / 2;
    let tmp_buffer_size = (max_scan_line_size * scan_line_count) / 2;
//
//        size_t outBufferSize =
//        uiAdd (uiMult (maxScanLineSize, numScanLines),
//               size_t (65536 + 8192));
    let out_buffer_size = (max_scan_line_size * scan_line_count) + 65536 + 8192; // TODO not use expected byte size?
//
//        _tmpBuffer = new unsigned short
//        [checkArraySize (tmpBufferSize, sizeof (unsigned short))];
    let mut tmp_buffer = vec![0_u16; tmp_buffer_size];

//        _outBuffer = new char [outBufferSize];
    let mut out = Vec::with_capacity(out_buffer_size);

//        const ChannelList &channels = header().channels();
//        bool onlyHalfChannels = true;
//
//        for (ChannelList::ConstIterator c = channels.begin();
//        c != channels.end();
//        ++c)
//        {
//            _numChans++;
//
//            assert (pixelTypeSize (c.channel().type) % pixelTypeSize (HALF) == 0);
//
//            if (c.channel().type != HALF)
//              onlyHalfChannels = false;
//        }

    // TODO only once per header!
    let has_only_half_channels = header.channels.list
        .iter().all(|channel| channel.sample_type == SampleType::F16);

//
//        _channelData = new ChannelData[_numChans];
//

//        const Box2i &dataWindow = hdr.dataWindow();
//
//        _minX = dataWindow.min.x;
//        _maxX = dataWindow.max.x;
//        _maxY = dataWindow.max.y;
//
//        //
//        // We can support uncompressed data in the machine's native format
//        // if all image channels are of type HALF, and if the Xdr and the
//        // native represenations of a half have the same size.
//        //

    let format =
        if has_only_half_channels { Format::Native }
        else { Format::Independent }; // half is always 16 bit in Rust???

//
//        if (onlyHalfChannels && (sizeof (half) == pixelTypeSize (HALF)))
//        _format = NATIVE;
//    }


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
    if compressed.len() == 0 {
        return Ok(Vec::new())
    }

//        // Determine the layout of the compressed pixel data
//        _minX = dataWindow.min.x;
//        _maxX = dataWindow.max.x;
//        _maxY = dataWindow.max.y;

//        int minX = range.min.x;
//        int maxX = range.max.x;
//        int minY = range.min.y;
//        int maxY = range.max.y;
//
//        if (maxY > _maxY) // select smaller of maxY and _maxY
//        maxY = _maxY;
//
//        if (maxX > _maxX)
//        maxX = _maxX;


    let _min_x = rectangle.position.0;
    let min_y = rectangle.position.1;

    let mut _max_x = rectangle.max().0;
    let mut max_y = rectangle.max().1;

    // TODO rustify
    if _max_x > header.data_window().max().0 {
        _max_x = header.data_window().max().0;
    }

    // let max_y = max_y.min(_max_y);
    if max_y > header.data_window().max().1 {
        max_y = header.data_window().max().1;
    }

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

    let mut channel_data: Vec<ChannelData> = Vec::new();

    let mut tmp_buffer_end = 0;

    for (_index, channel) in header.channels.list.iter().enumerate() {

        let channel = ChannelData {
            start_index: tmp_buffer_end,
            end_index: tmp_buffer_end,
            y_samples: channel.sampling.1,
            number_samples: channel.subsampled_resolution(rectangle.size),
            size: (channel.sample_type.bytes_per_sample() / SampleType::F16.bytes_per_sample())
        };

        tmp_buffer_end += channel.number_samples.0 * channel.number_samples.1 * channel.size;
        channel_data.push(channel);
    }

//
//        //
//        //
//        // Read range compression data
//        //
//
//        unsigned short minNonZero;
//        unsigned short maxNonZero;
//
//        AutoArray <unsigned char, BITMAP_SIZE> bitmap;
//        memset (bitmap, 0, sizeof (unsigned char) * BITMAP_SIZE);

    let mut bitmap = vec![0_u8; BITMAP_SIZE as usize];


//
//        Xdr::read <CharPtrIO> (inPtr, minNonZero);
//        Xdr::read <CharPtrIO> (inPtr, maxNonZero);

    let mut read = compressed.as_slice();

    let min_non_zero = u16::read(&mut read)?;
    let max_non_zero = u16::read(&mut read)?;

//
//        if (maxNonZero >= BITMAP_SIZE)
//        {
//            throw InputExc ("Error in header for PIZ-compressed data "
//            "(invalid bitmap size).");
//        }
    if max_non_zero as i32 >= BITMAP_SIZE {
        println!("invalid bitmap size");
        return Err(Error::invalid("compression data"));
    }
//
//        if (minNonZero <= maxNonZero)
//        {
//            Xdr::read <CharPtrIO> (inPtr, (char *) &bitmap[0] + minNonZero,
//                                   maxNonZero - minNonZero + 1);
//        }

    if min_non_zero <= max_non_zero {
        let length = max_non_zero - min_non_zero + 1;

        bitmap[ min_non_zero as usize .. (min_non_zero + length) as usize ]
            .copy_from_slice(&read[.. length as usize]);

    }
//
//        AutoArray <unsigned short, USHORT_RANGE> lut;
//        unsigned short maxValue = reverseLutFromBitmap (bitmap, lut);
//
    let (lookup_table, max_value) = reverse_lookup_table_from_bitmap(&bitmap);

//        // Huffman decoding
//        int length;
//        Xdr::read <CharPtrIO> (inPtr, length);
//
    let length = i32::read(&mut read)?;
    debug!(length);

//        if (length > inSize) {
//            throw InputExc ("Error in header for PIZ-compressed data "
//            "(invalid array length).");
//        }
//
//        hufUncompress (inPtr, length, _tmpBuffer, tmpBufferEnd - _tmpBuffer);

    if length as usize > read.len() {
        println!("invalid array length");
        return Err(Error::invalid("compression data"));
    }

    // TODO use DynamicHuffmanCodec?
    huffman::decompress(&read[..length as usize], &mut tmp_buffer)?;

//
//        //
//        // Wavelet decoding
//        //
//
//        for (int i = 0; i < _numChans; ++i) {
//            ChannelData &cd = _channelData[i];
//            for (int j = 0; j < cd.size; ++j) {
//                wav2Decode (cd.start + j,
//                            cd.nx, cd.size,
//                            cd.ny, cd.nx * cd.size,
//                            maxValue);
//            }
//        }

    for channel in &channel_data {
        for size in 0..channel.size {
            wavelet::decode(
                &mut tmp_buffer[(channel.start_index + size) ..],
                channel.number_samples,
                Vec2(channel.size, channel.number_samples.0 * channel.size),
                max_value
            )?;
        }
    }

//        // Expand the pixel data to their original range
//        applyLut (lut, _tmpBuffer, tmpBufferEnd - _tmpBuffer);
    apply_lookup_table(&mut tmp_buffer, &lookup_table);


//        // Rearrange the pixel data into the format expected by the caller.
//        char *outEnd = _outBuffer;
//     let mut out: Vec<u8> = Vec::new();

//        if (_format == XDR) {
//            //
//            // Machine-independent (Xdr) data format
//            //
//
//            for (int y = minY; y <= maxY; ++y) {
//                for (int i = 0; i < _numChans; ++i) {
//                    ChannelData &cd = _channelData[i];
//
//                    if (modp (y, cd.ys) != 0)
//                    continue;
//
//                    for (int x = cd.nx * cd.size; x > 0; --x) {
//                        Xdr::write <CharPtrIO> (outEnd, *cd.end);
//                        ++cd.end;
//                    }
//                }
//            }
//        }

    if format == Format::Independent {
        for y in min_y ..= max_y {
            for channel in &mut channel_data {
                if mod_p(y, channel.y_samples as i32) != 0 {
                    continue;
                }

                // TODO this should be a simple mirroring slice copy?
                for _x in (0 .. channel.number_samples.0 * channel.size).rev() {
                    u16::write(tmp_buffer[channel.end_index], &mut out)?;
                    // out.push(tmp_buffer[channel.end_index as usize]);
                    channel.end_index += 1;
                }
            }
        }
    }


//        else {
//            // Native, machine-dependent data format
//            for (int y = minY; y <= maxY; ++y) {
//                for (int i = 0; i < _numChans; ++i) {
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

    else { // native format
        for y in min_y ..= max_y {
            for channel in &mut channel_data {
                if mod_p(y, channel.y_samples as i32) != 0 {
                    continue;
                }

                // copy each channel
                use lebe::io::WriteEndian;

                let n = channel.number_samples.0 * channel.size;
                // out.extend_from_slice(&tmp_buffer[channel.end_index as usize .. (channel.end_index + n) as usize]);

                out.write_as_native_endian(&tmp_buffer[channel.end_index .. (channel.end_index + n)])?;

                // #[cfg(target_endian = "little")] { out.write_le(&tmp_buffer[channel.end_index as usize .. (channel.end_index + n) as usize])?; }
                // #[cfg(target_endian = "big")] { out.write_be(&tmp_buffer[channel.end_index as usize .. (channel.end_index + n) as usize])?; }

                channel.end_index += n;
            }
        }
    }
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
    for index in 1..channel_data.len() {
        assert_eq!(channel_data[index - 1].end_index, channel_data[index].start_index);
    }

    assert_eq!(channel_data.last().unwrap().end_index, tmp_buffer.len());
    assert_eq!(out.len(), expected_byte_size);

    Ok(out)
}


//
// Integer division and remainder where the
// remainder of x/y is always positive:
//
//	divp(x,y) == floor (double(x) / double (y))
//	modp(x,y) == x - y * divp(x,y)
//
//
//    inline int
//    divp (int x, int y)
//    {
//       return (x >= 0)? ((y >= 0)?  (     x  / y): -(      x  / -y)):
//       ((y >= 0)? -((y-1-x) / y):  ((-y-1-x) / -y));
//    }
//
//
//    inline int
//    modp (int x, int y)
//    {
//       return x - y * divp (x, y);
//    }

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




fn reverse_lookup_table_from_bitmap(bitmap: Bytes<'_>) -> (Vec<u16>, u16) {
//    int k = 0;
//
//    for (int i = 0; i < USHORT_RANGE; ++i)
//    {
//        if ((i == 0) || (bitmap[i >> 3] & (1 << (i & 7))))
//        lut[k++] = i;
//    }
//
//    int n = k - 1;
//
//    while (k < USHORT_RANGE)
//    lut[k++] = 0;
//
//    return n;		// maximum k where lut[k] is non-zero,


    // assert_eq!(U16_RANGE as u16 as i32, U16_RANGE); // TODO remove

    let mut table = Vec::with_capacity(U16_RANGE as usize);

    for index in 0 .. U16_RANGE as u16 {
        if index == 0 || (bitmap[index as usize >> 3] as usize & (1 << (index as usize & 7)) != 0) { // TODO where should be cast?
            table.push(index);
        }
    }

    let max_value = table.len() as u16;
    // assert_eq!(table.len() as u16 as usize, table.len());  // TODO remove

    table.resize(U16_RANGE as usize, 0);

    (table, max_value)
}

fn apply_lookup_table(data: &mut [u16], table: &[u16]) {
//    for (int i = 0; i < nData; ++i)
//        data[i] = lut[data[i]];
    for data in data {
        *data = table[*data as usize];
    }
}


pub fn compress_bytes(
    _header: &Header,
    _packed: Bytes<'_>,
    _rectangle: IntRect
) -> Result<ByteVec>
{
    unimplemented!();
}


#[cfg(test)]
mod test {
    #[test]
    fn x(){}
}