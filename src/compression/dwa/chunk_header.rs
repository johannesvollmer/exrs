// The fixed chunk leader of every DWA chunk: eleven little-endian u64 counters
// (`DataSizesSingle` in internal_dwa_compressor.h) plus the AC entropy-coder
// selector.

use std::convert::TryInto;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy)]
pub(super) enum AcCompression {
    StaticHuffman,
    Deflate,
}

impl AcCompression {
    fn as_counter(self) -> u64 {
        match self {
            AcCompression::StaticHuffman => 0,
            AcCompression::Deflate => 1,
        }
    }
}

/// The 11 little-endian u64 counters at the start of every DWA chunk
/// (`DataSizesSingle` in internal_dwa_compressor.h), in on-disk order.
pub(super) struct DwaHeader {
    pub(super) version: u64,
    pub(super) unknown_uncompressed_size: usize,
    pub(super) unknown_compressed_size: usize,
    pub(super) ac_compressed_size: usize,
    pub(super) dc_compressed_size: usize,
    pub(super) rle_compressed_size: usize,
    pub(super) rle_uncompressed_size: usize,
    pub(super) rle_raw_size: usize,
    pub(super) ac_count: usize,
    pub(super) dc_count: usize,
    pub(super) ac_compression: AcCompression,
}

impl DwaHeader {
    pub(super) fn parse(input: &mut &[u8]) -> Result<Self> {
        // The chunk leader is a fixed set of 11 little-endian counters.
        // The decoder intentionally rejects truncated or out-of-range values
        // before it looks at any of the payload sections.
        fn counter(input: &mut &[u8]) -> Result<u64> {
            let (bytes, rest) = input
                .split_first_chunk::<8>()
                .ok_or_else(|| Error::invalid("truncated DWA header"))?;
            *input = rest;
            Ok(u64::from_le_bytes(*bytes))
        }

        // the C parser rejects counters with the top bit set
        fn size(value: u64) -> Result<usize> {
            if value > (i64::MAX as u64) {
                return Err(Error::invalid("DWA counter out of range"));
            }
            value.try_into().map_err(|_| Error::invalid("DWA counter out of range"))
        }

        Ok(Self {
            version: counter(input)?,
            unknown_uncompressed_size: size(counter(input)?)?,
            unknown_compressed_size: size(counter(input)?)?,
            ac_compressed_size: size(counter(input)?)?,
            dc_compressed_size: size(counter(input)?)?,
            rle_compressed_size: size(counter(input)?)?,
            rle_uncompressed_size: size(counter(input)?)?,
            rle_raw_size: size(counter(input)?)?,
            ac_count: size(counter(input)?)?,
            dc_count: size(counter(input)?)?,
            ac_compression: match counter(input)? {
                0 => AcCompression::StaticHuffman,
                1 => AcCompression::Deflate,
                _ => {
                    return Err(Error::invalid("unknown DWA AC compression mode"));
                }
            },
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) {
        // Keep the on-disk layout identical to the decoder's parse order.
        let counters = [
            self.version,
            self.unknown_uncompressed_size as u64,
            self.unknown_compressed_size as u64,
            self.ac_compressed_size as u64,
            self.dc_compressed_size as u64,
            self.rle_compressed_size as u64,
            self.rle_uncompressed_size as u64,
            self.rle_raw_size as u64,
            self.ac_count as u64,
            self.dc_count as u64,
            self.ac_compression.as_counter(),
        ];

        for counter in counters {
            out.extend_from_slice(&counter.to_le_bytes());
        }
    }
}
