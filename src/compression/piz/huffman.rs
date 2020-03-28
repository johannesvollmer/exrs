
//! 16-bit Huffman compression and decompression.
//! Huffman compression and decompression routines written
//!	by Christian Rouet for his PIZ image file format.
// see https://github.com/AcademySoftwareFoundation/openexr/blob/88246d991e0318c043e6f584f7493da08a31f9f8/OpenEXR/IlmImf/ImfHuf.cpp

use std::io::{Read, Write};
use crate::error::{UnitResult, Result};
use smallvec::alloc::collections::BinaryHeap;
use crate::math::RoundingMode;
use crate::io::Data;

// void
// hufUncompress (const char compressed[],
// 	       int nCompressed,
// 	       unsigned short raw[],
// 	       int nRaw)
// {
pub fn decompress(compressed: &[u8], result: &mut Vec<u16>) -> UnitResult {
//     //
//     // need at least 20 bytes for header
//     //
//     if (nCompressed < 20 )
//     {
// 	if (nRaw != 0)
// 	    notEnoughData();
//
// 	return;
//     }
    if compressed.len() < 20 /*&& !result.is_empty()*/ {
        panic!("invalid compressed huffman data size");
        // return Err(Error::invalid("invalid huffman data size"));
    }
//
//     int im = readUInt (compressed);
//     int iM = readUInt (compressed + 4);
//     // int tableLength = readUInt (compressed + 8);
//     int nBits = readUInt (compressed + 12);

    let mut remaining_bytes = compressed;

    let min_hcode_index = u32::read(&mut remaining_bytes)? as usize; // FIXME endianness???
    let max_hcode_index = u32::read(&mut remaining_bytes)? as usize;
    let _skip = u32::read(&mut remaining_bytes)? as usize;
    let bit_count = u32::read(&mut remaining_bytes)? as usize;

    inspect!(min_hcode_index, max_hcode_index, bit_count);
//
//     if (im < 0 || im >= HUF_ENCSIZE || iM < 0 || iM >= HUF_ENCSIZE)
// 	invalidTableSize();
    if /*min_hcode_index < 0 ||*/ min_hcode_index >= ENCODING_TABLE_SIZE || /*max_hcode_index < 0 ||*/ max_hcode_index >= ENCODING_TABLE_SIZE {
        panic!();
        // return Err(Error::invalid("huffman table size"));
    }

//     TODO
//     const char *ptr = compressed + 20;

    let _skip = u32::read(&mut remaining_bytes)?;

//
//     if ( ptr + (nBits+7 )/8 > compressed+nCompressed)
//     {
//         notEnoughData();
//         return;
//     }
    if compressed.len() < RoundingMode::Up.divide(bit_count, 8) {
        panic!();
        // return Err(Error::invalid("huffman data size"));
    }

//     // Fast decoder needs at least 2x64-bits of compressed data, and
//     // needs to be run-able on this platform. Otherwise, fall back
//     // to the original decoder
//
//     if (FastHufDecoder::enabled() && nBits > 128) { // TODO
//         FastHufDecoder fhd (ptr, nCompressed - (ptr - compressed), im, iM, iM);
//         fhd.decode ((unsigned char*)ptr, nBits, raw, nRaw);
//     }
//     else {
//         AutoArray <Int64, HUF_ENCSIZE> freq;
//         AutoArray <HufDec, HUF_DECSIZE> hdec;
//         hufClearDecTable (hdec);
//
//         hufUnpackEncTable (&ptr,
//                            nCompressed - (ptr - compressed),
//                            im,
//                            iM,
//                            freq
//          );

    let frequencies = read_encoding_table(&mut remaining_bytes, min_hcode_index, max_hcode_index)?;

//
//         try {
//             if (nBits > 8 * (nCompressed - (ptr - compressed)))
//                 invalidNBits();
//
//             hufBuildDecTable (freq, im, iM, hdec);
//             hufDecode (freq, hdec, ptr, nBits, iM, nRaw, raw);
//         }
//         catch (...) {
//             hufFreeDecTable (hdec);
//             throw;
//         }
//
//         hufFreeDecTable (hdec);
    if bit_count > 8 * remaining_bytes.len() {
        panic!();
        // return Err(Error::invalid("bit count"))
    }

    let h_decode = build_decoding_table(&frequencies, min_hcode_index, max_hcode_index)?;
    debug_assert_eq!(h_decode.len(), DECODING_TABLE_SIZE);

    // TODO without copy!!
    let decoded = decode(
        &frequencies, &h_decode, remaining_bytes,
        bit_count, max_hcode_index, result.len()
    )?;

    result.copy_from_slice(&decoded);

//     }
// }

    Ok(())
}

pub fn _compress(_uncompressed: &[u16], _result: &mut [u8]) -> UnitResult {
    unimplemented!()
}



const ENCODE_BITS: usize = 16;			// literal (value) bit length
const DECODE_BITS: usize = 14;			// decoding bit size (>= 8)

const ENCODING_TABLE_SIZE: usize = (1 << ENCODE_BITS) + 1;
const DECODING_TABLE_SIZE: usize =  1 << DECODE_BITS;
const DECODE_MASK: usize = DECODING_TABLE_SIZE - 1;

const SHORT_ZEROCODE_RUN: i64 = 59;
const LONG_ZEROCODE_RUN: i64  = 63;
const SHORTEST_LONG_RUN: i64  = 2 + LONG_ZEROCODE_RUN - SHORT_ZEROCODE_RUN;
const _LONGEST_LONG_RUN: i64   = 255 + SHORTEST_LONG_RUN;

//    struct HufDec
//    {				// short code		long code
//    //-------------------------------
//    int		len:8;		// code length		0
//    int		lit:24;		// lit			p size
//    int	*	p;		// 0			lits
//    };


// #[derive(Default, Clone, Debug, Eq, PartialEq)]
// struct Code { // TODO use enum
//     short_code_len: i8,             // short: code length   | long: 0
//     short_code_lit: i32,            // short: lit           | long: p size TODO make this a u16???
//
//     long_code_lits: Vec<u16>,       // short: [],           | long: lits
// }


// FIXME
#[derive(Clone, Debug, Eq, PartialEq)]
// TODO repr(packed)?
enum Code {
    Short (ShortCode),
    Long (Vec<u16>),
    Empty
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShortCode {
    value: i32,
    len: u8
}


// void
// hufDecode
//     (const Int64 * 	hcode,	// i : encoding table
//      const HufDec * 	hdecod,	// i : decoding table
//      const char* 	in,	// i : compressed input buffer
//      int		ni,	// i : input size (in bits)
//      int		rlc,	// i : run-length code
//      int		no,	// i : expected output size (in bytes)
//      unsigned short*	out)	//  o: uncompressed output buffer
// {
fn decode(
    encoding_table: &[i64],
    decoding_table: &[Code],
    input: &[u8],
    input_bit_count: usize,
    run_length_code: usize,
    expected_ouput_size: usize,
) -> Result<Vec<u16>> {
    let mut output = Vec::with_capacity(expected_ouput_size);

//      Int64 c = 0;
//     int lc = 0;
//     unsigned short * outb = out;
//     unsigned short * oe = out + no;
//     const char * ie = in + (ni + 7) / 8; // input byte size
    let mut code_bits = 0_i64;
    let mut code_bit_count = 0_i64;

    let mut read = input;
    debug_assert_eq!(input.len(), RoundingMode::Up.divide(input_bit_count, 8) /*(input_bit_count + 7) / 8*/);

//     // Loop on input bytes
//
//     while (in < ie)
//     {
    while !read.is_empty() {

// 	getChar (c, lc, in);
        read_byte(&mut code_bits, &mut code_bit_count, &mut read)?;

//
// 	// Access decoding table
// 	while (lc >= HUF_DECBITS)
// 	{
        while code_bit_count >= DECODE_BITS as i64 {

// 	    const HufDec pl = hdecod[(c >> (lc-HUF_DECBITS)) & HUF_DECMASK];
            let pl_index = ((code_bits >> (code_bit_count - DECODE_BITS as i64)) & DECODE_MASK as i64) as usize;
            let pl = &decoding_table[pl_index];

// 	    if (pl.len){
// 		// Get short code
//
// 		lc -= pl.len;
// 		getCode (pl.lit, rlc, c, lc, in, out, outb, oe);
// 	    }


            if let Code::Short(code) = pl {
                code_bit_count -= code.len as i64;
                inspect!(pl_index, code, run_length_code, code_bits, code_bit_count, output);
                read_code_into_vec(code.value as u16, run_length_code, &mut code_bits, &mut code_bit_count, &mut read, &mut output, expected_ouput_size)?;
            }
            /*if pl.short_code_len != 0 { // this is a short code
                lc -= pl.short_code_len as i64;
                inspect!(pl_index, pl, run_length_code, c, lc, output);
                read_code(pl.short_code_lit as u16*//*TODO*//*, run_length_code, &mut c, &mut lc, &mut read, &mut output, expected_ouput_size)?;
            }*/

// 	    else
// 	    {
// 		if (!pl.p)
// 		    invalidCode(); // wrong code
//
            else if let Code::Long(ref long_code) = pl {
                /*if pl.lits.is_empty() {
                    panic!();
                    // return Err(Error::invalid("huffman code"));
                }*/

// 		// Search long code
//
// 		int j;
//
// 		for (j = 0; j < pl.lit; j++)
// 		{

                let mut code_search_index = 0;

                inspect!(pl_index, pl);
                debug_assert_ne!(long_code.len(), 0);

                // TODO pl.lits.iter().find(|lit| ...).ok_or(Err())
                while code_search_index < long_code.len() {


// 		    int	l = hufLength (hcode[pl.p[j]]);
//                     debug_assert!(j > 0);

                    let plpj = long_code[code_search_index];
                    let hcode_plpj = encoding_table[plpj as usize];
                    let length = length(hcode_plpj);
//
// 		    while (lc < l && in < ie)	// get more bits
// 			getChar (c, lc, in);
                    while code_bit_count < length && !read.is_empty() {
                        read_byte(&mut code_bits, &mut code_bit_count, &mut read)?;
                    }

// 		    if (lc >= l)
// 		    {

// 			if (hufCode (hcode[pl.p[j]]) ==
// 				((c >> (lc - l)) & ((Int64(1) << l) - 1)))
// 			{
// 			    // Found : get long code
// 			    lc -= l;
// 			    getCode (pl.p[j], rlc, c, lc, in, out, outb, oe);
// 			    break;
// 			}
// 		    }
                    inspect!(code_bit_count, length, code(hcode_plpj), (code_bits >> (code_bit_count - length)) & ((1 << length) - 1));
                    if code_bit_count >= length && code(hcode_plpj) == ((code_bits >> (code_bit_count - length)) & ((1 << length) - 1)) {
                        println!("found long code");

                        code_bit_count -= length;
                        read_code_into_vec(plpj, run_length_code, &mut code_bits, &mut code_bit_count, &mut read, &mut output, expected_ouput_size)?;
                        break;
                    }

                    code_search_index += 1;
                }

                if code_search_index == long_code.len() { // loop ran through without finding the code
                    inspect!(long_code);
                    panic!("could not find long code");
                    // return Err(Error::invalid("huffman code"))
                }
// 		}
            }
            else {
                panic!("code is none");
            }
//
// 		if (j == pl.lit)
// 		    invalidCode(); // Not found
// 	    }
// 	}
//     }


        }

    }

//
//     //
//     // Get remaining (short) codes
//     //
//
//     int i = (8 - ni) & 7;
//     c >>= i;
//     lc -= i;
    let count = (8 - input.len()) & 7;
    code_bits >>= count as i64;
    code_bit_count -= count as i64;

//
//     while (lc > 0)
//     {
// 	const HufDec pl = hdecod[(c << (HUF_DECBITS - lc)) & HUF_DECMASK];
//
// 	if (pl.len)
// 	{
// 	    lc -= pl.len;
// 	    getCode (pl.lit, rlc, c, lc, in, out, outb, oe);
// 	}
// 	else
// 	{
// 	    invalidCode(); // wrong (long) code
// 	}
//     }
    while code_bit_count > 0 {
        let code = &decoding_table[((code_bits << (DECODE_BITS as i64 - code_bit_count)) & DECODE_MASK as i64) as usize];

        if let Code::Short(code) = code {
            code_bit_count -= code.len as i64;
            read_code_into_vec(code.value as u16, run_length_code, &mut code_bits, &mut code_bit_count, &mut read, &mut output, expected_ouput_size)?;
        }
        // if pl.short_code_len != 0 {
        //     lc -= pl.short_code_len as i64;
        //     read_code(pl.short_code_lit as u16, run_length_code, &mut c, &mut lc, &mut read, &mut output, expected_ouput_size)?;
        // }
        else {
            panic!();
            // return Err(Error::invalid("huffman code"))
        }
    }

//
//     if (out - outb != no)
// 	notEnoughData ();
    if output.len() != expected_ouput_size {
        panic!();
        // return Err(Error::invalid("huffman data length"))
    }

    Ok(output)
}

// // Build a decoding hash table based on the encoding table hcode:
// //	- short codes (<= HUF_DECBITS) are resolved with a single table access;
// //	- long code entry allocations are not optimized, because long codes are
// //	  unfrequent;
// //	- decoding tables are used by hufDecode();
// void
// hufBuildDecTable
//     (const Int64*	hcode,		// i : encoding table
//      int		im,		// i : min index in hcode
//      int		iM,		// i : max index in hcode
//      HufDec *		hdecod)		//  o: (allocated by caller)
//      					//     decoding table [HUF_DECSIZE]
// {
fn build_decoding_table(encoding_table: &[i64], min_hcode_index: usize, max_hcode_index: usize) -> Result<Vec<Code>> { // TODO use slices instead of slice+min/max
    let mut decoding_table = vec![Code::Empty; DECODING_TABLE_SIZE]; // not an array because of code not being copy

    for code_index in min_hcode_index ..= max_hcode_index {
        let hcode = encoding_table[code_index];

//     // Init hashtable & loop on all codes.
//     // Assumes that hufClearDecTable(hdecod) has already been called.
//     for (; im <= iM; im++)
//     {
// 	Int64 c = hufCode (hcode[im]);
// 	int l = hufLength (hcode[im]);
        let code = code(hcode);
        let length = length(hcode);
//
// 	if (c >> l)
// 	{
// 	    //
// 	    // Error: c is supposed to be an l-bit code,
// 	    // but c contains a value that is greater
// 	    // than the largest l-bit number.
// 	    //
//
// 	    invalidTableEntry();
// 	}
        if (code >> length) != 0 {
            panic!();
            // return Err(Error::invalid("huffman table entry"));
        }
//
// 	if (l > HUF_DECBITS)
// 	{
// 	    //
// 	    // Long code: add a secondary entry
// 	    //
        if length > DECODE_BITS as i64 {

//
// 	    HufDec *pl = hdecod + (c >> (l - HUF_DECBITS));
            let code = &mut decoding_table[(code >> (length - DECODE_BITS as i64)) as usize];
//
// 	    if (pl->len)
// 	    {
// 		//
// 		// Error: a short code has already
// 		// been stored in table entry *pl.
// 		//
//
// 		invalidTableEntry();
// 	    }

//
// 	    pl->lit++;
//
// 	    if (pl->p)
// 	    {
// 		    int *p = pl->p;
// 		    pl->p = new int [pl->lit];
//
// 		    for (int i = 0; i < pl->lit - 1; ++i)
// 		        pl->p[i] = p[i];
//
// 		    delete [] p;
// 	    }
// 	    else
// 	    {
// 		    pl->p = new int [1];
// 	    }
// 	    pl->p[pl->lit - 1]= im;
// 	}
            match code {
                Code::Empty => *code = Code::Long(vec![code_index as u16]),
                Code::Long(lits) => lits.push(code_index as u16),
                _ => {
                    panic!("expected non short code");
                    // return Err(Error::invalid("huffman table entry"));
                }
            }
        }
        else if length != 0 {
// 	else if (l)
// 	{
// 	    // Short code: init all primary entries
// 	    HufDec *pl = hdecod + (c << (HUF_DECBITS - l));
//
// 	    for (Int64 i = 1 << (HUF_DECBITS - l); i > 0; i--, pl++)
// 	    {
// 		    if (pl->len || pl->p) {
// 		        // Error: a short code or a long code has
// 		        // already been stored in table entry *pl.
// 		        invalidTableEntry();
// 		    }

// 		    pl->len = l;
// 		    pl->lit = im;

            debug_assert!(length >= 0, "ShortCode.len must be signed???");

            let default_value = Code::Short(ShortCode {

                // short_code_len: l as i8, // TODO wrap or not wrap?
                // short_code_lit: im as i32,
                // long_code_lits: Vec::new()
                value: code_index as i32,
                len: length as u8 // TODO wrap or not wrap? signed or not?
            });

            // inspect!(default_value);

            let start_index = (code << (DECODE_BITS as i64 - length)) as usize;
            let count = 1 << (DECODE_BITS as i64 - length);

            for value in &mut decoding_table[start_index .. start_index + count] {
                // assert!(value.long_code_lits.is_empty() && value.short_code_len == 0);

                *value = default_value.clone();
            }

// 	    }
// 	}
//     }
// }

        }
    }

    Ok(decoding_table)
}



// void
// hufUnpackEncTable
//     (const char**	pcode,		// io: ptr to packed table (updated)
//      int		ni,		// i : input size (in bytes)
//      int		im,		// i : min hcode index
//      int		iM,		// i : max hcode index
//      Int64*		hcode)		//  o: encoding table [HUF_ENCSIZE]

/// run-length-decompresses all zero runs from the packed table to the encoding table
fn read_encoding_table(packed: &mut &[u8], mut min_hcode_index: usize, max_hcode_index: usize) -> Result<[i64; ENCODING_TABLE_SIZE]> {
    let mut encoding_table = [0_i64; ENCODING_TABLE_SIZE];

//     const char *p = *pcode;
//     Int64 c = 0;
//     int lc = 0;
    let mut remaining_bytes = *packed;
    let mut code_bits = 0_i64;
    let mut code_bit_count = 0_i64;

//
//     for (; im <= iM; im++)
//     {
    // for code_index in min_hcode_index ..= max_hcode_index {
    while min_hcode_index <= max_hcode_index {

// 	        if (p - *pcode > ni)
// 	            unexpectedEndOfTable();
        if remaining_bytes.len() < 1 { // TODO we do not need these errors as `read` handles those for us
            panic!();
            // return Err(Error::invalid("huffman table length"));
        }
//
// 	        Int64 l = hcode[im] = getBits (6, c, lc, p); // code length
        let code_len = read_bits(6, &mut code_bits, &mut code_bit_count, &mut remaining_bytes);
        encoding_table[min_hcode_index] = code_len;

//
// 	        if (l == (Int64) LONG_ZEROCODE_RUN)
// 	        {
        if code_len == LONG_ZEROCODE_RUN {
// 	            if (p - *pcode > ni)
// 		        unexpectedEndOfTable();
            if remaining_bytes.len() < 1 {
                panic!();
                // return Err(Error::invalid("huffman table length"));
            }
//
// 	            int zerun = getBits (8, c, lc, p) + SHORTEST_LONG_RUN;
            let zerun = read_bits(8, &mut code_bits, &mut code_bit_count, &mut remaining_bytes) + SHORTEST_LONG_RUN;
//
// 	            if (im + zerun > iM + 1) // TODO open new issue in openexr for negative length?
// 		            tableTooLong();
            if zerun < 0 || min_hcode_index as i64 + zerun > max_hcode_index as i64 + 1 {
                panic!();
                // return Err(Error::invalid("huffman table length"));
            }
//
// 	            while (zerun--)
// 		            hcode[im++] = 0;
//
// 	            im--;
            for value in &mut encoding_table[min_hcode_index .. min_hcode_index + zerun as usize] {
                *value = 0;
            }

            min_hcode_index += zerun as usize; // TODO + or - 1

// 	        }
        }
// 	        else if (l >= (Int64) SHORT_ZEROCODE_RUN)
// 	        {
        else if code_len >= SHORT_ZEROCODE_RUN {
// 	            int zerun = l - SHORT_ZEROCODE_RUN + 2;
//
// 	            if (im + zerun > iM + 1)
// 		            tableTooLong();
//
// 	            while (zerun--)
// 		            hcode[im++] = 0;
//
// 	            im--;
// 	        }

            let zerun = code_len - SHORT_ZEROCODE_RUN + 2;
            if zerun < 0 || min_hcode_index as i64 + zerun > max_hcode_index as i64 + 1 {
                panic!();
                // return Err(Error::invalid("huffman table length"));
            }

            for value in &mut encoding_table[min_hcode_index .. min_hcode_index + zerun as usize] {
                *value = 0;
            }

            min_hcode_index += zerun as usize; // TODO + or - 1
//     }
        }
        else {
            min_hcode_index += 1;
        }
    }
//
//     *pcode = const_cast<char *>(p);
    *packed = remaining_bytes;
//
//     hufCanonicalCodeTable (hcode);
    build_canonical_table(&mut encoding_table);

    Ok(encoding_table)
}


//    inline Int64
//    hufLength (Int64 code) code & 63;
fn length(code: i64) -> i64 { code & 63 }

//    inline Int64
//    hufCode (Int64 code) code >> 6;
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
fn _write_bits(count: i64, bits: i64, c: &mut i64, lc: &mut i64, mut out: impl Write) {
    *c = *c << count;
    *lc += count;

    *c = *c | bits;

    while *lc >= 8 {
        *lc -= 8;
        out.write(&[ (*c >> *lc) as u8 ]).expect("bit write err"); // TODO make sure never or always wraps?
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
// TODO replace those functions with a `Reader` struct that remembers all the parameters??
#[inline]
fn read_bits(count: i64, code_bits: &mut i64, code_bit_count: &mut i64, read: &mut impl Read) -> i64 {
    while *code_bit_count < count {
        read_byte(code_bits, code_bit_count, read).unwrap(); // TODO unwrap?
    }

    *code_bit_count -= count;
    (*code_bits >> *code_bit_count) & ((1 << count) - 1)
}

// getChar(c, lc, in)			\
// {						\
// c = (c << 8) | *(unsigned char *)(in++);	\
// lc += 8;					\
// }
#[inline]
fn read_byte(code_bits: &mut i64, bit_count: &mut i64, input: &mut impl Read) -> UnitResult {
    *code_bits = (*code_bits << 8) | u8::read(input)? as i64;
    *bit_count += 8;

    Ok(())
}

// #define getCode(po, rlc, c, lc, in, out, ob, oe)\
// {						\
// if (po == rlc)				\
// {						\
// if (lc < 8)				\
// getChar(c, lc, in);			\
// \
// lc -= 8;				\
// \
// unsigned char cs = (c >> lc);		\
// \
// if (out + cs > oe)			\
// tooMuchData();			\
// else if (out - 1 < ob)			\
// notEnoughData();			\
// \
// unsigned short s = out[-1];		\
// \
// while (cs-- > 0)			\
// *out++ = s;				\
// }						\
// else if (out < oe)				\
// {						\
// *out++ = po;				\
// }						\
// else					\
// {						\
// tooMuchData();				\
// }						\
// }
#[inline]
// pl.lit, run_length_code, c, lc, read, out
fn read_code_into_vec(lits: u16, run_length_code: usize, c: &mut i64, lc: &mut i64, read: &mut &[u8], out: &mut Vec<u16>, max_len: usize) -> UnitResult {
    if lits as usize == run_length_code {
        if *lc < 8 {
            read_byte(c, lc, read)?;
        }

        *lc -= 8;

        let mut cs = *c >> *lc;
        if out.len() as i64 + cs > max_len as i64 {
            panic!("more data than expected");
            // return Err(Error::invalid("huffman data size"));
        }
        else if out.is_empty() {
            panic!("cannot get last value because none were written yet");
            // return Err(Error::invalid("huffman data size"));
        }

        let s = *out.last().unwrap();
        while cs > 0 {
            println!("repeating code {}", s);
            out.push(s);
            cs -= 1;
        }
    }
    else if out.len() < max_len  {
        println!("inserting code {}", lits);
        out.push(lits);
    }
    else {
        panic!();
        // return Err(Error::invalid("huffman data size"));
    }

    Ok(())
}

fn _count_frequencies(frequencies: &mut[i64], data: &[u16]) {
    // for (int i = 0; i < HUF_ENCSIZE; ++i)
    // 	freq[i] = 0;
    //
    //     for (int i = 0; i < n; ++i)
    // 	++freq[data[i]];

    0_i64.fill_slice(frequencies);

    for value in data {
        frequencies[*value as usize] += 1;
    }
}

// unsigned int
// readUInt (const char buf[4])
// {
//     const unsigned char *b = (const unsigned char *) buf;
//
//     return ( b[0]        & 0x000000ff) |
// 	   ((b[1] <<  8) & 0x0000ff00) |
// 	   ((b[2] << 16) & 0x00ff0000) |
// 	   ((b[3] << 24) & 0xff000000);
// }
//
// } // namespace

// TODO
// fn read_u32(read: impl Read) -> IoResult<u32> {
//     u32::read_from_native_endian(read)
// }




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
fn build_canonical_table(h_code: &mut [i64]) {
    debug_assert_eq!(h_code.len(), ENCODING_TABLE_SIZE);

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
    for &code in h_code.iter() {
        n[code as usize] += 1;
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
    for code_i in h_code.iter_mut() {
        let l = *code_i;
        if l > 0 {
            *code_i = l | (n[l as usize] << 6);
            n[l as usize] += 1;
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
/*fn compare_heap(a: &i64, b: &i64) -> bool {
    (*a > *b) || ((*a == *b) && (a > b))
}*/


//    hufBuildEncTable
//        (Int64*	frq,	// io: input frequencies [HUF_ENCSIZE], output table
//         int*	im,	//  o: min frq index
//         int*	iM)	//  o: max frq index
//    {
pub fn _build_encoding_table(
    frequencies: &mut [i64],  // input frequencies, output encoding table
) -> (usize, usize) // return frequency max min range
{
    debug_assert_eq!(frequencies.len(), ENCODING_TABLE_SIZE);

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
    let mut h_link = [0_usize; ENCODING_TABLE_SIZE];
    let mut f_heap = [0_usize; ENCODING_TABLE_SIZE];

    //    *im = 0;
    //
    //    while (!frq[*im])
    //        (*im)++;
    let min_frequency_index = {
        let mut index = 0;
        while frequencies[index] != 0 { index += 1; }
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

    for index in 0 ..ENCODING_TABLE_SIZE {
        h_link[index] = index;

        if frequencies[index] != 0 {
            f_heap[nf] = index; // &frequencies[index];
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
    f_heap[nf] = max_frequency_index; // &frequencies[max_frequency_index];
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
    let mut heap = BinaryHeap::from(f_heap.to_vec()); // TODO do not create vec in the first place?

    //    AutoArray <Int64, HUF_ENCSIZE> scode;
    //    memset (scode, 0, sizeof (Int64) * HUF_ENCSIZE);
    let mut s_code = [0_i64; ENCODING_TABLE_SIZE];

    //    while (nf > 1)
    //    {
    while nf > 1 {

        // Find the indices, mm and m, of the two smallest non-zero frq
        // values in fHeap, add the smallest frq to the second-smallest
        // frq, and remove the smallest frq value from fHeap.
        //
        //        int mm = fHeap[0] - frq;
        //        pop_heap (&fHeap[0], &fHeap[nf], FHeapCompare());
        //        --nf;
        let mm = heap.pop().expect("cannot pop heap bug");
        nf -= 1;

        //        int m = fHeap[0] - frq;
        //        pop_heap (&fHeap[0], &fHeap[nf], FHeapCompare());
        let m = heap.pop().expect("cannot pop heap bug");

        //        frq[m ] += frq[mm];
        //        push_heap (&fHeap[0], &fHeap[nf], FHeapCompare());
        frequencies[m] += frequencies[mm];
        heap.push(m); // m?????

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
        //
        //        // Add a bit to all codes in the first list.

        //        for (int j = m; true; j = hlink[j]) {
        //            scode[j]++;
        //            assert (scode[j] <= 58);
        //
        //            if (hlink[j] == j) {
        //                // Merge the two lists.
        //
        //                hlink[j] = mm;
        //                break;
        //            }
        //        }
        let mut j = m;
        loop {
            s_code[j] += 1;
            assert!(s_code[j] <= 58);

            if h_link[j] == j {
                // merge the two lists
                h_link[j] = mm;
                break;
            }

            j = h_link[j];
        }

        //
        //        // Add a bit to all codes in the second list
        //        for (int j = mm; true; j = hlink[j]) {
        //            scode[j]++;
        //            assert (scode[j] <= 58);
        //
        //            if (hlink[j] == j)
        //              break;
        //        }
        //    }
        let mut j = mm;
        loop {
            s_code[j] += 1;
            assert!(s_code[j] <= 58);

            if h_link[j] == j {
                // merge the two lists
                h_link[j] = mm;
                break;
            }

            j = h_link[j];
        }

        // Build a canonical Huffman code table, replacing the code
        // lengths in scode with (code, code length) pairs.  Copy the
        // code table from scode into frq.

        //    hufCanonicalCodeTable (scode);
        //    memcpy (frq, scode, sizeof (Int64) * HUF_ENCSIZE);

        debug_assert_eq!(s_code.len(), ENCODING_TABLE_SIZE);
        debug_assert_eq!(frequencies.len(), ENCODING_TABLE_SIZE);

        build_canonical_table(&mut s_code);
        frequencies.copy_from_slice(&s_code);
    }

    (min_frequency_index, max_frequency_index)
}