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
