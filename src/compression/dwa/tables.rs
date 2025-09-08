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
pub(crate) fn parse_huffman_table(_data: &[u8], _class: TableClass) -> Result<HuffTable> {
    // Will be implemented when porting OpenEXR's table format.
    Err(Error::unsupported("DWA Huffman table parsing not yet implemented"))
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
pub(crate) fn decode_symbol(br: &mut BitReader, canon: &CanonicalHuff) -> Result<u8> {
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
