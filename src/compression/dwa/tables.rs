//! DWA tables (Huffman/codebooks) scaffolding.
//! Placeholder structures to prepare for porting OpenEXR's actual tables.

use crate::error::{Error, Result};
use super::helpers::BitReader;

#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub(crate) struct HuffTable {
    /// Number of codes for each bit length 1..=16 (JPEG-style canonical Huffman)
    pub counts_per_len: [u8; 16],
    /// Symbols in canonical order
    pub symbols: Vec<u8>,
}

#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TableClass {
    Dc,
    Ac,
}

#[allow(dead_code)]
pub(crate) fn parse_huffman_table(data: &[u8], _class: TableClass) -> Result<HuffTable> {
    // Expect 16 bytes of code counts per length (1..=16), followed by that many symbols.
    if data.len() < 16 {
        return Err(Error::invalid("huffman table too short (counts)"));
    }

    let mut counts_per_len = [0u8; 16];
    counts_per_len.copy_from_slice(&data[0..16]);

    let total_symbols: usize = counts_per_len.iter().map(|&c| c as usize).sum();
    let symbols_start = 16usize;
    let symbols_end = symbols_start.saturating_add(total_symbols);
    if data.len() < symbols_end {
        return Err(Error::invalid("huffman table too short (symbols)"));
    }

    let symbols = data[symbols_start..symbols_end].to_vec();

    Ok(HuffTable { counts_per_len, symbols })
}

/// Canonical Huffman decode scaffolding derived from a HuffTable.
/// This does not perform bit I/O; it only prepares lookup parameters.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct CanonicalHuff {
    /// First canonical code for each bit length index 0..=16 (index 0 is unused)
    pub first_code: [u16; 17],
    /// Starting symbol index in `symbols` for each bit length (index 0 unused)
    pub first_symbol_index: [u16; 17],
    /// Maximum code length present in table (0 if table empty)
    pub max_bits: u8,
    /// Symbols in canonical order (copied from HuffTable)
    pub symbols: Vec<u8>,
}

#[allow(dead_code)]
pub(crate) fn build_canonical(table: &HuffTable) -> CanonicalHuff {
    let mut first_code = [0u16; 17];
    let mut first_symbol_index = [0u16; 17];

    // counts_per_len is for lengths 1..=16
    let mut code: u16 = 0;
    let mut prev_count: u16 = 0;

    // prefix sum for symbol indices
    let mut accum: u16 = 0;
    let mut max_bits: u8 = 0;

    for bits in 1..=16 {
        let count = table.counts_per_len[(bits - 1) as usize] as u16;
        if count != 0 { max_bits = bits as u8; }

        // Next code for this length is (previous code + previous count) << 1
        code = (code + prev_count) << 1;
        first_code[bits as usize] = code;

        first_symbol_index[bits as usize] = accum;
        accum = accum.wrapping_add(count);

        prev_count = count;
    }

    CanonicalHuff {
        first_code,
        first_symbol_index,
        max_bits,
        symbols: table.symbols.clone(),
    }
}


#[allow(dead_code)]
pub(crate) fn decode_symbol(br: &mut BitReader<'_>, canon: &CanonicalHuff) -> Result<u8> {
    if canon.max_bits == 0 { return Err(Error::unsupported("empty huffman table")); }

    let mut code: u16 = 0;
    for len in 1..=canon.max_bits {
        let bit = br.read_bit().ok_or_else(|| Error::invalid("bitstream truncated (huff)"))? as u16;
        code = (code << 1) | bit;

        let first = canon.first_code[len as usize];
        let idx0 = canon.first_symbol_index[len as usize];
        let count = if len < 16 {
            canon.first_symbol_index[(len + 1) as usize].saturating_sub(idx0)
        } else {
            // last length: compute count from symbols length
            (canon.symbols.len() as u16).saturating_sub(idx0)
        };

        if count == 0 { continue; }
        if code >= first {
            let offset = code - first;
            if offset < count {
                let idx = (idx0 + offset) as usize;
                return canon.symbols.get(idx)
                    .copied()
                    .ok_or_else(|| Error::invalid("huffman index OOB"));
            }
        }
    }

    Err(Error::invalid("no huffman code matched"))
}

/// Parse multiple Huffman tables from a compact buffer format used for tests:
/// [u8 count] then repeated tables, each as 16 count bytes followed by that many symbols.
#[allow(dead_code)]
pub(crate) fn parse_many_huffman_tables(data: &[u8]) -> Result<Vec<HuffTable>> {
    if data.is_empty() { return Err(Error::invalid("huffman tables: empty buffer")); }
    let count = data[0] as usize;
    let mut off = 1usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if data.len() < off + 16 { return Err(Error::invalid("huffman table too short (counts)")); }
        let mut counts_per_len = [0u8; 16];
        counts_per_len.copy_from_slice(&data[off..off + 16]);
        off += 16;
        let total_symbols: usize = counts_per_len.iter().map(|&c| c as usize).sum();
        if data.len() < off + total_symbols { return Err(Error::invalid("huffman table too short (symbols)")); }
        let symbols = data[off..off + total_symbols].to_vec();
        off += total_symbols;
        out.push(HuffTable { counts_per_len, symbols });
    }
    Ok(out)
}

/// Fast 8-bit prefix decode assist. For prefixes up to 8 bits, map to (symbol, len) if uniquely decodable.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct FastHuff8 {
    /// For each 8-bit prefix, contains Some((symbol, code_len)) if a full code ends within 8 bits; None otherwise.
    lut: [Option<(u8, u8)>; 256],
    /// Maximum code length in bits
    max_bits: u8,
}

#[allow(dead_code)]
pub(crate) fn build_fast8(canon: &CanonicalHuff) -> FastHuff8 {
    let mut lut: [Option<(u8, u8)>; 256] = [None; 256];
    let max_bits = canon.max_bits;
    // For each code length L and each symbol range at that length, fill all 8-bit prefixes that start with that code
    for len in 1..=max_bits.min(8) {
        let first = canon.first_code[len as usize] as u32;
        let idx0 = canon.first_symbol_index[len as usize] as u32;
        let count = if len < 16 {
            (canon.first_symbol_index[(len + 1) as usize] as u32).saturating_sub(idx0)
        } else {
            (canon.symbols.len() as u32).saturating_sub(idx0)
        };
        if count == 0 { continue; }
        for i in 0..count {
            let code = first + i; // canonical code value of length len
            // expand to 8 bits by appending all suffixes of length (8-len)
            let prefix = code << (8 - len as u32);
            let sym = canon.symbols[(idx0 + i) as usize];
            let fill = 1u32 << (8 - len as u32);
            for s in 0..fill {
                let idx = (prefix | s) as usize;
                // Only set if not already set (shorter codes take precedence)
                if lut[idx].is_none() {
                    lut[idx] = Some((sym, len));
                }
            }
        }
    }
    FastHuff8 { lut, max_bits }
}

#[allow(dead_code)]
pub(crate) fn decode_symbol_fast(br: &mut BitReader<'_>, canon: &CanonicalHuff, fast: &FastHuff8) -> Result<u8> {
    if fast.max_bits == 0 { return Err(Error::unsupported("empty huffman table")); }
    // Try fast path only if we have at least 8 bits buffered; otherwise, use slow path progressively
    let pref = if br.remaining_bits() >= 8 { br.peek_bits(8).unwrap() as u8 } else { 0 };
    if br.remaining_bits() >= 1 {
        if br.remaining_bits() >= 8 {
            if let Some((sym, len)) = fast.lut[pref as usize] {
                // consume len bits and return
                br.skip_bits(len as usize);
                return Ok(sym);
            }
        }
        // Fallback to slow canonical step-by-step decode
        return decode_symbol(br, canon);
    }
    Err(Error::invalid("bitstream truncated (fast huff)"))
}


#[cfg(test)]
mod tests {
    use super::*;

    fn make_toy_table() -> CanonicalHuff {
        // One code of length 1: symbol 0 ("A"), and two codes of length 2: symbols 1 ("B"), 2 ("C").
        let mut ht = HuffTable::default();
        ht.counts_per_len = [0;16];
        ht.counts_per_len[0] = 1; // length 1
        ht.counts_per_len[1] = 2; // length 2
        ht.symbols = vec![0u8, 1u8, 2u8];
        build_canonical(&ht)
    }

    // Pack bits MSB-first into a Vec<u8>
    fn pack_bits_msb_first(bits: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cur: u8 = 0;
        let mut n: u8 = 0;
        for &b in bits {
            cur = (cur << 1) | (b & 1);
            n += 1;
            if n == 8 { out.push(cur); cur = 0; n = 0; }
        }
        if n != 0 { out.push(cur << (8 - n)); }
        out
    }

    #[test]
    fn canonical_decode_toy_table() {
        let canon = make_toy_table();
        // Canonical codes derived:
        // len1: first_code=0 => code '0' => sym 0
        // len2: first_code=(0+1)<<1 = 2 => codes '10' => sym 1, '11' => sym 2
        let bits = [0, 1,0, 1,1]; // 0, 10, 11
        let bytes = pack_bits_msb_first(&bits);
        let mut br = BitReader::new(&bytes);
        let a = decode_symbol(&mut br, &canon).unwrap();
        let b = decode_symbol(&mut br, &canon).unwrap();
        let c = decode_symbol(&mut br, &canon).unwrap();
        assert_eq!((a,b,c), (0,1,2));
    }
}


#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parse_simple_huff_table() {
        // counts: 1 code of length 1, 2 codes of length 2, rest zero.
        let mut buf = vec![0u8; 16];
        buf[0] = 1; // len 1
        buf[1] = 2; // len 2
        // symbols follow: 3 symbols total
        buf.extend_from_slice(&[10u8, 20u8, 30u8]);

        let ht = parse_huffman_table(&buf, TableClass::Ac).expect("parse ok");
        assert_eq!(ht.counts_per_len[0], 1);
        assert_eq!(ht.counts_per_len[1], 2);
        assert_eq!(ht.symbols, vec![10u8, 20u8, 30u8]);

        let canon = build_canonical(&ht);
        assert!(canon.max_bits >= 2);
        assert_eq!(canon.symbols.len(), 3);
    }

    #[test]
    fn parse_short_errors() {
        assert!(parse_huffman_table(&[], TableClass::Dc).is_err());
        let mut buf = vec![0u8; 16];
        buf[0] = 1; // needs 1 symbol, but provide none
        assert!(parse_huffman_table(&buf, TableClass::Dc).is_err());
    }
}


#[cfg(test)]
mod fast_and_many_tests {
    use super::*;

    // Reuse packer from the other tests by redefining here for this module
    fn pack_bits_msb_first(bits: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cur: u8 = 0;
        let mut n: u8 = 0;
        for &b in bits {
            cur = (cur << 1) | (b & 1);
            n += 1;
            if n == 8 { out.push(cur); cur = 0; n = 0; }
        }
        if n != 0 { out.push(cur << (8 - n)); }
        out
    }

    fn make_toy_table() -> CanonicalHuff {
        // One code of length 1: symbol 0, two codes of length 2: symbols 1 and 2
        let mut ht = HuffTable::default();
        ht.counts_per_len = [0;16];
        ht.counts_per_len[0] = 1; // 1 of length 1
        ht.counts_per_len[1] = 2; // 2 of length 2
        ht.symbols = vec![0u8, 1u8, 2u8];
        build_canonical(&ht)
    }

    #[test]
    fn parse_many_two_tables() {
        // Build buffer: count=2, then table1 (len1:1 symbol 42), table2 (len2:1 symbol 99)
        let mut buf = Vec::new();
        buf.push(2u8);
        // table 1 counts
        let mut counts = [0u8;16]; counts[0] = 1; // one len-1 code
        buf.extend_from_slice(&counts);
        buf.push(42u8); // one symbol
        // table 2 counts
        let mut counts2 = [0u8;16]; counts2[1] = 1; // one len-2 code
        buf.extend_from_slice(&counts2);
        buf.push(99u8);

        let v = parse_many_huffman_tables(&buf).expect("parse many ok");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].symbols, vec![42u8]);
        assert_eq!(v[1].symbols, vec![99u8]);
    }

    #[test]
    fn fast8_matches_slow() {
        let canon = make_toy_table();
        let fast = build_fast8(&canon);
        // Sequence: 0, 10, 11
        let bits = [0, 1,0, 1,1];
        let bytes = pack_bits_msb_first(&bits);
        let mut br_fast = BitReader::new(&bytes);
        let mut br_slow = BitReader::new(&bytes);
        let a_fast = decode_symbol_fast(&mut br_fast, &canon, &fast).unwrap();
        let a_slow = decode_symbol(&mut br_slow, &canon).unwrap();
        assert_eq!(a_fast, a_slow);
        let b_fast = decode_symbol_fast(&mut br_fast, &canon, &fast).unwrap();
        let b_slow = decode_symbol(&mut br_slow, &canon).unwrap();
        assert_eq!(b_fast, b_slow);
        let c_fast = decode_symbol_fast(&mut br_fast, &canon, &fast).unwrap();
        let c_slow = decode_symbol(&mut br_slow, &canon).unwrap();
        assert_eq!(c_fast, c_slow);
    }
}
