use super::*;
use super::Result;
use crate::file::meta::attributes::{I32Box2, PixelType};
use crate::file::meta::{Header};
use crate::file::data::compression::Error::InvalidData;
use crate::error::ReadResult;
use crate::file::io::{Data, Write, Read};
use byteorder::{WriteBytesExt, ReadBytesExt};

// inspired by  https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfPizCompressor.cpp


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
//    return (x >= 0)? ((y >= 0)?  (     x  / y): -(      x  / -y)):
//    ((y >= 0)? -((y-1-x) / y):  ((-y-1-x) / -y));
//    }
//
//
//    inline int
//    modp (int x, int y)
//    {
//    return x - y * divp (x, y);
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


const u16_range: i32 = (1 << 16);
const bitmap_size: i32  = (u16_range >> 3); // rly



pub fn decompress_bytes(
    header: &Header,
    compressed: ByteVec,
    rectangle: I32Box2,
    expected_byte_size: usize,
) -> Result<Vec<u8>>
{

    struct ChannelData {
        start_index: u32,
        end_index: u32,
        number_samples_x: u32,
        number_samples_y: u32,
        y_samples: u32,
        size: u32,
    }

    let xdr = true;
    let format = xdr; // TODO

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
//        // TODO: Remove this when we can change the ABI
//        (void) _maxScanLineSize;
//        size_t tmpBufferSize = uiMult (maxScanLineSize, numScanLines) / 2;
//
//        size_t outBufferSize =
//        uiAdd (uiMult (maxScanLineSize, numScanLines),
//               size_t (65536 + 8192));
//
//        _tmpBuffer = new unsigned short
//        [checkArraySize (tmpBufferSize, sizeof (unsigned short))];
//
//        _outBuffer = new char [outBufferSize];
//
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
//            onlyHalfChannels = false;
//        }

    // TODO only once per header!
    let has_only_half_channels = header.channels.list
        .iter().all(|channel| channel.pixel_type == PixelType::F16);

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

    let use_native_format = has_only_half_channels; // half is always 16 bit

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

//        //
//        // Determine the layout of the compressed pixel data
//        //
//
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


    let min_x = rectangle.x_min;
    let min_y = rectangle.y_min;

    let mut max_x = rectangle.x_max;
    let mut max_y = rectangle.y_max;

    // TODO rustify
    if max_x > header.data_window.x_max {
        max_x = header.data_window.x_max;
    }

    if max_y > header.data_window.y_max {
        max_y = header.data_window.y_max;
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

    let mut tmp_buffer = vec![0_u16; header.data_window.dimensions().0 as usize]; // TODO better size calculation?
    let mut tmp_buffer_end = 0_u32;

    for (index, channel) in header.channels.list.iter().enumerate() {
        let (number_samples_x, number_samples_y) = channel.subsampled_resolution(rectangle.dimensions());

        let channel = ChannelData {
            start_index: tmp_buffer_end,
            end_index: tmp_buffer_end,
            y_samples: channel.y_sampling as u32,
            number_samples_x, number_samples_y,
            size: channel.pixel_type.bytes_per_sample() / PixelType::F16.bytes_per_sample()
        };

        tmp_buffer_end += channel.number_samples_x * channel.number_samples_y * channel.size;
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

    let mut bitmap = vec![0_u8; bitmap_size as usize];


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
    if max_non_zero as i32 >= bitmap_size {
        println!("invalid bitmap size");
        return Err(InvalidData);
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

//        //
//        // Huffman decoding
//        //
//
//        int length;
//        Xdr::read <CharPtrIO> (inPtr, length);
//
    let length = i32::read(&mut read)?;

//        if (length > inSize)
//        {
//            throw InputExc ("Error in header for PIZ-compressed data "
//            "(invalid array length).");
//        }
//
//        hufUncompress (inPtr, length, _tmpBuffer, tmpBufferEnd - _tmpBuffer);

    if length as usize > read.len() {
        println!("invalid array length");
        return Err(InvalidData);
    }

    // TODO use DynamicHuffmanCodec?
    huffman_decompress(&read[..length as usize], &mut tmp_buffer)?;

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
    for channel in &channel_data {
        for size in 0..channel.size {
            wave_2_decode(
                &mut tmp_buffer[(channel.start_index + size) as usize..],
                channel.number_samples_x, channel.size, channel.number_samples_y,
                channel.number_samples_x * channel.size, max_value
            )?;
        }
    }

//
//        //
//        // Expand the pixel data to their original range
//        //
//
//        applyLut (lut, _tmpBuffer, tmpBufferEnd - _tmpBuffer);
    apply_lookup_table(&mut tmp_buffer, &lookup_table);


//
//        //
//        // Rearrange the pixel data into the format expected by the caller.
//        //
//
//        char *outEnd = _outBuffer;
//
    // TODO what is XDR?
    let mut out = Vec::new();

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

    if format == xdr {
        for y in min_y ..= max_y {
            for channel in &mut channel_data {
                if mod_p(y, channel.y_samples as i32) != 0 {
                    continue;
                }

                // TODO this should be a simple mirroring slice copy?
                for x in (0 .. channel.number_samples_x * channel.size).rev() {
                    out.push(tmp_buffer[channel.end_index as usize]);
                    channel.end_index += 1;
                }
            }
        }
    }


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

    else { // native format
        for y in min_y ..= max_y {
            for channel in &mut channel_data {
                if mod_p(y, channel.y_samples as i32) != 0 {
                    continue;
                }

                // copy each channel
                let n = channel.number_samples_x * channel.size;
                out.extend_from_slice(&tmp_buffer[channel.end_index as usize .. (channel.end_index + n) as usize]);
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

    assert_eq!(channel_data.last().unwrap().end_index as usize, tmp_buffer.len());

//    Ok(out)
    unimplemented!("Ok(out)")
}


// https://github.com/AcademySoftwareFoundation/openexr/blob/master/OpenEXR/IlmImf/ImfHuf.cpp
fn huffman_decompress(data: &[u8], result: &mut [u16]) -> ReadResult<()> {


    const HUF_ENCBITS : i32 = 16;			// literal (value) bit length
    const HUF_DECBITS : i32 = 14;			// decoding bit size (>= 8)

    const HUF_ENCSIZE : i32 = (1 << HUF_ENCBITS) + 1;	// encoding table size
    const HUF_DECSIZE : i32 =  1 << HUF_DECBITS;	// decoding table size
    const HUF_DECMASK : i32 = HUF_DECSIZE - 1;

    struct HufDecoder {
        len: i32,
        lit: i32,
        p: i32
    }


    fn huf_length (code: i64) -> i64 {
        return code & 63;
    }

    fn huf_code (code: i64) -> i64 {
        return code >> 6;
    }

//    inline void
//    outputBits (int nBits, Int64 bits, Int64 &c, int &lc, char *&out)
//    {
//        c <<= nBits;
//        lc += nBits;
//
//        c |= bits;
//
//        while (lc >= 8)
//            *out++ = (c >> (lc -= 8));
//    }

    /*fn output_bits (bits: i64, bit_count: i32, c: &mut i64, lc: &mut i32, out: &mut impl Write) {
        *c = *c << bit_count;
        *lc += bit_count;
        *c = *c | bits;

        while *lc >= 8 {
            out.write_u8(c >> (lc -= 8))
        }
    }*/

//    inline Int64
//    getBits (int nBits, Int64 &c, int &lc, const char *&in)
//    {
//        while (lc < nBits)
//        {
    //        c = (c << 8) | *(unsigned char *)(in++);
    //        lc += 8;
//        }
//
//        lc -= nBits;
//        return (c >> lc) & ((1 << nBits) - 1);
//    }

    fn get_bits (bit_count: i32, c: &mut i64, lc: &mut i32, input: &mut impl Read) -> std::io::Result<i64> {
        while *lc < bit_count {
            *c = (*c << 8) | input.read_u8()? as i64;
            *lc += 8;
        }

        *lc -= bit_count;
        Ok((*c >> *lc as i64) & ((1 >> bit_count as i64) - 1))
    }


//    void
//    hufCanonicalCodeTable (Int64 hcode[HUF_ENCSIZE])
//    {
    fn canonical_code_table(table: &mut [i64]) {
        let mut count = [0_i64; 59];

//        Int64 n[59];
//
//        //
//        // For each i from 0 through 58, count the
//        // number of different codes of length i, and
//        // store the count in n[i].
//        //
//
//        for (int i = 0; i <= 58; ++i)
//        n[i] = 0;
//
//        for (int i = 0; i < HUF_ENCSIZE; ++i)
//        n[hcode[i]] += 1;
        for entry_index in 0 .. HUF_ENCSIZE {
            count[table[entry_index as usize] as usize] += 1;
        }
//
//        //
//        // For each i from 58 through 1, compute the
//        // numerically lowest code with length i, and
//        // store that code in n[i].
//        //
//
//        Int64 c = 0;
//
//        for (int i = 58; i > 0; --i)
//        {
//            Int64 nc = ((c + n[i]) >> 1);
//            n[i] = c;
//            c = nc;
//        }
        let mut c = 0;
        for i in (0 .. 59).rev() {
            let nc = ((c + count[i]) >> 1);
            count[i] = c;
            c = nc;
        }
//
//        //
//        // hcode[i] contains the length, l, of the
//        // code for symbol i.  Assign the next available
//        // code of length l to the symbol and store both
//        // l and the code in hcode[i].
//        //
//
//        for (int i = 0; i < HUF_ENCSIZE; ++i)
//        {
//            int l = hcode[i];
//
//            if (l > 0)
//            hcode[i] = l | (n[l]++ << 6);
//        }

        for entry_index in 0 .. HUF_ENCSIZE {
            let l = table[entry_index as usize];
            if l > 0 {
                table[entry_index as usize] = l | (count[l as usize] << 6);
                count[l as usize] += 1;
            }
        }
//    }
    }



//    void
//    hufDecode
//        (const Int64 * 	hcode,	// i : encoding table
//    const HufDec * 	hdecod,	// i : decoding table
//    const char* 	in,	// i : compressed input buffer
//    int		ni,	// i : input size (in bits)
//    int		rlc,	// i : run-length code
//    int		no,	// i : expected output size (in bytes)
//    unsigned short*	out)	//  o: uncompressed output buffer
//    {
//        Int64 c = 0;
//        int lc = 0;
//        unsigned short * outb = out;
//        unsigned short * oe = out + no;
//        const char * ie = in + (ni + 7) / 8; // input byte size
//
//        //
//        // Loop on input bytes
//        //
//
//        while (in < ie)
//        {
//            getChar (c, lc, in);
//
//            //
//            // Access decoding table
//            //
//
//            while (lc >= HUF_DECBITS)
//                {
//                    const HufDec pl = hdecod[(c >> (lc-HUF_DECBITS)) & HUF_DECMASK];
//
//                    if (pl.len)
//                    {
//                        //
//                        // Get short code
//                        //
//
//                        lc -= pl.len;
//                        getCode (pl.lit, rlc, c, lc, in, out, outb, oe);
//                    }
//                    else
//                    {
//                        if (!pl.p)
//                        invalidCode(); // wrong code
//
//                        //
//                        // Search long code
//                        //
//
//                        int j;
//
//                        for (j = 0; j < pl.lit; j++)
//                        {
//                            int	l = hufLength (hcode[pl.p[j]]);
//
//                            while (lc < l && in < ie)	// get more bits
//                            getChar (c, lc, in);
//
//                            if (lc >= l)
//                            {
//                                if (hufCode (hcode[pl.p[j]]) ==
//                                    ((c >> (lc - l)) & ((Int64(1) << l) - 1)))
//                                {
//                                    //
//                                    // Found : get long code
//                                    //
//
//                                    lc -= l;
//                                    getCode (pl.p[j], rlc, c, lc, in, out, outb, oe);
//                                    break;
//                                }
//                            }
//                        }
//
//                        if (j == pl.lit)
//                        invalidCode(); // Not found
//                    }
//                }
//        }
//
//        //
//        // Get remaining (short) codes
//        //
//
//        int i = (8 - ni) & 7;
//        c >>= i;
//        lc -= i;
//
//        while (lc > 0)
//            {
//                const HufDec pl = hdecod[(c << (HUF_DECBITS - lc)) & HUF_DECMASK];
//
//                if (pl.len)
//                {
//                    lc -= pl.len;
//                    getCode (pl.lit, rlc, c, lc, in, out, outb, oe);
//                }
//                else
//                {
//                    invalidCode(); // wrong (long) code
//                }
//            }
//
//        if (out - outb != no)
//        notEnoughData ();
//    }





}



// https://github.com/AcademySoftwareFoundation/openexr/blob/8cd1b9210855fa4f6923c1b94df8a86166be19b1/OpenEXR/IlmImf/ImfWav.cpp
fn wave_2_decode(buffer: &[u16], x_size: u32, x_offset: u32, y_size: u32, y_offset: u32, max: u16 ) -> ReadResult<()> {
    unimplemented!()
}

fn reverse_lookup_table_from_bitmap(bitmap: Bytes) -> (Vec<u16>, u16) {
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

//    let mut k = 0;

    assert_eq!(u16_range as u16 as i32, u16_range);

    let mut table = Vec::with_capacity(u16_range as usize);

    for index in 0 .. u16_range as u16 {
        if index == 0 || (bitmap[index as usize >> 3] as usize & (1 << (index as usize & 7)) != 0) { // TODO where should be cast?
//            lut[k] = i;
//            k += 1;
            table.push(index);
        }
    }

    let n = table.len() as u16;
    assert_eq!(table.len() as u16 as usize, table.len());

    table.resize(u16_range as usize, 0);

    (table, n)
}

fn apply_lookup_table(data: &mut [u16], table: &[u16]) {
//    for (int i = 0; i < nData; ++i)
//        data[i] = lut[data[i]];
    for data in data {
        *data = table[*data as usize];
    }
}


pub fn compress_bytes(_packed: Bytes) -> Result<ByteVec> {
    unimplemented!();
}
