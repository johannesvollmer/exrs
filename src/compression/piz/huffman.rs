//! 16-bit Huffman compression and decompression.
//! Huffman compression and decompression routines written
//!    by Christian Rouet for his PIZ image file format.
// see https://github.com/AcademySoftwareFoundation/openexr/blob/88246d991e0318c043e6f584f7493da08a31f9f8/OpenEXR/IlmImf/ImfHuf.cpp

use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    convert::TryFrom,
    io::{Cursor, Read, Write},
};

use crate::{
    error::{u64_to_usize, Error, Result, UnitResult},
    io::Data,
    math::RoundingMode,
};

pub fn decompress(compressed: &[u8], expected_size: usize) -> Result<Vec<u16>> {
    let mut remaining_compressed = compressed;

    let min_code_index = usize::try_from(u32::read_le(&mut remaining_compressed)?)?;
    let max_code_index_32 = u32::read_le(&mut remaining_compressed)?;
    let _table_size = usize::try_from(u32::read_le(&mut remaining_compressed)?)?; // TODO check this and return Err?
    let bit_count = usize::try_from(u32::read_le(&mut remaining_compressed)?)?;
    let _skipped = u32::read_le(&mut remaining_compressed)?; // what is this

    let max_code_index = usize::try_from(max_code_index_32)?;
    if min_code_index >= ENCODING_TABLE_SIZE || max_code_index >= ENCODING_TABLE_SIZE {
        return Err(Error::invalid(INVALID_TABLE_SIZE));
    }

    if RoundingMode::Up.divide(bit_count, 8) > remaining_compressed.len() {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    let (encoding_table, symbols) =
        read_encoding_table(&mut remaining_compressed, min_code_index, max_code_index)?;
    if bit_count > 8 * remaining_compressed.len() {
        return Err(Error::invalid(INVALID_BIT_COUNT));
    }

    let decoding_table = build_decoding_table(&encoding_table, &symbols, max_code_index_32)?;

    let result = decode_with_tables(
        &decoding_table,
        remaining_compressed,
        bit_count,
        max_code_index_32,
        expected_size,
    )?;

    Ok(result)
}

pub fn compress(uncompressed: &[u16]) -> Result<Vec<u8>> {
    if uncompressed.is_empty() {
        return Ok(vec![]);
    }

    let mut frequencies = count_frequencies(uncompressed);
    let (min_code_index, max_code_index) = build_encoding_table(&mut frequencies)?;

    let mut result = Cursor::new(Vec::with_capacity(uncompressed.len()));
    u32::write_slice_le(&mut result, &[0; 5])?; // we come back to these later
                                                // after we know more about the
                                                // compressed data

    let table_start = result.position();
    pack_encoding_table(&frequencies, min_code_index, max_code_index, &mut result)?;

    let data_start = result.position();
    let bit_count =
        encode_with_frequencies(&frequencies, uncompressed, max_code_index, &mut result)?;

    // write meta data after this
    result.set_position(0);
    let table_length = data_start - table_start;

    u32::try_from(min_code_index)?.write_le(&mut result)?;
    u32::try_from(max_code_index)?.write_le(&mut result)?;
    u32::try_from(table_length)?.write_le(&mut result)?;
    u32::try_from(bit_count)?.write_le(&mut result)?;
    0_u32.write_le(&mut result)?;

    Ok(result.into_inner())
}

const ENCODE_BITS: u64 = 16; // literal (value) bit length
const DECODE_BITS: u64 = 12; // decoding bit size (>= 8); 16 KiB of u32 entries fits L1

const ENCODING_TABLE_SIZE: usize = ((1 << ENCODE_BITS) + 1) as usize;
const DECODING_TABLE_SIZE: usize = (1 << DECODE_BITS) as usize;

const SHORT_ZEROCODE_RUN: u64 = 59;
const LONG_ZEROCODE_RUN: u64 = 63;
const SHORTEST_LONG_RUN: u64 = 2 + LONG_ZEROCODE_RUN - SHORT_ZEROCODE_RUN;
const LONGEST_LONG_RUN: u64 = 255 + SHORTEST_LONG_RUN;

/// The decoding table, indexed by the next `DECODE_BITS` bits of input.
struct DecodingTable {
    /// `symbol << 6 | length` for a code of at most `DECODE_BITS` bits whose
    /// symbol is a plain value. The zero low bits of `length << 26 | symbol
    /// << 6` send the run-length code and symbols too large for a value to
    /// the slow path, sharing the one branch that long codes take; those are
    /// zero, as are entries where no code has this prefix.
    entries: Box<[u32; DECODING_TABLE_SIZE]>,
    long: LongCodes,
}

/// Codes longer than `DECODE_BITS` bits, decoded arithmetically as in
/// OpenEXR's `FastHufDecoder`. Canonical codes of one length are consecutive,
/// and left-aligned in 64 bits, every code compares greater than all longer
/// codes. So the length of the code at the start of the input is the shortest
/// that has codes and whose first left-aligned code is not greater than the
/// input bits.
struct LongCodes {
    /// The lengths that have codes, ascending.
    lengths: Vec<u64>,
    /// The first code of each length, left-aligned.
    first_left_aligned: [u64; 59],
    /// The first code of each length.
    first: [u64; 59],
    /// Where each length's symbols start in `symbols`.
    offset: [u32; 59],
    count: [u32; 59],
    /// Ordered by length, then by code.
    symbols: Vec<u32>,
    max_length: u64,
}

impl LongCodes {
    /// The symbol and length of the long code at the start of `bits`, which
    /// must hold at least `max_length` valid bits.
    #[inline]
    fn decode(&self, bits: u64) -> Option<(u32, u64)> {
        let length = *self
            .lengths
            .iter()
            .find(|&&length| bits >= self.first_left_aligned[length as usize])?;
        let index = (bits >> (64 - length)) - self.first[length as usize];
        (index < u64::from(self.count[length as usize])).then(|| {
            let symbol = self.symbols[(self.offset[length as usize] + index as u32) as usize];
            (symbol, length)
        })
    }
}

/// Reads input bits most significant first from a 64-bit register,
/// refilled several bytes at a time. Past the end of the input it reads
/// zeros, so callers check `position()` against the input length once,
/// rather than before every code.
struct BitReader<'i> {
    input: &'i [u8],
    /// The next unread bits, most significant first. Bits past `count` are
    /// already those of the input, or zero.
    bits: u64,
    /// The number of valid bits in `bits`.
    count: u64,
    /// The next input byte not yet in `bits`; beyond the input once zeros
    /// past its end have been read.
    next_byte: usize,
}

impl<'i> BitReader<'i> {
    fn new(input: &'i [u8]) -> Self {
        let mut reader = Self {
            input,
            bits: 0,
            count: 0,
            next_byte: 0,
        };
        reader.refill();
        reader
    }

    /// Bits consumed so far.
    #[inline(always)]
    fn position(&self) -> usize {
        self.next_byte * 8 - self.count as usize
    }

    /// Fill `bits` to at least 57 bits.
    #[inline(always)]
    fn refill(&mut self) {
        if let Some(bytes) = self.input.get(self.next_byte..self.next_byte + 8) {
            // Takes whole bytes only. Part of the next byte may also land in
            // `bits`, where the next refill writes the same bits again.
            let word = u64::from_be_bytes(<[u8; 8]>::try_from(bytes).unwrap());
            self.bits |= word >> self.count;
            self.next_byte += ((63 - self.count) / 8) as usize;
            self.count |= 56;
        } else {
            while self.count <= 56 {
                let byte = self.input.get(self.next_byte).copied().unwrap_or(0);
                self.bits |= u64::from(byte) << (56 - self.count);
                self.next_byte += 1;
                self.count += 8;
            }
        }
    }

    /// The next `count` (1 to 57, after a refill) bits.
    #[inline(always)]
    fn peek(&self, count: u64) -> u64 {
        self.bits >> (64 - count)
    }

    #[inline(always)]
    fn consume(&mut self, count: u64) {
        debug_assert!(count <= self.count);
        self.bits <<= count;
        self.count -= count;
    }

    /// Continue reading from bit `position`.
    fn seek(&mut self, position: usize) {
        self.next_byte = position / 8;
        self.bits = 0;
        self.count = 0;
        self.refill();
        self.consume((position % 8) as u64);
    }
}

/// The `count` (at most 64) input bits starting at bit `position`, zero
/// past the end of the input.
fn peek_bits(input: &[u8], position: usize, count: u64) -> u64 {
    debug_assert!((1..=64).contains(&count));
    let byte = position / 8;
    let mut bytes = [0_u8; 16];
    let tail = input.get(byte..).unwrap_or(&[]);
    let available = tail.len().min(16);
    bytes[..available].copy_from_slice(&tail[..available]);
    let bits = u128::from_be_bytes(bytes) << (position % 8);
    (bits >> (128 - count)) as u64
}

/// Decode `input_bit_count` bits of input based on the decoding table.
fn decode_with_tables(
    decoding_table: &DecodingTable,
    input: &[u8],
    input_bit_count: usize,
    run_length_code: u32,
    expected_output_size: usize,
) -> Result<Vec<u16>> {
    // The expected size comes from the file. Every code takes at least one
    // bit and a run-length code (at least 9 bits with its count) repeats at
    // most 255 values, so a larger size cannot be valid; refusing it before
    // allocating keeps a small file from requesting a huge buffer.
    if expected_output_size > input_bit_count.saturating_mul(29) {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    let mut output = vec![0_u16; expected_output_size];
    let mut length = 0;
    let mut reader = BitReader::new(input);
    let entries = &*decoding_table.entries;

    // As in OpenEXR's FastHufDecoder, decode until the output is full and
    // check the input was long enough once, at the end.
    while length < expected_output_size {
        if reader.count < 32 {
            reader.refill();
        }

        let entry = entries[reader.peek(DECODE_BITS) as usize];

        let code_length = u64::from(entry & 63);
        if code_length != 0 {
            reader.consume(code_length);
            output[length] = (entry >> 6) as u16;
            length += 1;
            continue;
        }

        let symbol = if entry != 0 {
            reader.consume(u64::from(entry >> 26));
            entry >> 6 & 0xfffff
        } else {
            let long = &decoding_table.long;
            let position = reader.position();
            // After a refill the register holds at least 57 bits; only codes
            // of 58 bits can need the slower window.
            let bits = if reader.count >= long.max_length {
                reader.bits
            } else {
                peek_bits(input, position, 64)
            };
            let (symbol, code_length) =
                long.decode(bits).ok_or_else(|| Error::invalid(INVALID_CODE))?;
            if code_length <= reader.count {
                reader.consume(code_length);
            } else {
                reader.seek(position + code_length as usize);
            }
            symbol
        };

        if symbol == run_length_code {
            if reader.count < 8 {
                reader.refill();
            }
            let repetitions = reader.peek(8) as usize;
            reader.consume(8);

            if length + repetitions > expected_output_size {
                return Err(Error::invalid(TOO_MUCH_DATA));
            }
            if length == 0 {
                return Err(Error::invalid(NOT_ENOUGH_DATA));
            }
            let repeated = output[length - 1];
            output[length..length + repetitions].fill(repeated);
            length += repetitions;
        } else {
            // code may be too large for u16
            output[length] = u16::try_from(symbol)?;
            length += 1;
        }
    }

    if reader.position() > input_bit_count {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    Ok(output)
}

/// Build a decoding table based on the encoding table code:
///    - short codes (<= `DECODE_BITS`) are resolved with a single table access;
///    - long codes are resolved arithmetically, see `LongCodes`.
fn build_decoding_table(
    encoding_table: &[u64],
    symbols: &[u32],
    run_length_code: u32,
) -> Result<DecodingTable> {
    let mut entries = Box::new([0_u32; DECODING_TABLE_SIZE]);
    let mut long = LongCodes {
        lengths: Vec::new(),
        first_left_aligned: [0; 59],
        first: [0; 59],
        offset: [0; 59],
        count: [0; 59],
        symbols: Vec::new(),
        max_length: 0,
    };

    for &symbol in symbols {
        let encoded_code = encoding_table[symbol as usize];

        let code = code(encoded_code);
        let length = length(encoded_code);

        if code >> length != 0 {
            return Err(Error::invalid(INVALID_TABLE_ENTRY));
        }

        if length > DECODE_BITS {
            let prefix = u64_to_usize(code >> (length - DECODE_BITS), "long code index")?;
            if entries[prefix] != 0 {
                return Err(Error::invalid(INVALID_TABLE_ENTRY));
            }

            // Symbols arrive in ascending order, and canonical codes of one
            // length ascend with them: the first seen is the length's first.
            let length = length as usize;
            if long.count[length] == 0 {
                long.first[length] = code;
                long.first_left_aligned[length] = code << (64 - length);
            }
            long.count[length] += 1;
            long.max_length = long.max_length.max(length as u64);
        } else if length != 0 {
            let start_index = u64_to_usize(code << (DECODE_BITS - length), "huffman start index")?;
            let count = u64_to_usize(1 << (DECODE_BITS - length), "huffman count")?;
            let entry = if symbol == run_length_code || symbol > u32::from(u16::MAX) {
                (length as u32) << 26 | symbol << 6
            } else {
                symbol << 6 | length as u32
            };
            entries[start_index..start_index + count].fill(entry);
        }
    }

    long.lengths = (DECODE_BITS + 1..=long.max_length)
        .filter(|&length| long.count[length as usize] != 0)
        .collect();

    // Group the long-code symbols by length, keeping code order within each.
    let mut total = 0;
    for (offset, &count) in long.offset.iter_mut().zip(&long.count) {
        *offset = total;
        total += count;
    }
    let mut next = long.offset;
    long.symbols = vec![0; total as usize];
    for &symbol in symbols {
        let length = length(encoding_table[symbol as usize]);
        if length > DECODE_BITS {
            let slot = &mut next[length as usize];
            long.symbols[*slot as usize] = symbol;
            *slot += 1;
        }
    }

    Ok(DecodingTable {
        entries,
        long,
    })
}

/// Run-length-decompresses all zero runs from the packed table to the encoding
/// table
fn read_encoding_table(
    packed: &mut impl Read,
    min_code_index: usize,
    max_code_index: usize,
) -> Result<(Vec<u64>, Vec<u32>)> {
    let mut code_bits = 0_u64;
    let mut code_bit_count = 0_u64;
    // The symbols that have a code, ascending: usually far fewer than the
    // table's 65,537 entries, so later passes visit only these.
    let mut symbols = Vec::new();

    // TODO push() into encoding table instead of index stuff?
    let mut encoding_table = vec![0_u64; ENCODING_TABLE_SIZE];
    let mut code_index = min_code_index;
    while code_index <= max_code_index {
        let code_len = read_bits(6, &mut code_bits, &mut code_bit_count, packed)?;
        encoding_table[code_index] = code_len;

        if code_len == LONG_ZEROCODE_RUN {
            let zerun_bits = read_bits(8, &mut code_bits, &mut code_bit_count, packed)?;
            let zerun = usize::try_from(zerun_bits + SHORTEST_LONG_RUN).unwrap();

            if code_index + zerun > max_code_index + 1 {
                return Err(Error::invalid(TABLE_TOO_LONG));
            }

            for value in &mut encoding_table[code_index..code_index + zerun] {
                *value = 0;
            }

            code_index += zerun;
        } else if code_len >= SHORT_ZEROCODE_RUN {
            let duplication_count = usize::try_from(code_len - SHORT_ZEROCODE_RUN + 2).unwrap();
            if code_index + duplication_count > max_code_index + 1 {
                return Err(Error::invalid(TABLE_TOO_LONG));
            }

            for value in &mut encoding_table[code_index..code_index + duplication_count] {
                *value = 0;
            }

            code_index += duplication_count;
        } else {
            if code_len != 0 {
                symbols.push(u32::try_from(code_index)?);
            }
            code_index += 1;
        }
    }

    build_canonical_table(&mut encoding_table, symbols.iter().map(|&symbol| symbol as usize))?;
    Ok((encoding_table, symbols))
}

// TODO Use BitStreamReader for all the bit reads?!
#[inline]
fn read_bits(
    count: u64,
    code_bits: &mut u64,
    code_bit_count: &mut u64,
    input: &mut impl Read,
) -> Result<u64> {
    while *code_bit_count < count {
        read_byte(code_bits, code_bit_count, input)?;
    }

    *code_bit_count -= count;
    Ok((*code_bits >> *code_bit_count) & ((1 << count) - 1))
}

#[inline]
fn read_byte(code_bits: &mut u64, bit_count: &mut u64, input: &mut impl Read) -> UnitResult {
    *code_bits = (*code_bits << 8) | u64::from(u8::read_ne(input)?);
    *bit_count += 8;
    Ok(())
}

fn count_frequencies(data: &[u16]) -> Vec<u64> {
    let mut frequencies = vec![0_u64; ENCODING_TABLE_SIZE];

    for value in data {
        frequencies[*value as usize] += 1;
    }

    frequencies
}

fn write_bits(
    count: u64,
    bits: u64,
    code_bits: &mut u64,
    code_bit_count: &mut u64,
    mut out: impl Write,
) -> UnitResult {
    *code_bits = (*code_bits << count) | bits;
    *code_bit_count += count;

    while *code_bit_count >= 8 {
        *code_bit_count -= 8;
        out.write_all(&[
            (*code_bits >> *code_bit_count) as u8, // TODO make sure never or always wraps?
        ])?;
    }

    Ok(())
}

fn write_code(
    scode: u64,
    code_bits: &mut u64,
    code_bit_count: &mut u64,
    mut out: impl Write,
) -> UnitResult {
    write_bits(length(scode), code(scode), code_bits, code_bit_count, &mut out)
}

#[inline(always)]
fn send_code(
    scode: u64,
    run_count: u64,
    run_code: u64,
    code_bits: &mut u64,
    code_bit_count: &mut u64,
    mut out: impl Write,
) -> UnitResult {
    // Output a run of runCount instances of the symbol sCount.
    // Output the symbols explicitly, or if that is shorter, output
    // the sCode symbol once followed by a runCode symbol and runCount
    // expressed as an 8-bit number.
    if length(scode) + length(run_code) + 8 < length(scode) * run_count {
        write_code(scode, code_bits, code_bit_count, &mut out)?;
        write_code(run_code, code_bits, code_bit_count, &mut out)?;
        write_bits(8, run_count, code_bits, code_bit_count, &mut out)?;
    } else {
        for _ in 0..=run_count {
            write_code(scode, code_bits, code_bit_count, &mut out)?;
        }
    }

    Ok(())
}

fn encode_with_frequencies(
    frequencies: &[u64],
    uncompressed: &[u16],
    run_length_code: usize,
    mut out: &mut Cursor<Vec<u8>>,
) -> Result<u64> {
    let mut code_bits = 0;
    let mut code_bit_count = 0;

    let mut run_start_value = uncompressed[0];
    let mut run_length = 0;

    let start_position = out.position();

    // Loop on input values
    for &current_value in &uncompressed[1..] {
        // Count same values or send code
        if run_start_value == current_value && run_length < 255 {
            run_length += 1;
        } else {
            send_code(
                frequencies[run_start_value as usize],
                run_length,
                frequencies[run_length_code],
                &mut code_bits,
                &mut code_bit_count,
                &mut out,
            )?;

            run_length = 0;
        }

        run_start_value = current_value;
    }

    // Send remaining code
    send_code(
        frequencies[run_start_value as usize],
        run_length,
        frequencies[run_length_code],
        &mut code_bits,
        &mut code_bit_count,
        &mut out,
    )?;

    let data_length = out.position() - start_position; // we shouldn't count the
                                                       // last byte write

    if code_bit_count != 0 {
        out.write_all(&[(code_bits << (8 - code_bit_count) & 0xff) as u8])?;
    }

    Ok(data_length * 8 + code_bit_count)
}

/// Pack an encoding table:
/// 	- only code lengths, not actual codes, are stored
/// 	- runs of zeroes are compressed as follows:
/// ```md
/// 	  unpacked		packed
/// 	  --------------------------------
/// 	  1 zero		0	(6 bits)
/// 	  2 zeroes		59
/// 	  3 zeroes		60
/// 	  4 zeroes		61
/// 	  5 zeroes		62
/// 	  n zeroes (6 or more)	63 n-6	(6 + 8 bits)
/// ```
fn pack_encoding_table(
    frequencies: &[u64],
    min_index: usize,
    max_index: usize,
    mut out: &mut Cursor<Vec<u8>>,
) -> UnitResult {
    let mut code_bits = 0_u64;
    let mut code_bit_count = 0_u64;

    let mut frequency_index = min_index;
    while frequency_index <= max_index {
        // TODO slice iteration?
        let code_length = length(frequencies[frequency_index]);

        if code_length == 0 {
            let mut zero_run = 1;

            while frequency_index < max_index && zero_run < LONGEST_LONG_RUN {
                if length(frequencies[frequency_index + 1]) > 0 {
                    break;
                }

                frequency_index += 1;
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

                frequency_index += 1; // we must increment or else this may go
                                      // very wrong
                continue;
            }
        }

        write_bits(6, code_length, &mut code_bits, &mut code_bit_count, &mut out)?;
        frequency_index += 1;
    }

    if code_bit_count > 0 {
        out.write_all(&[(code_bits << (8 - code_bit_count)) as u8])?;
    }

    Ok(())
}

/// Build a "canonical" Huffman code table:
///    - for each (uncompressed) symbol, code contains the length of the
///      corresponding code (in the compressed data)
///    - canonical codes are computed and stored in code
///    - the rules for constructing canonical codes are as follows:
///      * shorter codes (if filled with zeroes to the right) have a numerically
///        higher value than longer codes
///      * for codes with the same length, numerical values increase with
///        numerical symbol values
///    - because the canonical code table can be constructed from symbol lengths
///      alone, the code table can be transmitted without sending the actual
///      code values
///    - see <http://www.compressconsult.com/huffman>/
/// `symbols` lists, ascending, at least every symbol whose code length is
/// non-zero; zero-length entries need not be visited.
fn build_canonical_table(
    code_table: &mut [u64],
    symbols: impl Iterator<Item = usize> + Clone,
) -> UnitResult {
    debug_assert_eq!(code_table.len(), ENCODING_TABLE_SIZE);

    let mut count_per_code = [0_u64; 59];

    for symbol in symbols.clone() {
        count_per_code[u64_to_usize(code_table[symbol], "table entry")?] += 1;
    }

    // For each i from 58 through 1, compute the
    // numerically lowest code with length i, and
    // store that code in n[i].
    {
        let mut code = 0_u64; // TODO use foldr?
        for count in &mut count_per_code.iter_mut().rev() {
            let next_code = (code + *count) >> 1;
            *count = code;
            code = next_code;
        }
    }

    // code[i] contains the length, l, of the
    // code for symbol i.  Assign the next available
    // code of length l to the symbol and store both
    // l and the code in code[i]. // TODO iter + filter ?
    for symbol in symbols {
        let symbol_length = &mut code_table[symbol];
        let current_length = *symbol_length;
        let code_index = u64_to_usize(current_length, "huffman code index")?;
        if current_length > 0 {
            *symbol_length = current_length | (count_per_code[code_index] << 6);
            count_per_code[code_index] += 1;
        }
    }

    Ok(())
}

/// Compute Huffman codes (based on frq input) and store them in frq:
///    - code structure is : [63:lsb - 6:msb] | [5-0: bit length];
///    - max code length is 58 bits;
///    - codes outside the range [im-iM] have a null length (unused values);
///    - original frequencies are destroyed;
///    - encoding tables are used by `hufEncode()` and `hufBuildDecTable()`;
///
/// NB: The following code "(*a == *b) && (a > b))" was added to ensure
///     elements in the heap with the same value are sorted by index.
///     This is to ensure, the STL `make_heap()/pop_heap()/push_heap()` methods
///     produced a resultant sorted heap that is identical across OSes.
fn build_encoding_table(
    frequencies: &mut [u64], // input frequencies, output encoding table
) -> Result<(usize, usize)> // return frequency max min range
{
    debug_assert_eq!(frequencies.len(), ENCODING_TABLE_SIZE);

    /// Frequency with position, used for `MinHeap`.
    #[derive(Eq, PartialEq, Copy, Clone)]
    struct HeapFrequency {
        position: usize,
        frequency: u64,
    }

    impl Ord for HeapFrequency {
        fn cmp(&self, other: &Self) -> Ordering {
            other.frequency.cmp(&self.frequency).then_with(|| other.position.cmp(&self.position))
        }
    }

    impl PartialOrd for HeapFrequency {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    // This function assumes that when it is called, array frq
    // indicates the frequency of all possible symbols in the data
    // that are to be Huffman-encoded.  (frq[i] contains the number
    // of occurrences of symbol i in the data.)
    //
    // The loop below does three things:
    //
    // 1) Finds the minimum and maximum indices that point to non-zero entries
    //    in frq:
    //
    //     frq[im] != 0, and frq[i] == 0 for all i < im
    //     frq[iM] != 0, and frq[i] == 0 for all i > iM
    //
    // 2) Fills array fHeap with pointers to all non-zero entries in frq.
    //
    // 3) Initializes array hlink such that hlink[i] == i for all array entries.

    // We need to use vec here or we overflow the stack.
    let mut links = vec![0_usize; ENCODING_TABLE_SIZE];
    let mut frequency_heap = vec![0_usize; ENCODING_TABLE_SIZE];

    // This is a good solution since we don't have usize::MAX items (no panics
    // or UB), and since this is short-circuit, it stops at the first in
    // order non zero element.
    let min_frequency_index = frequencies.iter().position(|f| *f != 0).unwrap_or(0);

    let mut max_frequency_index = 0;
    let mut frequency_count = 0;

    // assert bounds check to optimize away bounds check in loops
    assert!(links.len() >= ENCODING_TABLE_SIZE);
    assert!(frequencies.len() >= ENCODING_TABLE_SIZE);

    for index in min_frequency_index..ENCODING_TABLE_SIZE {
        links[index] = index; // TODO for x in links.iter().enumerate()

        if frequencies[index] != 0 {
            frequency_heap[frequency_count] = index;
            max_frequency_index = index;
            frequency_count += 1;
        }
    }

    // Add a pseudo-symbol, with a frequency count of 1, to frq;
    // adjust the fHeap and hlink array accordingly.  Function
    // hufEncode() uses the pseudo-symbol for run-length encoding.

    max_frequency_index += 1;
    frequencies[max_frequency_index] = 1;
    frequency_heap[frequency_count] = max_frequency_index;
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

    let mut s_code = vec![0_u64; ENCODING_TABLE_SIZE];

    while frequency_count > 1 {
        // Find the indices, mm and m, of the two smallest non-zero frq
        // values in fHeap, add the smallest frq to the second-smallest
        // frq, and remove the smallest frq value from fHeap.
        let (high_position, low_position) = {
            let smallest_frequency = heap.pop().expect("heap empty bug");
            frequency_count -= 1;

            let mut second_smallest_frequency = heap.peek_mut().expect("heap empty bug");
            second_smallest_frequency.frequency += smallest_frequency.frequency;

            (second_smallest_frequency.position, smallest_frequency.position)
        };

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
        let mut index = high_position; // TODO fold()
        loop {
            s_code[index] += 1;
            debug_assert!(s_code[index] <= 58);

            // merge the two lists
            if links[index] == index {
                links[index] = low_position;
                break;
            }

            index = links[index];
        }

        // Add a bit to all codes in the second list
        let mut index = low_position; // TODO fold()
        loop {
            s_code[index] += 1;
            debug_assert!(s_code[index] <= 58);

            if links[index] == index {
                break;
            }

            index = links[index];
        }
    }

    // Build a canonical Huffman code table, replacing the code
    // lengths in scode with (code, code length) pairs.  Copy the
    // code table from scode into frq.
    build_canonical_table(&mut s_code, 0..ENCODING_TABLE_SIZE)?;
    frequencies.copy_from_slice(&s_code);

    Ok((min_frequency_index, max_frequency_index))
}

#[inline]
const fn length(code: u64) -> u64 {
    code & 63
}
#[inline]
const fn code(code: u64) -> u64 {
    code >> 6
}

const INVALID_BIT_COUNT: &str = "invalid number of bits";
const INVALID_TABLE_ENTRY: &str = "invalid code table entry";
const NOT_ENOUGH_DATA: &str = "decoded data are shorter than expected";
const INVALID_TABLE_SIZE: &str = "unexpected end of code table data";
const TABLE_TOO_LONG: &str = "code table is longer than expected";
const INVALID_CODE: &str = "invalid code";
const TOO_MUCH_DATA: &str = "decoded data are longer than expected";

#[cfg(test)]
mod test {
    use rand::{Rng, SeedableRng};

    use super::*;

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

    const UNCOMPRESSED_ARRAY_SPECIAL: [u16; 100] = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 28182, 0, 65534, 0, 65534, 0, 65534, 0, 65534, 0, 0, 0,
        0, 0, 0, 0, 0, 54124, 13798, 27503, 52154, 61961, 30474, 46880, 39097, 15754, 52897, 42371,
        54053, 14178, 48276, 34591, 42602, 32126, 42062, 31474, 16274, 55991, 2882, 17039, 56389,
        20835, 57057, 54081, 3414, 33957, 52584, 10222, 25139, 40002, 44980, 1602, 48021, 19703,
        6562, 61777, 41582, 201, 31253, 51790, 15888, 40921, 3627, 12184, 16036, 26349, 3159,
        29002, 14535, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 65534, 65534, 65534, 65534, 65534, 65534,
        65534, 65534, 65534,
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
        if rng.gen_bool(0.2) {
            let value = if rng.gen_bool(0.5) {
                0
            } else {
                u16::MAX
            };
            return vec![value; size];
        }

        let mut data = vec![0_u16; size];

        data.iter_mut().for_each(|v| {
            *v = rng.gen_range(0_u16..u16::MAX);
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
        let mut random = rand::rngs::StdRng::from_seed(SEED);
        let raw = fill(&mut random, u16::MAX as usize);

        let compressed = compress(&raw).unwrap();
        let uncompressed = decompress(&compressed, raw.len()).unwrap();

        assert_eq!(uncompressed, raw);
    }

    #[test]
    fn repetitions_special() {
        let raw = UNCOMPRESSED_ARRAY_SPECIAL;

        let compressed = compress(&raw).unwrap();
        let uncompressed = decompress(&compressed, raw.len()).unwrap();

        assert_eq!(uncompressed, raw.to_vec());
    }

    #[test]
    fn round_trip100() {
        let mut random = rand::rngs::StdRng::from_seed(SEED);

        for size_multiplier in 1..10 {
            let raw = fill(&mut random, size_multiplier * 50_000);

            let compressed = compress(&raw).unwrap();
            let uncompressed = decompress(&compressed, raw.len()).unwrap();

            assert_eq!(uncompressed, raw);
        }
    }

    #[test]
    fn test_zeroes() {
        let uncompressed: &[u16] =
            &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        let compressed = compress(uncompressed).unwrap();
        let decompressed = decompress(&compressed, uncompressed.len()).unwrap();

        assert_eq!(uncompressed, decompressed.as_slice());
    }

    /// With no codes of `DECODE_BITS` bits or fewer, every code takes the
    /// long path. Here every symbol has a 16-bit code, so lengths 13 to 15
    /// have none, and symbol 65535's code is sixteen ones. Near the end of the
    /// input the bit reader fills all 64 bits with them, so lengths without
    /// codes must never match, even input bits of all ones.
    #[test]
    fn long_codes_after_unused_lengths_with_all_one_bits() {
        let mut encoding_table = vec![0_u64; ENCODING_TABLE_SIZE];
        encoding_table[..1 << 16].fill(16);
        let symbols: Vec<u32> = (0..1 << 16).collect();
        build_canonical_table(&mut encoding_table, 0..1 << 16).unwrap();
        let decoding_table = build_decoding_table(&encoding_table, &symbols, 1 << 16).unwrap();

        let input = [0xff_u8; 12];
        let decoded = decode_with_tables(&decoding_table, &input, 6 * 16, 1 << 16, 6).unwrap();

        assert_eq!(decoded, [u16::MAX; 6]);
    }

    const SEED: [u8; 32] = [
        12, 155, 32, 34, 112, 109, 98, 54, 12, 255, 32, 34, 112, 109, 98, 55, 12, 155, 32, 34, 12,
        109, 98, 54, 12, 35, 32, 34, 112, 109, 48, 54,
    ];
}
