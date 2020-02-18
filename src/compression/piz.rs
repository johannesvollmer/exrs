
//! The PIZ compression method is a wavelet compression,
//! based on the PIZ image format, customized for OpenEXR.

use super::*;
use super::Result;
use crate::meta::attributes::{IntRect, PixelType};
use crate::meta::{Header};
use crate::io::Data;
use crate::error::IoResult;
use crate::math::Vec2;
use std::io::{Write, Read};
use std::hash::Hasher;
use std::ops::Range;


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


const U16_RANGE: i32 = (1 << 16);
const BITMAP_SIZE: i32  = (U16_RANGE >> 3); // rly



pub fn decompress_bytes(
    header: &Header,
    compressed: ByteVec,
    rectangle: IntRect,
    _expected_byte_size: usize,
) -> Result<Vec<u8>>
{

    struct ChannelData {
        start_index: u32,
        end_index: u32,
        number_samples: Vec2<u32>,
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

    let _use_native_format = has_only_half_channels; // half is always 16 bit

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


    let _min_x = rectangle.position.0;
    let min_y = rectangle.position.1;

    let mut _max_x = rectangle.max().0;
    let mut max_y = rectangle.max().1;

    // TODO rustify
    if _max_x > header.data_window().max().0 {
        _max_x = header.data_window().max().0;
    }

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

    let mut tmp_buffer = vec![0_u16; header.data_size.0]; // TODO better size calculation?
    let mut tmp_buffer_end = 0_u32;

    for (_index, channel) in header.channels.list.iter().enumerate() {

        let channel = ChannelData {
            start_index: tmp_buffer_end,
            end_index: tmp_buffer_end,
            y_samples: channel.sampling.1 as u32,
            number_samples: channel.subsampled_resolution(rectangle.size).map(|x| x as u32),
            // number_samples_x, number_samples_y,
            size: (channel.pixel_type.bytes_per_sample() / PixelType::F16.bytes_per_sample()) as u32
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
        return Err(Error::invalid("compression data"));
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
                channel.number_samples.0, channel.size, channel.number_samples.1,
                channel.number_samples.0 * channel.size, max_value
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
                for _x in (0 .. channel.number_samples.0 * channel.size).rev() {
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
                let n = channel.number_samples.0 * channel.size;
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

// see https://github.com/AcademySoftwareFoundation/openexr/blob/88246d991e0318c043e6f584f7493da08a31f9f8/OpenEXR/IlmImf/ImfHuf.cpp
/// 16-bit Huffman compression and decompression.
/// Huffman compression and decompression routines written
///	by Christian Rouet for his PIZ image file format.
fn huffman_decompress(_data: &[u8], _result: &mut [u16]) -> IoResult<()> {

    const ENCODE_BITS: usize = 16;			// literal (value) bit length
    const DECODE_BITS: usize = 14;			// decoding bit size (>= 8)

    const ENCODE_SIZE: usize = (1 << ENCODE_BITS) + 1;	// encoding table size
    const DECODE_SIZE: usize =  1 << DECODE_BITS;	        // decoding table size
    const DECODE_MASK: usize = DECODE_SIZE - 1;


    //    struct HufDec
    //    {				// short code		long code
    //    //-------------------------------
    //    int		len:8;		// code length		0
    //    int		lit:24;		// lit			p size
    //    int	*	p;		// 0			lits
    //    };
    struct Decode {
        len_8b: i8,             // short: code length   | long: 0
        lit_24b: i32,           // short: lit           | long: p size
        start_index: usize,     // short: 0,            | long: lits
    }

    //    inline Int64
    //    hufLength (Int64 code)
    //    {
    //        return code & 63;
    //    }
    fn length(code: i64) -> i64 { code & 63 }

    //    inline Int64
    //    hufCode (Int64 code)
    //    {
    //        return code >> 6;
    //    }
    fn code(code: i64) -> i64 { code >> 6 }

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
    fn write_bits(count: i64, bits: i64, c: &mut i64, lc: &mut i64, mut out: impl Write) {
        *c = *c << count;
        lc += count;

        *c = *c | bits;

        while *lc >= 8 {
            *lc -= 8;
            out.write_u8( (c >> *lc) as u8); // TODO make sure never or always wraps?
        }
    }

    //    inline Int64
    //    getBits (int nBits, Int64 &c, int &lc, const char *&in)
    //    {
    //      while (lc < nBits)
    //      {
    //          c = (c << 8) | *(unsigned char *)(in++);
    //          lc += 8;
    //      }
    //
    //      lc -= nBits;
    //      return (c >> lc) & ((1 << nBits) - 1);
    //    }
    fn read_bits(count: i64, c: &mut i64, lc: &mut i64, mut read: impl Read) -> i64 {
        while *lc < count {
            *c = (*c << 8) | (read.read_u8() as i64);
            *lc += 8;
        }

        *lc -= count;

        (*c >> *lc) & ((1 << count) - 1)
    }


    // Build a "canonical" Huffman code table:
    //	- for each (uncompressed) symbol, hcode contains the length
    //	  of the corresponding code (in the compressed data)
    //	- canonical codes are computed and stored in hcode
    //	- the rules for constructing canonical codes are as follows:
    //	  * shorter codes (if filled with zeroes to the right)
    //	    have a numerically higher value than longer codes
    //	  * for codes with the same length, numerical values
    //	    increase with numerical symbol values
    //	- because the canonical code table can be constructed from
    //	  symbol lengths alone, the code table can be transmitted
    //	  without sending the actual code values
    //	- see http://www.compressconsult.com/huffman/

    // hufCanonicalCodeTable (Int64 hcode[HUF_ENCSIZE])
    fn canonical_table(h_code: &mut [i64]) {
        debug_assert_eq!(h_code.len(), ENCODE_SIZE);

        // Int64 n[59];
        // for (int i = 0; i <= 58; ++i)
        //    n[i] = 0;
        let mut n = [ 0_i64; 59 ];


        // For each i from 0 through 58, count the
        // number of different codes of length i, and
        // store the count in n[i].
        //
        //    for (int i = 0; i < HUF_ENCSIZE; ++i)
        //        n[hcode[i]] += 1;
        for &code in &h_code {
            n[code] += 1;
        }

        // For each i from 58 through 1, compute the
        // numerically lowest code with length i, and
        // store that code in n[i].
        //    Int64 c = 0;
        //
        //    for (int i = 58; i > 0; --i)
        //    {
        //        Int64 nc = ((c + n[i]) >> 1);
        //        n[i] = c;
        //        c = nc;
        //    }

        let mut c = 0_i64;
        for n in &mut n.iter_mut().rev() {
            let nc = (c + *n) >> 1;
            *n = c;
            c = nc;
        }

        // hcode[i] contains the length, l, of the
        // code for symbol i.  Assign the next available
        // code of length l to the symbol and store both
        // l and the code in hcode[i].
        //    for (int i = 0; i < HUF_ENCSIZE; ++i)
        //    {
        //        int l = hcode[i];
        //
        //        if (l > 0)
        //        hcode[i] = l | (n[l]++ << 6);
        //    }
        for &code in &h_code {
            let l = code;
            if l > 0 {
                *h_code = l | (n << 6);
                n[l] += 1;
            }
        }
    }


    // Compute Huffman codes (based on frq input) and store them in frq:
    //	- code structure is : [63:lsb - 6:msb] | [5-0: bit length];
    //	- max code length is 58 bits;
    //	- codes outside the range [im-iM] have a null length (unused values);
    //	- original frequencies are destroyed;
    //	- encoding tables are used by hufEncode() and hufBuildDecTable();
    //
    // NB: The following code "(*a == *b) && (a > b))" was added to ensure
    //     elements in the heap with the same value are sorted by index.
    //     This is to ensure, the STL make_heap()/pop_heap()/push_heap() methods
    //     produced a resultant sorted heap that is identical across OSes.

    //    struct FHeapCompare
    //    {
    //        bool operator () (Int64 *a, Int64 *b)
    //    {
    //    return ((*a > *b) || ((*a == *b) && (a > b)));
    //    }
    //    };
    fn compare_heap(a: &i64, b: &i64) -> bool {
        ((*a > *b) || ((*a == *b) && (a > b)))
    }


    //    hufBuildEncTable
    //        (Int64*	frq,	// io: input frequencies [HUF_ENCSIZE], output table
    //         int*	im,	//  o: min frq index
    //         int*	iM)	//  o: max frq index
    //    {
    fn build_encoding_table(
        frequencies: &mut [i64],  // input frequencies, output encoding table
    ) -> Range<i64> // return frequency max min range
    {
        debug_assert_eq!(frequencies.len(), ENCODE_SIZE);

        // This function assumes that when it is called, array frq
        // indicates the frequency of all possible symbols in the data
        // that are to be Huffman-encoded.  (frq[i] contains the number
        // of occurrences of symbol i in the data.)
        //
        // The loop below does three things:
        //
        // 1) Finds the minimum and maximum indices that point
        //    to non-zero entries in frq:
        //
        //     frq[im] != 0, and frq[i] == 0 for all i < im
        //     frq[iM] != 0, and frq[i] == 0 for all i > iM
        //
        // 2) Fills array fHeap with pointers to all non-zero
        //    entries in frq.
        //
        // 3) Initializes array hlink such that hlink[i] == i
        //    for all array entries.


        //    AutoArray <int, HUF_ENCSIZE> hlink;
        //    AutoArray <Int64 *, HUF_ENCSIZE> fHeap;
        let mut h_link = vec![0_i32; ENCODE_SIZE];
        let mut f_heap = vec![0_i64; ENCODE_SIZE];

        //    *im = 0;
        //
        //    while (!frq[*im])
        //        (*im)++;
        let min_frequency_index = {
            let mut index = 0;
            while ![frequencies[index]] { index += 1; }
            index
        };

        //
        //    int nf = 0;
        //
        //    for (int i = *im; i < HUF_ENCSIZE; i++)
        //    {
        //        hlink[i] = i;
        //
        //        if (frq[i])
        //        {
        //            fHeap[nf] = &frq[i];
        //            nf++;
        //            *iM = i;
        //        }
        //    }
        let mut nf = 0;
        let mut max_frequency_index = 0;

        for index in 0 .. ENCODE_SIZE {
            h_link[index] = index as i32;

            if frequencies[index] != 0 {
                f_heap[nf] = index as i64; // &frequencies[index];
                nf += 1;
                max_frequency_index = index;
            }
        }

        // Add a pseudo-symbol, with a frequency count of 1, to frq;
        // adjust the fHeap and hlink array accordingly.  Function
        // hufEncode() uses the pseudo-symbol for run-length encoding.

        //    (*iM)++;
        //    frq[*iM] = 1;
        //    fHeap[nf] = &frq[*iM];
        //    nf++;
        max_frequency_index += 1;
        frequencies[max_frequency_index] = 1;
        f_heap[nf] = max_frequency_index as i64; // &frequencies[max_frequency_index];
        nf += 1;

        // Build an array, scode, such that scode[i] contains the number
        // of bits assigned to symbol i.  Conceptually this is done by
        // constructing a tree whose leaves are the symbols with non-zero
        // frequency:
        //
        //     Make a heap that contains all symbols with a non-zero frequency,
        //     with the least frequent symbol on top.
        //
        //     Repeat until only one symbol is left on the heap:
        //
        //         Take the two least frequent symbols off the top of the heap.
        //         Create a new node that has first two nodes as children, and
        //         whose frequency is the sum of the frequencies of the first
        //         two nodes.  Put the new node back into the heap.
        //
        // The last node left on the heap is the root of the tree.  For each
        // leaf node, the distance between the root and the leaf is the length
        // of the code for the corresponding symbol.
        //
        // The loop below doesn't actually build the tree; instead we compute
        // the distances of the leaves from the root on the fly.  When a new
        // node is added to the heap, then that node's descendants are linked
        // into a single linear list that starts at the new node, and the code
        // lengths of the descendants (that is, their distance from the root
        // of the tree) are incremented by one.

        //    make_heap (&fHeap[0], &fHeap[nf], FHeapCompare());
        //
        //    AutoArray <Int64, HUF_ENCSIZE> scode;
        //    memset (scode, 0, sizeof (Int64) * HUF_ENCSIZE);
        make_heap(&f_heap[0], &f_heap[nf], compare_heap);
        let s_code = vec![ 0_i64; ENCODE_SIZE ];

        while nf > 1 {
            //    while (nf > 1)
            //    {
            //        //
            //        // Find the indices, mm and m, of the two smallest non-zero frq
            //        // values in fHeap, add the smallest frq to the second-smallest
            //        // frq, and remove the smallest frq value from fHeap.
            //        //
            //
            //        int mm = fHeap[0] - frq;
            //        pop_heap (&fHeap[0], &fHeap[nf], FHeapCompare());
            //        --nf;

            let mm = unimplemented!();

            //
            //        int m = fHeap[0] - frq;
            //        pop_heap (&fHeap[0], &fHeap[nf], FHeapCompare());
            //
            //        frq[m ] += frq[mm];
            //        push_heap (&fHeap[0], &fHeap[nf], FHeapCompare());
            //
            //        //
            //        // The entries in scode are linked into lists with the
            //        // entries in hlink serving as "next" pointers and with
            //        // the end of a list marked by hlink[j] == j.
            //        //
            //        // Traverse the lists that start at scode[m] and scode[mm].
            //        // For each element visited, increment the length of the
            //        // corresponding code by one bit. (If we visit scode[j]
            //        // during the traversal, then the code for symbol j becomes
            //        // one bit longer.)
            //        //
            //        // Merge the lists that start at scode[m] and scode[mm]
            //        // into a single list that starts at scode[m].
            //        //
            //
            //        //
            //        // Add a bit to all codes in the first list.
            //        //
            //
            //        for (int j = m; true; j = hlink[j])
            //        {
            //            scode[j]++;
            //
            //            assert (scode[j] <= 58);
            //
            //            if (hlink[j] == j)
            //            {
            //                //
            //                // Merge the two lists.
            //                //
            //
            //                hlink[j] = mm;
            //                break;
            //            }
            //        }
            //
            //        //
            //        // Add a bit to all codes in the second list
            //        //
            //
            //        for (int j = mm; true; j = hlink[j])
            //        {
            //            scode[j]++;
            //
            //            assert (scode[j] <= 58);
            //
            //            if (hlink[j] == j)
            //            break;
            //        }
            //    }

            //
            // Build a canonical Huffman code table, replacing the code
            // lengths in scode with (code, code length) pairs.  Copy the
            // code table from scode into frq.
            //

            //    hufCanonicalCodeTable (scode);
            //    memcpy (frq, scode, sizeof (Int64) * HUF_ENCSIZE);

        }

        (min_frequency_index, max_frequency_index)
    }
}





// https://github.com/AcademySoftwareFoundation/openexr/blob/8cd1b9210855fa4f6923c1b94df8a86166be19b1/OpenEXR/IlmImf/ImfWav.cpp
fn wave_2_decode(_buffer: &[u16], _x_size: u32, _x_offset: u32, _y_size: u32, _y_offset: u32, _max: u16 ) -> IoResult<()> {
    unimplemented!()
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

//    let mut k = 0;

    assert_eq!(U16_RANGE as u16 as i32, U16_RANGE);

    let mut table = Vec::with_capacity(U16_RANGE as usize);

    for index in 0 .. U16_RANGE as u16 {
        if index == 0 || (bitmap[index as usize >> 3] as usize & (1 << (index as usize & 7)) != 0) { // TODO where should be cast?
//            lut[k] = i;
//            k += 1;
            table.push(index);
        }
    }

    let n = table.len() as u16;
    assert_eq!(table.len() as u16 as usize, table.len());

    table.resize(U16_RANGE as usize, 0);

    (table, n)
}

fn apply_lookup_table(data: &mut [u16], table: &[u16]) {
//    for (int i = 0; i < nData; ++i)
//        data[i] = lut[data[i]];
    for data in data {
        *data = table[*data as usize];
    }
}


pub fn compress_bytes(_packed: Bytes<'_>) -> Result<ByteVec> {
    unimplemented!();
}


#[cfg(test)]
mod test {
    #[test]
    fn huffman(){

    }
}