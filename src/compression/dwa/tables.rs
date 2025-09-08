//! DWA tables (Huffman/codebooks) scaffolding.
//! Placeholder structures to prepare for porting OpenEXR's actual tables.

use crate::error::{Error, Result};

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
