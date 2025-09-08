//! DWAA/DWAB decoder (ported from OpenEXR Core internal_dwa.c and internal_dwa_decoder.h)
//!
//! This is a work-in-progress implementation. Initial commits provide the structure and
//! function signatures; full decoding will be implemented incrementally.

use crate::compression::ByteVec;
use crate::error::{Error, Result};
use crate::meta::attribute::{ChannelList, IntegerBounds};
use super::helpers::BitReader;

/// Minimal header info scaffold for DWA blocks. The actual format contains
/// multiple tables and chunked data; this struct will expand as we port more.
#[derive(Debug, Default, Clone)]
pub(crate) struct DwaHeader {
    /// Placeholder: number of bytes in the first segment (e.g., AC table)
    pub first_segment_len: u32,
    /// Placeholder flags/version if present
    pub version_or_flags: u16,
}

/// Attempt to parse a minimal header from the start of `data`.
/// Returns (header, bytes_consumed) on success.
fn parse_header(data: &[u8]) -> Result<(DwaHeader, usize)> {
    // For now, perform basic length checks and read a couple of bytes so we
    // can hook up future parsing while keeping tests green.
    if data.len() < 4 {
        return Err(Error::unsupported("DWA bitstream too short for header"));
    }

    // Read a couple of bytes directly (little-endian ordering is expected for EXR blocks)
    let first_segment_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

    let mut consumed = 4usize;
    let mut version_or_flags: u16 = 0;
    if data.len() >= consumed + 2 {
        version_or_flags = u16::from_le_bytes([data[consumed], data[consumed + 1]]);
        consumed += 2;
    }

    Ok((DwaHeader { first_segment_len, version_or_flags }, consumed))
}

/// Types of DWA tables we expect to parse (scaffold only)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DwaTableKind {
    DcLut,
    AcLut,
    HuffmanCodes,
}

/// Placeholder for codebook/table parsing. Consumes some bytes and returns context.
fn parse_codebooks(payload: &[u8]) -> Result<(/*bytes_consumed*/usize, DwaTableKind)> {
    if payload.is_empty() {
        return Err(Error::unsupported("DWA: no payload for codebooks"));
    }
    // For now, pretend we found an AC LUT and consumed min(2, payload.len()) bytes
    let consumed = core::cmp::min(2, payload.len());
    Ok((consumed, DwaTableKind::AcLut))
}

pub(crate) fn decompress(
    _channels: &ChannelList,
    compressed_le: ByteVec,
    _pixel_section: IntegerBounds,
    _expected_byte_size: usize,
    pedantic: bool,
) -> Result<ByteVec> {
    // Begin port: set up bit reader and perform minimal sanity checks.
    let mut br = BitReader::new(&compressed_le);
    // Align to byte boundary in case upstream provided byte-aligned blocks.
    br.align_to_byte();

    // Parse minimal header using raw bytes (before bit-level parsing of tables)
    let (hdr, consumed) = parse_header(&compressed_le)?;

    // If pedantic, ensure there are remaining bytes after the header to parse
    if pedantic && consumed >= compressed_le.len() {
        return Err(Error::invalid("DWA stream has no payload after header"));
    }

    // Parse (stub) codebooks next
    let (cb_consumed, which) = parse_codebooks(&compressed_le[consumed..])?;
    let total_consumed = consumed + cb_consumed;

    // For now, still not decoding image data. Provide detailed NotSupported message.
    Err(Error::unsupported(format!(
        "DWA header parsed (first_segment_len={}, flags={}); parsed {:?} ({} bytes); remaining payload={}",
        hdr.first_segment_len,
        hdr.version_or_flags,
        which,
        cb_consumed,
        compressed_le.len().saturating_sub(total_consumed)
    )))
}
