//! 16-bit Huffman compression and decompression.
//! Huffman compression and decompression routines written
//!	by Christian Rouet for his PIZ image file format.
// see https://github.com/AcademySoftwareFoundation/openexr/blob/88246d991e0318c043e6f584f7493da08a31f9f8/OpenEXR/IlmImf/ImfHuf.cpp

use crate::error::{Error, Result, UnitResult};
use crate::io::Data;
use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    io::{Cursor, Read, Write},
};

const INVALID_BIT_COUNT: &'static str =
    "Error in header for Huffman-encoded data (invalid number of bits).";
const INVALID_TABLE_ENTRY: &'static str =
    "Error in header for Huffman-encoded data (invalid code table entry).";
const NOT_ENOUGH_DATA: &'static str =
    "Error in Huffman-encoded data (decoded data are shorter than expected).";
const INVALID_TABLE_SIZE: &'static str =
    "Error in Huffman-encoded data (unexpected end of code table data).";
const TABLE_TOO_LONG: &'static str =
    "Error in Huffman-encoded data (code table is longer than expected).";
const INVALID_CODE: &'static str = "Error in Huffman-encoded data (invalid code).";
const TOO_MUCH_DATA: &'static str =
    "Error in Huffman-encoded data (decoded data are longer than expected).";

const ENCODE_BITS: usize = 16; // literal (value) bit length
const DECODE_BITS: usize = 14; // decoding bit size (>= 8)

const ENCODING_TABLE_SIZE: usize = (1 << ENCODE_BITS) + 1;
const DECODING_TABLE_SIZE: usize = 1 << DECODE_BITS;
const DECODE_MASK: usize = DECODING_TABLE_SIZE - 1;

const SHORT_ZEROCODE_RUN: i64 = 59;
const LONG_ZEROCODE_RUN: i64 = 63;
const SHORTEST_LONG_RUN: i64 = 2 + LONG_ZEROCODE_RUN - SHORT_ZEROCODE_RUN;
const LONGEST_LONG_RUN: i64 = 255 + SHORTEST_LONG_RUN;

trait MemoryStreamLength {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl MemoryStreamLength for Cursor<&[u8]> {
    /// Special length, which returns the number of bytes left in the stream.
    #[inline]
    fn len(&self) -> usize {
        let size = self.get_ref().len();
        size - self.position() as usize
    }
}

pub fn decompress(compressed: &[u8], expected_size: usize) -> Result<Vec<u16>> {
    if compressed.len() < 20 && expected_size > 0 {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    let mut mem_stream = Cursor::new(compressed);

    let min_hcode_index = u32::read(&mut mem_stream)? as usize;
    let max_hcode_index = u32::read(&mut mem_stream)? as usize;

    let _ = u32::read(&mut mem_stream)? as usize; // Table size
    let bit_count = u32::read(&mut mem_stream)? as usize;

    if min_hcode_index >= ENCODING_TABLE_SIZE || max_hcode_index >= ENCODING_TABLE_SIZE {
        return Err(Error::invalid(INVALID_TABLE_SIZE));
    }

    let packed_data = mem_stream.get_ref(); // Get reference to underlying data

    if packed_data[20] as usize + ((bit_count + 7) / 8)
        > packed_data[0] as usize + packed_data.len()
    {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    mem_stream.set_position(20);

    //let packed_data = &mem_stream.get_ref()[20..]; // After the header
    let encoding_table = read_encoding_table(&mut mem_stream, min_hcode_index, max_hcode_index)?;
    if bit_count > 8 * mem_stream.len() {
        return Err(Error::invalid(INVALID_BIT_COUNT));
    }
    let decoding_table = build_decoding_table(&encoding_table, min_hcode_index, max_hcode_index)?;

    let packed_data = &mem_stream.get_ref();
    let remaining_bytes = &packed_data[packed_data.len() - mem_stream.len()..];
    let result = decode(
        &encoding_table,
        &decoding_table,
        &remaining_bytes,
        bit_count,
        max_hcode_index,
        expected_size,
    )?;

    Ok(result)
}

pub fn compress(uncompressed: &[u16]) -> Result<Vec<u8>> {
    if uncompressed.is_empty() {
        return Ok(vec![]);
    }
    let calculated_length = 3 * uncompressed.len() + 4 * 65536;
    let mut result = vec![0_u8; calculated_length];

    let mut frequencies = vec![0_i64; ENCODING_TABLE_SIZE];

    count_frequencies(&mut frequencies, uncompressed);

    let (min_hcode_index, max_hcode_index) = build_encoding_table(&mut frequencies);

    let table_length = pack_encoding_table(
        &frequencies,
        min_hcode_index,
        max_hcode_index,
        &mut result[20..],
    )?;
    let encode_start = table_length + 20; // We need to add the initial offset

    let n_bits = encode(
        &frequencies,
        uncompressed,
        max_hcode_index,
        &mut result[encode_start..],
    )?;
    let data_length = (n_bits + 7) / 8;

    let mut buffer = std::io::Cursor::new(result);
    buffer.set_position(0);

    (min_hcode_index as u32).write(&mut buffer)?;
    (max_hcode_index as u32).write(&mut buffer)?;
    (table_length as u32).write(&mut buffer)?;
    n_bits.write(&mut buffer)?;
    0_u32.write(&mut buffer)?;

    let mut result = buffer.into_inner();
    let final_size = table_length + data_length as usize + 20;

    result.resize(final_size, 0);
    Ok(result)
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Code {
    Short(ShortCode),
    Long(Vec<u16>),
    Empty,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShortCode {
    value: i32,
    len: u8,
}

/// Decode (uncompress) n bits based on encoding & decoding tables:
fn decode(
    encoding_table: &[i64],
    decoding_table: &[Code],
    mut input: &[u8],
    input_bit_count: usize,
    run_length_code: usize,
    expected_ouput_size: usize,
) -> Result<Vec<u16>> {
    let mut output = Vec::with_capacity(expected_ouput_size);
    let mut code_bits = 0_i64;
    let mut code_bit_count = 0_i64;

    while input.len() > 0 {
        read_byte(&mut code_bits, &mut code_bit_count, &mut input)?;

        // Access decoding table
        while code_bit_count >= DECODE_BITS as i64 {
            let pl_index = ((code_bits >> (code_bit_count - DECODE_BITS as i64))
                & DECODE_MASK as i64) as usize;
            let pl = &decoding_table[pl_index];

            //
            // Get short code
            //
            if let Code::Short(code) = pl {
                code_bit_count -= code.len as i64;

                read_code_into_vec(
                    code.value as u16,
                    run_length_code,
                    &mut code_bits,
                    &mut code_bit_count,
                    &mut input,
                    &mut output,
                    expected_ouput_size,
                )?;
            } else if let Code::Long(ref long_codes) = pl {
                let mut code_search_index = 0;

                debug_assert_ne!(long_codes.len(), 0);

                while code_search_index < long_codes.len() {
                    let long_code = long_codes[code_search_index];
                    let encoded_long_code = encoding_table[long_code as usize];
                    let length = length(encoded_long_code);

                    while code_bit_count < length && input.len() > 0 {
                        read_byte(&mut code_bits, &mut code_bit_count, &mut input)?;
                    }

                    if code_bit_count >= length {
                        let required_code =
                            (code_bits >> (code_bit_count - length)) & ((1 << length) - 1);

                        if code(encoded_long_code) == required_code {
                            code_bit_count -= length;
                            read_code_into_vec(
                                long_code,
                                run_length_code,
                                &mut code_bits,
                                &mut code_bit_count,
                                &mut input,
                                &mut output,
                                expected_ouput_size,
                            )?;
                            break;
                        }
                    }

                    code_search_index += 1;
                }

                if code_search_index == long_codes.len() {
                    // loop ran through without finding the code
                    return Err(Error::invalid(INVALID_CODE));
                }
            } else {
                return Err(Error::invalid(INVALID_CODE));
            }
        }
    }

    let count = (8 - input_bit_count as i32) & 7;
    code_bits >>= count as i64;
    code_bit_count -= count as i64;

    while code_bit_count > 0 {
        let code = &decoding_table
            [((code_bits << (DECODE_BITS as i64 - code_bit_count)) & DECODE_MASK as i64) as usize];

        if let Code::Short(short_code) = code {
            code_bit_count -= short_code.len as i64;

            read_code_into_vec(
                short_code.value as u16,
                run_length_code,
                &mut code_bits,
                &mut code_bit_count,
                &mut input,
                &mut output,
                expected_ouput_size,
            )?;
        } else {
            return Err(Error::invalid(INVALID_CODE));
        }
    }

    if output.len() != expected_ouput_size {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    Ok(output)
}

/// Build a decoding hash table based on the encoding table hcode:
///	- short codes (<= HUF_DECBITS) are resolved with a single table access;
///	- long code entry allocations are not optimized, because long codes are
///	  unfrequent;
///	- decoding tables are used by hufDecode();
fn build_decoding_table(
    encoding_table: &[i64],
    min_hcode_index: usize,
    max_hcode_index: usize,
) -> Result<Vec<Code>> {
    let mut decoding_table = vec![Code::Empty; DECODING_TABLE_SIZE]; // not an array because of code not being copy

    for code_index in min_hcode_index..=max_hcode_index {
        let hcode = encoding_table[code_index];

        let code = code(hcode);
        let length = length(hcode);

        if (code >> length) != 0 {
            return Err(Error::invalid(INVALID_TABLE_ENTRY));
        }

        if length > DECODE_BITS as i64 {
            let long_code = &mut decoding_table[(code >> (length - DECODE_BITS as i64)) as usize];

            match long_code {
                Code::Empty => *long_code = Code::Long(vec![code_index as u16]),
                Code::Long(lits) => lits.push(code_index as u16),
                _ => {
                    return Err(Error::invalid(INVALID_TABLE_ENTRY));
                }
            }
        } else if length != 0 {
            let default_value = Code::Short(ShortCode {
                value: code_index as i32,
                len: length as u8,
            });

            let start_index = (code << (DECODE_BITS as i64 - length)) as usize;
            let count = 1 << (DECODE_BITS as i64 - length);

            for value in &mut decoding_table[start_index..start_index + count] {
                *value = default_value.clone();
            }
        }
    }

    Ok(decoding_table)
}

/// Run-length-decompresses all zero runs from the packed table to the encoding table
fn read_encoding_table(
    packed: &mut impl Read,
    min_hcode_index: usize,
    max_hcode_index: usize,
) -> Result<Vec<i64>> {
    let mut encoding_table = vec![0_i64; ENCODING_TABLE_SIZE];
    let mut code_bits = 0_i64;
    let mut code_bit_count = 0_i64;

    let mut index = min_hcode_index;
    while index <= max_hcode_index {
        let code_len = read_bits(6, &mut code_bits, &mut code_bit_count, packed)?;
        encoding_table[index] = code_len;

        if code_len == LONG_ZEROCODE_RUN {
            let zerun =
                read_bits(8, &mut code_bits, &mut code_bit_count, packed)? + SHORTEST_LONG_RUN;

            if zerun < 0 || index as i64 + zerun > max_hcode_index as i64 + 1 {
                return Err(Error::invalid(TABLE_TOO_LONG));
            }

            for value in &mut encoding_table[index..index + zerun as usize] {
                *value = 0;
            }

            index += zerun as usize;
        } else if code_len >= SHORT_ZEROCODE_RUN {
            let zerun = code_len - SHORT_ZEROCODE_RUN + 2;
            if zerun < 0 || index as i64 + zerun > max_hcode_index as i64 + 1 {
                return Err(Error::invalid(TABLE_TOO_LONG));
            }

            for value in &mut encoding_table[index..index + zerun as usize] {
                *value = 0;
            }

            index += zerun as usize;
        } else {
            index += 1;
        }
    }

    build_canonical_table(&mut encoding_table);

    Ok(encoding_table)
}

#[inline]
fn length(code: i64) -> i64 {
    code & 63
}

#[inline]
fn code(code: i64) -> i64 {
    code >> 6
}

// TODO Use BitStreamReader for all the bit reads?!
#[inline]
fn read_bits(
    count: i64,
    code_bits: &mut i64,
    code_bit_count: &mut i64,
    input: &mut impl Read,
) -> Result<i64> {
    while *code_bit_count < count {
        read_byte(code_bits, code_bit_count, input)?;
    }

    *code_bit_count -= count;
    Ok((*code_bits >> *code_bit_count) & ((1 << count) - 1))
}

#[inline]
fn read_byte(code_bits: &mut i64, bit_count: &mut i64, input: &mut impl Read) -> UnitResult {
    *code_bits = (*code_bits << 8) | u8::read(input)? as i64;
    *bit_count += 8;
    Ok(())
}

#[inline]
fn read_code_into_vec(
    code: u16,
    run_length_code: usize,
    code_bits: &mut i64,
    code_bit_count: &mut i64,
    read: &mut impl Read,
    out: &mut Vec<u16>,
    max_len: usize,
) -> UnitResult {
    if code as usize == run_length_code {
        if *code_bit_count < 8 {
            read_byte(code_bits, code_bit_count, read)?;
        }

        *code_bit_count -= 8;

        let code_repetitions = *code_bits >> *code_bit_count;
        if out.len() as i64 + code_repetitions > max_len as i64 {
            return Err(Error::invalid(TOO_MUCH_DATA));
        } else if out.is_empty() {
            return Err(Error::invalid(NOT_ENOUGH_DATA));
        }

        let repeated_code = *out.last().unwrap();
        out.extend(std::iter::repeat(repeated_code).take(code_repetitions as usize));
    } else if out.len() < max_len {
        out.push(code);
    } else {
        return Err(Error::invalid(TOO_MUCH_DATA));
    }

    Ok(())
}

fn count_frequencies(frequencies: &mut [i64], data: &[u16]) {
    for value in data {
        frequencies[*value as usize] += 1;
    }
}

fn write_bits(
    count: i64,
    bits: i64,
    code_bits: &mut i64,
    code_bit_count: &mut i64,
    mut out: impl Write,
) -> UnitResult {
    *code_bits = *code_bits << count;
    *code_bit_count += count;

    *code_bits = *code_bits | bits;

    while *code_bit_count >= 8 {
        *code_bit_count -= 8;
        out.write(&[(*code_bits >> *code_bit_count) as u8])?; // TODO make sure never or always wraps?
    }
    Ok(())
}

fn write_code(scode: i64, code_bits: &mut i64, code_bit_count: &mut i64, mut out: impl Write) -> UnitResult {
    write_bits(
        length(scode),
        code(scode),
        code_bits,
        code_bit_count,
        &mut out,
    )
}

#[inline(always)]
fn send_code(
    scode: i64,
    run_count: i32,
    run_code: i64,
    code_bits: &mut i64,
    code_bit_count: &mut i64,
    mut out: impl Write,
) -> UnitResult {
    //
    // Output a run of runCount instances of the symbol sCount.
    // Output the symbols explicitly, or if that is shorter, output
    // the sCode symbol once followed by a runCode symbol and runCount
    // expressed as an 8-bit number.
    //
    if length(scode) + length(run_code) + 8 < length(scode) * i64::from(run_count) {
        write_code(scode, code_bits, code_bit_count, &mut out)?;
        write_code(run_code, code_bits, code_bit_count, &mut out)?;
        write_bits(8, run_count as i64, code_bits, code_bit_count, &mut out)?;
    } else {
        for _ in 0..=(run_count as i64) {
            write_code(scode, code_bits, code_bit_count, &mut out)?;
        }
    }
    Ok(())
}

fn encode(
    frequencies: &[i64],
    uncompressed: &[u16],
    run_length_code: usize,
    compressed: &mut [u8],
) -> Result<u32> {
    let mut code_bits = 0;
    let mut code_bit_count = 0;
    let mut s = uncompressed[0];
    let mut cs = 0;
    let mut out = std::io::Cursor::new(compressed);

    //
    // Loop on input values
    //
    for index in 1..uncompressed.len() {
        //
        // Count same values or send code
        //
        if s == uncompressed[index] && cs < 255 {
            cs += 1;
        } else {
            send_code(
                frequencies[s as usize],
                cs,
                frequencies[run_length_code],
                &mut code_bits,
                &mut code_bit_count,
                &mut out,
            )?;
            cs = 0;
        }

        s = uncompressed[index];
    }

    //
    // Send remaining code
    //
    send_code(
        frequencies[s as usize],
        cs,
        frequencies[run_length_code],
        &mut code_bits,
        &mut code_bit_count,
        &mut out,
    )?;

    let data_length = out.position(); // we shouldn't count the last byte write

    if code_bit_count != 0 {
        out.write(&[(code_bits << (8 - code_bit_count) & 0xff) as u8])?;
    }

    Ok((data_length * 8 + code_bit_count as u64) as u32)
}

///
/// Pack an encoding table:
///	- only code lengths, not actual codes, are stored
///	- runs of zeroes are compressed as follows:
///
///	  unpacked		packed
///	  --------------------------------
///	  1 zero		0	(6 bits)
///	  2 zeroes		59
///	  3 zeroes		60
///	  4 zeroes		61
///	  5 zeroes		62
///	  n zeroes (6 or more)	63 n-6	(6 + 8 bits)
///
fn pack_encoding_table(
    frequencies: &[i64],
    min_index: usize,
    max_index: usize,
    table: &mut [u8],
) -> Result<usize> {
    let mut out = std::io::Cursor::new(table);
    let mut code_bits = 0_i64;
    let mut code_bit_count = 0_i64;

    let mut index = min_index;
    while index <= max_index {
        let code_length = length(frequencies[index]);

        if code_length == 0 {
            let mut zero_run = 1;

            while index < max_index && zero_run < LONGEST_LONG_RUN {
                if length(frequencies[index + 1]) > 0 {
                    break;
                }
                index += 1;
                zero_run += 1;
            }

            if zero_run >= 2 {
                if zero_run >= SHORTEST_LONG_RUN {
                    write_bits(
                        6,
                        LONG_ZEROCODE_RUN,
                        &mut code_bits,
                        &mut code_bit_count,
                        &mut out,
                    )?;
                    write_bits(
                        8,
                        zero_run - SHORTEST_LONG_RUN,
                        &mut code_bits,
                        &mut code_bit_count,
                        &mut out,
                    )?;
                } else {
                    write_bits(
                        6,
                        SHORT_ZEROCODE_RUN + zero_run - 2,
                        &mut code_bits,
                        &mut code_bit_count,
                        &mut out,
                    )?;
                }
                index += 1; // we must increment or else this may go very wrong
                continue;
            }
        }
        write_bits(
            6,
            code_length,
            &mut code_bits,
            &mut code_bit_count,
            &mut out,
        )?;
        index += 1;
    }

    if code_bit_count > 0 {
        out.write(&[(code_bits << (8 - code_bit_count)) as u8])?;
    }
    Ok(out.position() as usize)
}

/// Build a "canonical" Huffman code table:
///	- for each (uncompressed) symbol, hcode contains the length
///	  of the corresponding code (in the compressed data)
///	- canonical codes are computed and stored in hcode
///	- the rules for constructing canonical codes are as follows:
///	  * shorter codes (if filled with zeroes to the right)
///	    have a numerically higher value than longer codes
///	  * for codes with the same length, numerical values
///	    increase with numerical symbol values
///	- because the canonical code table can be constructed from
///	  symbol lengths alone, the code table can be transmitted
///	  without sending the actual code values
///	- see http://www.compressconsult.com/huffman/
fn build_canonical_table(code_table: &mut [i64]) {
    debug_assert_eq!(code_table.len(), ENCODING_TABLE_SIZE);

    let mut count_per_code = [0_i64; 59];

    for &code in code_table.iter() {
        count_per_code[code as usize] += 1;
    }

    // For each i from 58 through 1, compute the
    // numerically lowest code with length i, and
    // store that code in n[i].
    let mut c = 0_i64;
    for count in &mut count_per_code.iter_mut().rev() {
        let nc = (c + *count) >> 1;
        *count = c;
        c = nc;
    }

    // hcode[i] contains the length, l, of the
    // code for symbol i.  Assign the next available
    // code of length l to the symbol and store both
    // l and the code in hcode[i].
    for code_i in code_table.iter_mut() {
        let l = *code_i;
        if l > 0 {
            *code_i = l | (count_per_code[l as usize] << 6);
            count_per_code[l as usize] += 1;
        }
    }
}

/// Frequency with position, used for MinHeap.
#[derive(Eq, PartialEq)]
struct HeapFrequency {
    position: usize,
    frequency: i64,
}

impl Ord for HeapFrequency {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .frequency
            .cmp(&self.frequency)
            .then_with(|| other.position.cmp(&self.position))
    }
}

impl PartialOrd for HeapFrequency {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compute Huffman codes (based on frq input) and store them in frq:
///	- code structure is : [63:lsb - 6:msb] | [5-0: bit length];
///	- max code length is 58 bits;
///	- codes outside the range [im-iM] have a null length (unused values);
///	- original frequencies are destroyed;
///	- encoding tables are used by hufEncode() and hufBuildDecTable();
///
/// NB: The following code "(*a == *b) && (a > b))" was added to ensure
///     elements in the heap with the same value are sorted by index.
///     This is to ensure, the STL make_heap()/pop_heap()/push_heap() methods
///     produced a resultant sorted heap that is identical across OSes.
fn build_encoding_table(
    frequencies: &mut [i64], // input frequencies, output encoding table
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

    // We need to use vec here or we overflow the stack.
    let mut h_link = vec![0_usize; ENCODING_TABLE_SIZE];
    let mut frequency_heap = vec![0_usize; ENCODING_TABLE_SIZE];

    // This is a good solution since we don't have usize::MAX items (no panics or UB),
    // and since this is short-circuit, it stops at the first in order non zero element.
    let min_frequency_index = frequencies.iter().position(|f| *f != 0).unwrap_or(0);

    let mut frequency_count = 0;
    let mut max_frequency_index = 0;

    for index in min_frequency_index..ENCODING_TABLE_SIZE {
        h_link[index] = index;

        if frequencies[index] != 0 {
            frequency_heap[frequency_count] = index; // &frequencies[index];
            frequency_count += 1;
            max_frequency_index = index;
        }
    }

    // Add a pseudo-symbol, with a frequency count of 1, to frq;
    // adjust the fHeap and hlink array accordingly.  Function
    // hufEncode() uses the pseudo-symbol for run-length encoding.

    max_frequency_index += 1;
    frequencies[max_frequency_index] = 1;
    frequency_heap[frequency_count] = max_frequency_index; // &frequencies[max_frequency_index];
    frequency_count += 1;

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
    let mut heap = BinaryHeap::with_capacity(frequency_count);
    for index in frequency_heap.drain(..frequency_count) {
        heap.push(HeapFrequency {
            position: index,
            frequency: frequencies[index],
        });
    }

    let mut s_code = vec![0_i64; ENCODING_TABLE_SIZE];

    while frequency_count > 1 {
        // Find the indices, mm and m, of the two smallest non-zero frq
        // values in fHeap, add the smallest frq to the second-smallest
        // frq, and remove the smallest frq value from fHeap.
        let mm = heap.pop().expect("Cannot pop heap bug");
        frequency_count -= 1;

        let mut m = heap.pop().expect("Cannot pop heap bug");

        m.frequency += mm.frequency;
        let high_position = m.position;
        heap.push(m);

        // The entries in scode are linked into lists with the
        // entries in hlink serving as "next" pointers and with
        // the end of a list marked by hlink[j] == j.
        //
        // Traverse the lists that start at scode[m] and scode[mm].
        // For each element visited, increment the length of the
        // corresponding code by one bit. (If we visit scode[j]
        // during the traversal, then the code for symbol j becomes
        // one bit longer.)
        //
        // Merge the lists that start at scode[m] and scode[mm]
        // into a single list that starts at scode[m].

        // Add a bit to all codes in the first list.
        let mut j = high_position;
        loop {
            s_code[j] += 1;

            assert!(s_code[j] <= 58);

            if h_link[j] == j {
                // merge the two lists
                h_link[j] = mm.position;
                break;
            }
            j = h_link[j];
        }

        //
        // Add a bit to all codes in the second list
        //
        let mut j = mm.position;
        loop {
            s_code[j] += 1;

            assert!(s_code[j] <= 58);

            if h_link[j] == j {
                break;
            }

            j = h_link[j];
        }
    }

    // Build a canonical Huffman code table, replacing the code
    // lengths in scode with (code, code length) pairs.  Copy the
    // code table from scode into frq.
    build_canonical_table(&mut s_code);
    frequencies.copy_from_slice(&s_code);

    (min_frequency_index, max_frequency_index)
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::Rng;

    const UNCOMPRESSED_ARRAY: [u16; 100] = [
        3852, 2432, 33635, 49381, 10100, 15095, 62693, 63738, 62359, 5013, 7715, 59875, 28182,
        34449, 19983, 20399, 63407, 29486, 4877, 26738, 44815, 14042, 46091, 48228, 25682, 35412,
        7582, 65069, 6632, 54124, 13798, 27503, 52154, 61961, 30474, 46880, 39097, 15754, 52897,
        42371, 54053, 14178, 48276, 34591, 42602, 32126, 42062, 31474, 16274, 55991, 2882, 17039,
        56389, 20835, 57057, 54081, 3414, 33957, 52584, 10222, 25139, 40002, 44980, 1602, 48021,
        19703, 6562, 61777, 41582, 201, 31253, 51790, 15888, 40921, 3627, 12184, 16036, 26349,
        3159, 29002, 14535, 50632, 18118, 33583, 18878, 59470, 32835, 9347, 16991, 21303, 26263,
        8312, 14017, 41777, 43240, 3500, 60250, 52437, 45715, 61520,
    ];
    const COMPRESSED_ARRAY: [u8; 703] = [
        0xc9, 0x0, 0x0, 0x0, 0x2e, 0xfe, 0x0, 0x0, 0x56, 0x2, 0x0, 0x0, 0xa2, 0x2, 0x0, 0x0, 0x0,
        0x0, 0x0, 0x0, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xd6, 0x47,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x28, 0x1f, 0xff, 0xff, 0xed, 0x87, 0xff, 0xff, 0xf0,
        0x91, 0xff, 0xf8, 0x1f, 0xf4, 0xf1, 0xff, 0x78, 0x1f, 0xfd, 0xa1, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xfa, 0xc7, 0xfe, 0x4, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xed, 0x1f, 0xf3, 0xf1, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xe8, 0x7, 0xfd, 0xf8,
        0x7f, 0xff, 0xff, 0xff, 0xfd, 0x10, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x51, 0xff,
        0xff, 0xff, 0xff, 0xfe, 0x1, 0xff, 0x73, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0x0, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xfc, 0xa4, 0x7f, 0xf5, 0x7, 0xfc, 0x48, 0x7f, 0xe0, 0x47, 0xff, 0xff,
        0xf5, 0x91, 0xff, 0xff, 0xff, 0xff, 0xf1, 0xf1, 0xff, 0xff, 0xff, 0xff, 0xf8, 0x21, 0xff,
        0x7f, 0x1f, 0xf8, 0xd1, 0xff, 0xe7, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xbc, 0x1f, 0xf2, 0x91,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x1c, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xe7,
        0x1f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfc, 0x8c, 0x7f, 0xff, 0xff, 0xc, 0x1f, 0xff, 0xff,
        0xe5, 0x7, 0xff, 0xff, 0xfa, 0x81, 0xff, 0xff, 0xff, 0x20, 0x7f, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xfe, 0xbc, 0x7f, 0xff, 0xff, 0xff, 0xfc, 0x38, 0x7f, 0xff,
        0xff, 0xff, 0xfc, 0xd0, 0x7f, 0xd3, 0xc7, 0xff, 0xff, 0xf7, 0x91, 0xff, 0xff, 0xff, 0xff,
        0xfe, 0xc1, 0xff, 0xff, 0xff, 0xff, 0xf9, 0x61, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xc7,
        0x87, 0xff, 0xff, 0xfd, 0x81, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf1, 0x87, 0xff, 0xff,
        0xff, 0xff, 0xfe, 0x87, 0xff, 0x58, 0x7f, 0xff, 0xff, 0xff, 0xfd, 0xec, 0x7f, 0xff, 0xff,
        0xff, 0xfe, 0xd0, 0x7f, 0xff, 0xff, 0xff, 0xff, 0x6c, 0x7f, 0xcb, 0x47, 0xff, 0xff, 0xf3,
        0x61, 0xff, 0xff, 0xff, 0x80, 0x7f, 0xe1, 0xc7, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x1f,
        0x1f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x18, 0x1f, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xfd, 0xcc, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf8, 0x11, 0xff, 0xff,
        0xff, 0xff, 0xf8, 0x41, 0xff, 0xbc, 0x1f, 0xff, 0xff, 0xc4, 0x47, 0xff, 0xff, 0xf2, 0x91,
        0xff, 0xe0, 0x1f, 0xff, 0xff, 0xff, 0xff, 0x6d, 0x1f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0x2, 0x1f, 0xf9, 0xe1, 0xff, 0xff, 0xff, 0xff, 0xfc, 0xe1,
        0xff, 0xff, 0xfd, 0xb0, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xe1, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0x5a, 0x1f, 0xfc, 0x81, 0xbf, 0x29, 0x1b, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xf3, 0x61, 0xbf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xc8, 0x1b,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf6, 0xb1, 0xbf, 0xff, 0xfd, 0x80, 0x6f, 0xff,
        0xff, 0xf, 0x1b, 0xf8, 0xc1, 0xbf, 0xff, 0xfc, 0xb4, 0x6f, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xda, 0x46, 0xfc, 0x54, 0x6f, 0xc9, 0x6, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x21, 0x1b, 0xff, 0xff, 0xe0, 0x86, 0xff, 0xff,
        0xff, 0xff, 0xe2, 0xc6, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xf3, 0x91, 0xbf, 0xff, 0xfe, 0x24, 0x6f, 0xff, 0xff, 0x6b,
        0x1b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfd, 0xb1, 0xbf, 0xfa, 0x1b, 0xfb, 0x11,
        0xbf, 0xff, 0xfe, 0x8, 0x6f, 0xff, 0xff, 0x42, 0x1b, 0xff, 0xff, 0xff, 0xff, 0xb9, 0x1b,
        0xff, 0xff, 0xcf, 0xc6, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xf1, 0x31,
        0x86, 0x10, 0x9, 0xb4, 0xe4, 0x4c, 0xf7, 0xef, 0x42, 0x87, 0x6a, 0xb5, 0xc2, 0x34, 0x9e,
        0x2f, 0x12, 0xae, 0x21, 0x68, 0xf2, 0xa8, 0x74, 0x37, 0xe1, 0x98, 0x14, 0x59, 0x57, 0x2c,
        0x24, 0x3b, 0x35, 0x6c, 0x1b, 0x8b, 0xcc, 0xe6, 0x13, 0x38, 0xc, 0x8e, 0xe2, 0xc, 0xfe,
        0x49, 0x73, 0xbc, 0x2b, 0x7b, 0x9, 0x27, 0x79, 0x14, 0xc, 0x94, 0x42, 0xf8, 0x7c, 0x1,
        0x8d, 0x26, 0xde, 0x87, 0x26, 0x71, 0x50, 0x45, 0xc6, 0x28, 0x40, 0xd5, 0xe, 0x8d, 0x8,
        0x1e, 0x4c, 0xa4, 0x79, 0x57, 0xf0, 0xc3, 0x6d, 0x5c, 0x6d, 0xc0,
    ];

    fn fill(rng: &mut impl Rng, size: usize) -> Vec<u16> {
        let mut data = vec![0_u16; size];
        data.iter_mut().for_each(|v| {
            *v = rng.gen_range(0_u16, u16::MAX);
        });
        data
    }

    /// Test using both input and output from a custom ILM OpenEXR test.
    #[test]
    fn compression_comparation() {
        let raw = compress(&UNCOMPRESSED_ARRAY).unwrap();
        assert_eq!(raw, COMPRESSED_ARRAY.to_vec());
    }

    #[test]
    fn round_trip() {
        let mut rng = rand::thread_rng();

        let raw = fill(&mut rng, u16::MAX as usize);

        let compressed = compress(&raw).unwrap();
        let uncompressed = decompress(&compressed, raw.len()).unwrap();

        assert_eq!(uncompressed, raw);
    }

    #[test]
    fn round_trip100() {
        let mut rng = rand::thread_rng();

        for size_multiplier in 1..100 {
            let raw = fill(&mut rng, size_multiplier * 50_000);

            let compressed = compress(&raw).unwrap();
            let uncompressed = decompress(&compressed, raw.len()).unwrap();

            println!("passed size {}", raw.len());
            assert_eq!(uncompressed, raw);
        }
    }

    #[test]
    fn test_actual_image_data(){

        // FAILS:
        // let uncompressed: &[u16] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65535, 65534, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let uncompressed: &[u16] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 ];

        // WORKS:
        // let uncompressed: &[u16] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0 ];
        // let uncompressed: &[u16] = &[ 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0];

        inspect!(uncompressed.len());
        let compressed = compress(uncompressed).unwrap();
        let decompressed = decompress(&compressed, uncompressed.len()).unwrap();

        assert_eq!(uncompressed, decompressed.as_slice());
    }
}
