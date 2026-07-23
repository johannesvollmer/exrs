//! 16-bit Huffman compression and decompression.
//! Huffman compression and decompression routines written
//!    by Christian Rouet for his PIZ image file format.
// see https://github.com/AcademySoftwareFoundation/openexr/blob/88246d991e0318c043e6f584f7493da08a31f9f8/OpenEXR/IlmImf/ImfHuf.cpp
//
// Decoder: canonical left-justified Huffman, after Zavadskyi & Kovalchuk,
// "Engineering a Faster Huffman Decoder", DCC 2026,
// DOI 10.1109/dcc66757.2026.00029, https://github.com/reeWorlds/Fast-Large-Huffman
// https://ieeexplore.ieee.org/document/11510463
// - 12-bit prefix LUT (L1-resident), linear scan for longer codes
// - fused base/offset table (paper improvement 1)
// - two-word 64-bit refill, fast region without per-symbol end checks (improvement 3)
// - tables built directly from the packed 6-bit length stream (no 64K intermediate table)
// Same design as OpenEXRs `FastHufDecoder`:
// https://github.com/AcademySoftwareFoundation/openexr/blob/main/src/lib/OpenEXRCore/internal_huf.c

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
    let _table_size = usize::try_from(u32::read_le(&mut remaining_compressed)?)?;
    let bit_count = usize::try_from(u32::read_le(&mut remaining_compressed)?)?;
    let _reserved = u32::read_le(&mut remaining_compressed)?;

    let max_code_index = usize::try_from(max_code_index_32)?;
    if min_code_index >= ENCODING_TABLE_SIZE || max_code_index >= ENCODING_TABLE_SIZE {
        return Err(Error::invalid(INVALID_TABLE_SIZE));
    }

    if RoundingMode::Up.divide(bit_count, 8) > remaining_compressed.len() {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    let decoder =
        CanonicalDecoder::read(&mut remaining_compressed, min_code_index, max_code_index)?;
    if bit_count > 8 * remaining_compressed.len() {
        return Err(Error::invalid(INVALID_BIT_COUNT));
    }

    decoder.decode(remaining_compressed, bit_count, max_code_index_32, expected_size)
}

pub fn compress(uncompressed: &[u16]) -> Result<Vec<u8>> {
    if uncompressed.is_empty() {
        return Ok(vec![]);
    }

    let mut frequencies = count_frequencies(uncompressed);
    let (min_code_index, max_code_index) = build_encoding_table(&mut frequencies)?;

    let mut result = Cursor::new(Vec::with_capacity(uncompressed.len()));
    u32::write_slice_le(&mut result, &[0; 5])?; // we come back to these later after we know more about the compressed data

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

const ENCODING_TABLE_SIZE: usize = ((1 << ENCODE_BITS) + 1) as usize;

const SHORT_ZEROCODE_RUN: u64 = 59;
const LONG_ZEROCODE_RUN: u64 = 63;
const SHORTEST_LONG_RUN: u64 = 2 + LONG_ZEROCODE_RUN - SHORT_ZEROCODE_RUN;
const LONGEST_LONG_RUN: u64 = 255 + SHORTEST_LONG_RUN;

/// Longest possible code, from the 6-bit length fields (run markers start at 59)
const MAX_CODE_LENGTH: usize = 58;
/// Leading buffer bits resolved with a single table lookup;
/// keeps the tables L1-resident (4096 * 5 bytes)
const LUT_BITS: usize = 12;
const LUT_SIZE: usize = 1 << LUT_BITS;

/// Canonical left-justified decoder (see module header for sources).
/// Invariant: in a left-justified 64-bit buffer, the next code's length is
/// the first `l` with `lj_base[l] <= buffer`, and its id is
/// `(buffer >> (64 - l)) + lj_offset[l]`. `LUT_BITS`-bit prefixes resolve
/// with one lookup; longer codes (rare) take the linear scan.
struct CanonicalDecoder {
    max_len: usize,
    /// Smallest code of each length, left-justified in 64 bits;
    /// `u64::MAX` for lengths without codes
    lj_base: [u64; MAX_CODE_LENGTH + 2],
    /// Per length: (id of first code) - (smallest code), wrapping
    lj_offset: [u64; MAX_CODE_LENGTH + 2],
    /// Per length: one past the last valid id, to reject corrupt streams
    end_id: [u64; MAX_CODE_LENGTH + 2],
    /// The symbol of each id; ids sort symbols by (code length, code value)
    id_to_symbol: Vec<u32>,
    /// Resolved symbol per prefix with code length <= `LUT_BITS`;
    /// separate from `lut_length` (a fused table measures slower)
    lut_symbol: [u32; LUT_SIZE],
    /// Code length per prefix; 0 sends the decoder to the long-code scan,
    /// which also rejects prefixes that no code starts with
    lut_length: [u8; LUT_SIZE],
}

impl CanonicalDecoder {
    /// Parse the packed code length table (see `pack_encoding_table`)
    /// directly into decoding tables; no 64K intermediate symbol table.
    fn read(packed: &mut impl Read, min_code_index: usize, max_code_index: usize) -> Result<Self> {
        let mut code_bits = 0_u64;
        let mut code_bit_count = 0_u64;

        // coded symbols in ascending order with their lengths; typically a few K
        let mut symbol_lengths = Vec::with_capacity(1024);
        let mut count_per_length = [0_u64; MAX_CODE_LENGTH + 2];

        let mut code_index = min_code_index;
        while code_index <= max_code_index {
            let code_len = read_bits(6, &mut code_bits, &mut code_bit_count, packed)?;

            if code_len == LONG_ZEROCODE_RUN {
                let zerun_bits = read_bits(8, &mut code_bits, &mut code_bit_count, packed)?;
                let zerun = usize::try_from(zerun_bits + SHORTEST_LONG_RUN).unwrap();

                if code_index + zerun > max_code_index + 1 {
                    return Err(Error::invalid(TABLE_TOO_LONG));
                }

                code_index += zerun;
            } else if code_len >= SHORT_ZEROCODE_RUN {
                let zerun = usize::try_from(code_len - SHORT_ZEROCODE_RUN + 2).unwrap();

                if code_index + zerun > max_code_index + 1 {
                    return Err(Error::invalid(TABLE_TOO_LONG));
                }

                code_index += zerun;
            } else {
                if code_len > 0 {
                    count_per_length[u64_to_usize(code_len, "huffman code length")?] += 1;
                    symbol_lengths.push((u32::try_from(code_index)?, code_len as u8));
                }
                code_index += 1;
            }
        }

        Self::build(&symbol_lengths, &count_per_length)
    }

    fn build(
        symbol_lengths: &[(u32, u8)],
        count_per_length: &[u64; MAX_CODE_LENGTH + 2],
    ) -> Result<Self> {
        // ids sorted by (length, code) => the first id of each length is a prefix sum
        let mut first_id = [0_u64; MAX_CODE_LENGTH + 2];
        let mut symbol_count = 0_u64;
        let mut min_len = 1;
        let mut max_len = 0;
        for len in 1..=MAX_CODE_LENGTH {
            first_id[len] = symbol_count;
            symbol_count += count_per_length[len];
            if count_per_length[len] > 0 {
                if max_len == 0 {
                    min_len = len;
                }
                max_len = len;
            }
        }

        // smallest code per length: same descending fold as `build_canonical_table`,
        // so the codes match the encoder's
        let mut base_code = [u64::MAX; MAX_CODE_LENGTH + 2];
        {
            let mut code = 0_u64;
            for len in (1..=MAX_CODE_LENGTH).rev() {
                if count_per_length[len] > 0 {
                    base_code[len] = code;

                    // over-subscribed lengths (corrupt table) would overflow the code width
                    if (code + count_per_length[len] - 1) >> len != 0 {
                        return Err(Error::invalid(INVALID_TABLE_ENTRY));
                    }
                }
                code = (code + count_per_length[len]) >> 1;
            }
        }

        // ascending code values within a length go to ascending symbols
        let mut next_id = first_id;
        let mut id_to_symbol = vec![0_u32; usize::try_from(symbol_count)?];
        for &(symbol, len) in symbol_lengths {
            let len = usize::from(len);
            let id = u64_to_usize(next_id[len], "huffman symbol id")?;
            next_id[len] += 1;
            *id_to_symbol.get_mut(id).ok_or_else(|| Error::invalid(INVALID_TABLE_ENTRY))? = symbol;
        }

        let mut lj_base = [u64::MAX; MAX_CODE_LENGTH + 2];
        let mut lj_offset = [0_u64; MAX_CODE_LENGTH + 2];
        let mut end_id = [0_u64; MAX_CODE_LENGTH + 2];
        for len in 1..=MAX_CODE_LENGTH {
            if count_per_length[len] > 0 {
                lj_base[len] = base_code[len] << (64 - len);
                lj_offset[len] = first_id[len].wrapping_sub(base_code[len]);
                end_id[len] = first_id[len] + count_per_length[len];
            }
        }

        let mut lut_symbol = [0_u32; LUT_SIZE];
        let mut lut_length = [0_u8; LUT_SIZE];
        for (prefix, (lut_symbol, lut_length)) in
            lut_symbol.iter_mut().zip(lut_length.iter_mut()).enumerate()
        {
            let buffer = (prefix as u64) << (64 - LUT_BITS);

            let mut len = min_len;
            while len <= max_len && lj_base[len] > buffer {
                len += 1;
            }

            // prefix unused or code longer than LUT_BITS: keep the zero marker
            if len > max_len || len > LUT_BITS {
                continue;
            }

            let id = (buffer >> (64 - len)).wrapping_add(lj_offset[len]);
            if id < end_id[len] {
                *lut_symbol = id_to_symbol[id as usize];
                *lut_length = len as u8;
            }
        }

        Ok(Self {
            max_len,
            lj_base,
            lj_offset,
            end_id,
            id_to_symbol,
            lut_symbol,
            lut_length,
        })
    }

    fn decode(
        &self,
        data: &[u8],
        bit_count: usize,
        run_length_code: u32,
        expected_output_size: usize,
    ) -> Result<Vec<u16>> {
        // bitstream as big-endian words, zero-padded so the two-word loads
        // stay in range (incl. a run count read on a corrupt stream)
        let mut words = Vec::with_capacity(data.len() / 8 + 3);
        let mut chunks = data.chunks_exact(8);
        for chunk in &mut chunks {
            words.push(u64::from_be_bytes(<[u8; 8]>::try_from(chunk).expect("chunk size is 8")));
        }
        let remainder = chunks.remainder();
        if !remainder.is_empty() {
            let mut last = [0_u8; 8];
            last[..remainder.len()].copy_from_slice(remainder);
            words.push(u64::from_be_bytes(last));
        }
        words.extend_from_slice(&[0, 0]);

        // index output; the expected-size check doubles as the bounds check
        let mut out = vec![0_u16; expected_output_size];
        let mut out_position = 0_usize;
        let mut position = 0_usize; // stream position in bits

        // Fast region: whole refills before the last bit => no per-symbol
        // end checks; corrupt streams read padding zeros, size checks reject
        let fast_bit_limit = bit_count.saturating_sub(64);

        'refill: while position < fast_bit_limit {
            // next 64 bits, left-justified, from two adjacent words
            let mut buffer = read_word_at(&words, position);

            // unconsumed leading buffer bits; the bit position is only
            // recomputed at refill boundaries (position + 64 - remaining)
            let mut remaining = 64_usize;

            while remaining >= LUT_BITS {
                let prefix = (buffer >> (64 - LUT_BITS)) as usize;
                let mut len = usize::from(self.lut_length[prefix]);

                let symbol = if len != 0 {
                    self.lut_symbol[prefix]
                } else {
                    if remaining < self.max_len {
                        // needs more bits than the buffer holds
                        position += 64 - remaining;
                        continue 'refill;
                    }
                    let (symbol, long_len) = self.resolve_long_code(buffer)?;
                    len = long_len;
                    symbol
                };

                if symbol == run_length_code {
                    // 8-bit repetition count: from the buffer if enough bits, else the stream
                    let count = if remaining >= len + 8 {
                        ((buffer << len) >> 56) as usize
                    } else {
                        (read_word_at(&words, position + (64 - remaining) + len) >> 56) as usize
                    };

                    out_position = extend_with_run(&mut out, out_position, count)?;

                    if remaining >= len + 8 {
                        remaining -= len + 8;
                        buffer = (buffer << 1) << (len + 7); // len + 8 might be all 64 bits
                    } else {
                        position += (64 - remaining) + len + 8;
                        continue 'refill;
                    }
                } else {
                    if out_position >= expected_output_size {
                        return Err(Error::invalid(TOO_MUCH_DATA));
                    }
                    // non-run symbols are < max_code_index < table size; always fit
                    out[out_position] = symbol as u16;
                    out_position += 1;

                    remaining -= len; // len <= LUT_BITS or max_len <= remaining
                    buffer <<= len;
                }
            }

            position += 64 - remaining;
        }

        // Tail: same decoding with per-symbol end checks; vacated zeros
        // match the encoder's zero padding after the final bit
        'tail_refill: while position < bit_count {
            let mut buffer = read_word_at(&words, position);
            let mut remaining = 64_usize;

            while position < bit_count
                && (remaining >= LUT_BITS || position + remaining >= bit_count)
            {
                let prefix = (buffer >> (64 - LUT_BITS)) as usize;
                let mut len = usize::from(self.lut_length[prefix]);

                let symbol = if len != 0 {
                    self.lut_symbol[prefix]
                } else {
                    if remaining < self.max_len && position + remaining < bit_count {
                        continue 'tail_refill; // needs more real bits than remain
                    }
                    let (symbol, long_len) = self.resolve_long_code(buffer)?;
                    len = long_len;
                    symbol
                };

                if symbol == run_length_code {
                    let count = if remaining >= len + 8 {
                        ((buffer << len) >> 56) as usize
                    } else {
                        (read_word_at(&words, position + len) >> 56) as usize
                    };

                    out_position = extend_with_run(&mut out, out_position, count)?;

                    position += len + 8;
                    if remaining >= len + 8 {
                        remaining -= len + 8;
                        buffer = (buffer << 1) << (len + 7);
                    } else {
                        continue 'tail_refill;
                    }
                } else {
                    if out_position >= expected_output_size {
                        return Err(Error::invalid(TOO_MUCH_DATA));
                    }
                    out[out_position] = symbol as u16;
                    out_position += 1;

                    position += len;
                    // len > remaining only on corrupt streams; rejected below
                    remaining = remaining.saturating_sub(len);
                    buffer <<= len;
                }
            }
        }

        if out_position != expected_output_size {
            return Err(Error::invalid(NOT_ENOUGH_DATA));
        }

        Ok(out)
    }

    /// Length and symbol of a code longer than `LUT_BITS` (rare): scan for
    /// the first length with `lj_base <= buffer`; rejects invalid prefixes.
    #[inline(never)]
    fn resolve_long_code(&self, buffer: u64) -> Result<(u32, usize)> {
        let mut len = LUT_BITS + 1;
        while len <= self.max_len && self.lj_base[len] > buffer {
            len += 1;
        }
        if len > self.max_len {
            return Err(Error::invalid(INVALID_CODE));
        }

        let id = (buffer >> (64 - len)).wrapping_add(self.lj_offset[len]);
        if id >= self.end_id[len] {
            return Err(Error::invalid(INVALID_CODE));
        }

        let symbol = *self
            .id_to_symbol
            .get(u64_to_usize(id, "huffman symbol id")?)
            .ok_or_else(|| Error::invalid(INVALID_CODE))?;

        Ok((symbol, len))
    }
}

/// Write a run of the last decoded value, for the run length symbol;
/// returns the output position after the run
#[inline]
fn extend_with_run(out: &mut [u16], position: usize, count: usize) -> Result<usize> {
    if position + count > out.len() {
        return Err(Error::invalid(TOO_MUCH_DATA));
    }

    if position == 0 {
        return Err(Error::invalid(NOT_ENOUGH_DATA));
    }

    let repeated = out[position - 1];
    out[position..position + count].fill(repeated);
    Ok(position + count)
}

/// The 64 bits starting at the given bit position, from two adjacent words.
/// `words` must extend past the last touched bit (guaranteed by the padding
/// in `CanonicalDecoder::decode`); bits past the end read as zero.
#[inline]
fn read_word_at(words: &[u64], bit_position: usize) -> u64 {
    let word_index = bit_position >> 6;
    let bit_shift = bit_position & 63;

    // the second shift is split to avoid an undefined shift by 64 when
    // the position is word-aligned
    (words[word_index] << bit_shift) | ((words[word_index + 1] >> 1) >> (63 - bit_shift))
}

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
        out.write_all(&[(*code_bits >> *code_bit_count) as u8])?;
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

    let data_length = out.position() - start_position; // we shouldn't count the last byte write

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

                frequency_index += 1; // we must increment or else this may go very wrong
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
///
/// `code_table` holds one entry (code length, in bits 0..=5) per used symbol,
/// addressed by compact id, not by the 16-bit symbol value. This table build
/// never allocates or scans the full `ENCODING_TABLE_SIZE` (65537) symbol range.
fn build_canonical_table(code_table: &mut [u64]) -> UnitResult {
    let mut count_per_code = [0_u64; 59];

    for &code in code_table.iter() {
        count_per_code[u64_to_usize(code, "table entry")?] += 1;
    }

    // For each i from 58 through 1, compute the
    // numerically lowest code with length i, and
    // store that code in n[i].
    {
        let mut code = 0_u64;
        for count in &mut count_per_code.iter_mut().rev() {
            let next_code = (code + *count) >> 1;
            *count = code;
            code = next_code;
        }
    }

    // code[i] contains the length, l, of the
    // code for symbol i.  Assign the next available
    // code of length l to the symbol and store both
    // l and the code in code[i].
    for symbol_length in code_table.iter_mut() {
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

    // Huffman symbol loop: collect the (typically few thousand) symbols with
    // non-zero frequency instead of walking and allocating over the full
    // `ENCODING_TABLE_SIZE` (65537) range. `links`/`s_code` below are then
    // addressed by the compact index into `symbols`, not by the 16-bit symbol
    // value, so no 65537-sized table is ever built for the tree merge or the
    // canonical code assignment.
    let mut symbols = Vec::with_capacity(1024);
    for (index, &frequency) in frequencies.iter().enumerate() {
        if frequency != 0 {
            symbols.push(index);
        }
    }

    let min_frequency_index = symbols.first().copied().unwrap_or(0);
    let mut max_frequency_index = symbols.last().copied().unwrap_or(0);

    // Add a pseudo-symbol, with a frequency count of 1, to frq;
    // adjust the symbol list accordingly. Function hufEncode() uses the
    // pseudo-symbol for run-length encoding.
    max_frequency_index += 1;
    frequencies[max_frequency_index] = 1;
    symbols.push(max_frequency_index);

    let frequency_count = symbols.len();
    let mut links: Vec<usize> = (0..frequency_count).collect();

    // Build an array, scode, such that scode[i] contains the number
    // of bits assigned to symbol i.  Conceptually this is done by
    // constructing a tree whose leaves are the symbols with non-zero
    // frequency.
    //
    // The loop below doesn't actually build the tree; instead compute
    // the distances of the leaves from the root on the fly.  When a new
    // node is added to the heap, then that node's descendants are linked
    // into a single linear list that starts at the new node, and the code
    // lengths of the descendants (that is, their distance from the root
    // of the tree) are incremented by one.
    let mut heap = BinaryHeap::with_capacity(frequency_count);
    for (compact_id, &symbol) in symbols.iter().enumerate() {
        heap.push(HeapFrequency {
            position: compact_id,
            frequency: frequencies[symbol],
        });
    }

    let mut s_code = vec![0_u64; frequency_count];
    let mut remaining_count = frequency_count;

    while remaining_count > 1 {
        // Find the indices, mm and m, of the two smallest non-zero frq
        // values in fHeap, add the smallest frq to the second-smallest
        // frq, and remove the smallest frq value from fHeap.
        let (high_position, low_position) = {
            let smallest_frequency = heap.pop().expect("heap empty bug");
            remaining_count -= 1;

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
        let mut index = high_position;
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
        let mut index = low_position;
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
    // lengths in scode with (code, code length) pairs. scode is indexed by
    // compact id
    build_canonical_table(&mut s_code)?;

    frequencies.fill(0);
    for (compact_id, &symbol) in symbols.iter().enumerate() {
        frequencies[symbol] = s_code[compact_id];
    }

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

    const SEED: [u8; 32] = [
        12, 155, 32, 34, 112, 109, 98, 54, 12, 255, 32, 34, 112, 109, 98, 55, 12, 155, 32, 34, 12,
        109, 98, 54, 12, 35, 32, 34, 112, 109, 48, 54,
    ];
}
