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
    /// Total payload length following the header (placeholder; used for bounds)
    pub payload_len: u32,
    /// Version if present (placeholder)
    pub version: u16,
    /// Flags if present (placeholder)
    pub flags: u16,
}

/// Attempt to parse a minimal header from the start of `data`.
/// Returns (header, bytes_consumed) on success.
fn parse_header(data: &[u8]) -> Result<(DwaHeader, usize)> {
    // Expect at least 4 bytes for payload length; optionally 4 more for version/flags.
    if data.len() < 4 {
        return Err(Error::unsupported("DWA bitstream too short for header"));
    }

    let payload_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);

    let mut consumed = 4usize;
    let mut version: u16 = 0;
    let mut flags: u16 = 0;

    if data.len() >= consumed + 2 {
        version = u16::from_le_bytes([data[consumed], data[consumed + 1]]);
        consumed += 2;
    }
    if data.len() >= consumed + 2 {
        flags = u16::from_le_bytes([data[consumed], data[consumed + 1]]);
        consumed += 2;
    }

    // Basic bounds sanity: payload_len cannot exceed available bytes after header
    if (payload_len as usize) > data.len().saturating_sub(consumed) {
        // Not invalid, just unsupported until full implementation; include details
        return Err(Error::unsupported("DWA header payload length exceeds available data"));
    }

    Ok((DwaHeader { payload_len, version, flags }, consumed))
}

/// Types of DWA tables we expect to parse (scaffold only)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DwaTableKind {
    DcLut,
    AcLut,
    HuffmanCodes,
}

/// Placeholder for codebook/table parsing.
/// Format (stub): [u8 kind][u32 le length][length bytes of table data]
fn parse_codebooks(payload: &[u8]) -> Result<(/*bytes_consumed*/usize, DwaTableKind)> {
    if payload.is_empty() {
        return Err(Error::unsupported("DWA: no payload for codebooks"));
    }

    let mut offset = 0usize;
    let kind = match payload.get(offset) {
        Some(0) => DwaTableKind::DcLut,
        Some(1) => DwaTableKind::AcLut,
        Some(2) => DwaTableKind::HuffmanCodes,
        Some(_) => DwaTableKind::AcLut, // default to AC for unknown tag
        None => return Err(Error::unsupported("DWA: truncated codebook kind")),
    };
    offset += 1;

    if payload.len() < offset + 4 {
        return Err(Error::unsupported("DWA: truncated codebook length"));
    }

    let len_bytes = [payload[offset], payload[offset + 1], payload[offset + 2], payload[offset + 3]];
    let table_len = u32::from_le_bytes(len_bytes) as usize;
    offset += 4;

    // Consume table bytes, but clamp to available to stay safe in WIP
    let available = payload.len().saturating_sub(offset);
    let take = core::cmp::min(table_len, available);
    offset += take;

    Ok((offset, kind))
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

    // Parse (stub) codebooks next (within the declared payload if present)
    let payload_end = consumed + (hdr.payload_len as usize);
    let payload_end = core::cmp::min(payload_end, compressed_le.len());
    let payload = &compressed_le[consumed..payload_end];

    // Iterate across payload parsing multiple codebook segments
    let mut offset = 0usize;
    let mut kinds: Vec<DwaTableKind> = Vec::new();
    while offset < payload.len() {
        match parse_codebooks(&payload[offset..]) {
            Ok((cb_used, kind)) => {
                if cb_used == 0 { break; } // safety: avoid infinite loop on malformed data
                kinds.push(kind);
                offset = offset.saturating_add(cb_used);
            }
            Err(err) => {
                // Surface the detailed unsupported/invalid reason
                return Err(err);
            }
        }
    }

    let total_consumed = consumed + offset;

    // For now, still not decoding image data. Provide detailed NotSupported message.
    Err(Error::unsupported(format!(
        "DWA header parsed (payload_len={}, version={}, flags={}); parsed {} table(s) {:?} ({} bytes total); remaining payload={} of {}",
        hdr.payload_len,
        hdr.version,
        hdr.flags,
        kinds.len(),
        kinds,
        offset,
        compressed_le.len().saturating_sub(total_consumed),
        compressed_le.len().saturating_sub(consumed)
    )))
}
