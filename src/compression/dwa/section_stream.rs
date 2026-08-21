// The four on-disk DWA payload sections (UNKNOWN, AC, DC, RLE): splitting the
// chunk body into them, the zlib inflate wrapper they share, and the
// differencing transform applied to the DC stream.

use super::chunk_header::{AcCompression, DwaHeader};
use crate::error::{Error, Result};

/// Split the data after header + rules into the four sections, in on-disk
/// order. Errors on truncation like the C parser.
pub(super) fn split_sections<'d>(data: &'d [u8], header: &DwaHeader) -> Result<[&'d [u8]; 4]> {
    let mut rest = data;
    let mut take = |length: usize| -> Result<&'d [u8]> {
        if length > rest.len() {
            return Err(Error::invalid("truncated DWA section"));
        }
        let (section, remaining) = rest.split_at(length);
        rest = remaining;
        Ok(section)
    };

    Ok([
        take(header.unknown_compressed_size)?,
        take(header.ac_compressed_size)?,
        take(header.dc_compressed_size)?,
        take(header.rle_compressed_size)?,
    ])
}

fn inflate(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>> {
    let options = zune_inflate::DeflateOptions::default()
        .set_limit(expected_size)
        .set_size_hint(expected_size);

    let inflated = zune_inflate::DeflateDecoder::new_with_options(compressed, options)
        .decode_zlib()
        .map_err(|_| Error::invalid("DWA zlib data malformed"))?;

    if inflated.len() != expected_size {
        return Err(Error::invalid("DWA zlib data size mismatch"));
    }
    Ok(inflated)
}

/// Like `inflate`, but into a reused buffer.
fn inflate_into(compressed: &[u8], expected_size: usize, buffer: &mut Vec<u8>) -> Result<()> {
    if buffer.len() < expected_size {
        buffer.resize(expected_size, 0);
    }

    let mut decompress = flate2::Decompress::new(true);
    let status = decompress
        .decompress(compressed, &mut buffer[..expected_size], flate2::FlushDecompress::Finish)
        .map_err(|_| Error::invalid("DWA zlib data malformed"))?;

    if status != flate2::Status::StreamEnd
        || usize::try_from(decompress.total_out()) != Ok(expected_size)
    {
        return Err(Error::invalid("DWA zlib data size mismatch"));
    }
    Ok(())
}

/// UNKNOWN section: raw (non-DCT-compressible) channel data,
/// zlib-compressed, planar in channel order.
/// Writes into `buffer` so capacity can be reused across chunks.
pub(super) fn decode_unknown_section_into(
    section: &[u8],
    header: &DwaHeader,
    buffer: &mut Vec<u8>,
) -> Result<usize> {
    if header.unknown_uncompressed_size == 0 {
        return Ok(0);
    }
    inflate_into(section, header.unknown_uncompressed_size, buffer)?;
    Ok(header.unknown_uncompressed_size)
}

/// AC section: RLE DCT coefficients as u16, entropy coded with either the
/// PIZ static Huffman coder or zlib.
/// Writes into `out` (and Huffman scratch `words`); returns `header.ac_count`.
pub(super) fn decode_ac_section_into(
    section: &[u8],
    header: &DwaHeader,
    out: &mut Vec<u16>,
    words: &mut Vec<u64>,
) -> Result<usize> {
    if header.ac_count == 0 {
        return Ok(0);
    }

    match header.ac_compression {
        AcCompression::StaticHuffman => {
            crate::compression::huffman::decompress_into(section, header.ac_count, out, words)?;
        }
        AcCompression::Deflate => {
            let bytes = inflate(section, header.ac_count * 2)?;
            if out.len() < header.ac_count {
                out.resize(header.ac_count, 0);
            }
            for (slot, pair) in out[..header.ac_count].iter_mut().zip(bytes.chunks_exact(2)) {
                *slot = u16::from_le_bytes([pair[0], pair[1]]);
            }
        }
    }

    Ok(header.ac_count)
}

/// DC section: one u16 (half bits) per 8x8 block, zlib-compressed after
/// the "zip reconstruct" transform (differencing + byte deinterleave).
pub(super) fn decode_dc_section(section: &[u8], header: &DwaHeader) -> Result<Vec<u16>> {
    if header.dc_count == 0 {
        return Ok(vec![]);
    }

    let mut bytes = inflate(section, header.dc_count * 2)?;
    undo_zip_reconstruct(&mut bytes);
    Ok(bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect())
}

/// RLE section: zlib, then classic byte-oriented RLE. Result is planar per
/// channel, each channel further split into byte planes.
/// Writes into `buffer` so capacity can be reused; returns bytes written.
pub(super) fn decode_rle_section_into(
    section: &[u8],
    header: &DwaHeader,
    max_raw_size: usize,
    buffer: &mut Vec<u8>,
) -> Result<usize> {
    if header.rle_raw_size == 0 {
        return Ok(0);
    }
    // Cap expansion so a corrupt header cannot force a huge allocation.
    if header.rle_raw_size > max_raw_size {
        return Err(Error::invalid("DWA RLE data size"));
    }
    #[cfg(feature = "dwa-profile")]
    let t = super::profile::start();
    let inflated = inflate(section, header.rle_uncompressed_size)?;
    #[cfg(feature = "dwa-profile")]
    t.stop(&super::profile::RLE_INFLATE_NS);

    // Grow only; caller uses `[..written]`, which the unpack fully overwrites.
    #[cfg(feature = "dwa-profile")]
    let t = super::profile::start();
    if buffer.len() < header.rle_raw_size {
        buffer.resize(header.rle_raw_size, 0);
    }
    #[cfg(feature = "dwa-profile")]
    t.stop(&super::profile::RLE_ALLOC_NS);

    #[cfg(feature = "dwa-profile")]
    let t = super::profile::start();
    let written = crate::compression::rle::unpack_rle_tokens_into(
        &inflated,
        &mut buffer[..header.rle_raw_size],
    )?;
    #[cfg(feature = "dwa-profile")]
    t.stop(&super::profile::RLE_UNPACK_NS);

    Ok(written)
}

/// Ports "internal_zip_reconstruct_bytes": undo differencing, then
/// interleave the two buffer halves, in place via `optimize_bytes`.
fn undo_zip_reconstruct(bytes: &mut [u8]) {
    crate::compression::optimize_bytes::differences_to_samples(bytes);
    crate::compression::optimize_bytes::interleave_byte_blocks(bytes);
}

/// Encoder-side companion applied to the DC byte stream before zlib:
/// byte-fragment separation followed by successive differencing.
pub(super) fn zip_deconstruct_bytes(bytes: &mut [u8]) {
    crate::compression::optimize_bytes::separate_bytes_fragments(bytes);
    crate::compression::optimize_bytes::samples_to_differences(bytes);
}

#[cfg(test)]
mod test {
    use rand::{RngExt, SeedableRng};

    use super::*;

    const SEED: [u8; 32] = [
        19, 240, 8, 91, 3, 128, 9, 44, 201, 17, 88, 6, 255, 61, 30, 11, 2, 121, 99, 1, 250, 77, 33,
        7, 42, 13, 200, 176, 22, 5, 66, 100,
    ];

    /// The encoder-side DC transform (`zip_deconstruct_bytes`) and the
    /// decoder-side reconstruction (`undo_zip_reconstruct`) must be inverses,
    /// including for odd lengths and the < 2 byte short-circuit.
    #[test]
    fn zip_deconstruct_reconstruct_roundtrip() {
        let mut random = rand::rngs::StdRng::from_seed(SEED);

        for length in [0usize, 1, 2, 3, 4, 5, 17, 64, 129] {
            let original: Vec<u8> = (0..length).map(|_| random.random()).collect();

            let mut deconstructed = original.clone();
            zip_deconstruct_bytes(&mut deconstructed);
            undo_zip_reconstruct(&mut deconstructed);

            assert_eq!(deconstructed, original, "failed at length {length}");
        }
    }
}
