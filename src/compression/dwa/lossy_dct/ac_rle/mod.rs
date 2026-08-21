// AC run-length (de)coding for the lossy DCT: `rle_ac` writes the encoder's
// token stream, `un_rle_ac` reads it back. Operates purely on zig-zag-order
// `[u16; 64]` blocks -- see `quantization.rs` for the zig-zag scatter/gather
// that sits on either side of this in the real encode/decode pipeline.

use super::PackedStream;
use crate::error::{Error, Result};

/// `position` starts at 1 and increases by at least 1 per token (a run of 0
/// maps to +64, ending the loop immediately), so one block never reads more
/// than 63 tokens -- the threshold `un_rle_ac` checks up front to take its
/// branchless fast path.
pub(super) const MAX_TOKENS_PER_BLOCK: usize = 63;

pub(super) fn rle_ac(block: &[u16; 64], ac: &mut Vec<u16>) {
    // The AC stream uses a simple token format: literals are emitted as-is,
    // and runs of zeroes are encoded as 0xffxx tokens. 0xff00 marks EOB.
    let mut dct_comp = 1;

    while dct_comp < 64 {
        if block[dct_comp] != 0 {
            ac.push(block[dct_comp]);
            dct_comp += 1;
            continue;
        }

        let run_len = zero_run_length_at(block, dct_comp);

        if run_len == 1 {
            ac.push(block[dct_comp]);
        } else if run_len + dct_comp == 64 {
            ac.push(0xff00);
        } else {
            ac.push(0xff00 | run_len as u16);
        }

        dct_comp += run_len;
    }
}

/// Consecutive zeros at `block[start..64]`. Caller requires `block[start] ==
/// 0`, so the result is always `>= 1`.
///
/// Wordwise (SWAR): packs four little-endian `u16`s into a `u64` and uses
/// `trailing_zeros` for the first non-zero lane — same shape as
/// `rle::run_length_at`'s byte scan, just 16-bit lanes against an implicit
/// zero target (no broadcast/XOR needed).
#[inline(always)]
fn zero_run_length_at(block: &[u16; 64], start: usize) -> usize {
    debug_assert!(start < 64);
    debug_assert_eq!(block[start], 0);

    let region = &block[start..64];
    let mut count = 0usize;
    let mut chunks = region.chunks_exact(4);
    for chunk in &mut chunks {
        let word = u64::from(chunk[0])
            | (u64::from(chunk[1]) << 16)
            | (u64::from(chunk[2]) << 32)
            | (u64::from(chunk[3]) << 48);
        if word == 0 {
            count += 4;
        } else {
            return count + (word.trailing_zeros() / 16) as usize;
        }
    }
    for &value in chunks.remainder() {
        if value != 0 {
            return count;
        }
        count += 1;
    }
    count
}

/// Un-RLE one 8x8 block of AC values into block[1..]
/// (`LossyDctDecoder_unRleAc`): a value with high byte 0xff encodes a run
/// of `low byte` zeros (0 meaning "rest of the block"); anything else is a
/// literal. Returns the index of the last non-zero value, 0 if none
pub(super) fn un_rle_ac(ac: &mut PackedStream<'_>, block: &mut [u16; 64]) -> Result<usize> {
    // DWA AC values use the same compact token format the encoder writes:
    // 0xffxx means a zero run, and 0xff00 means end-of-block.

    // When at least MAX_TOKENS_PER_BLOCK values remain in the stream, check
    // the sub-slice bound once up front and decode branchless: the
    // run-vs-literal split is unpredictable content, so a real `if`/`else`
    // here mispredicts often (measured 3x its share of the whole decode's
    // branch-misses on real DWA content). `decode_token_branchless` replaces
    // it with select-based bitmasks; the only branch left is the loop exit
    // on `position`. ~2-3x faster than the branching version on realistic
    // block content -- see `dwa-un-rle-ac-branchless-findings`.
    let mut last_non_zero = 0;
    let mut position = 1;

    if ac.remaining() >= MAX_TOKENS_PER_BLOCK {
        let fast = ac.peek_slice(MAX_TOKENS_PER_BLOCK);
        let mut consumed = 0;

        while position < 64 {
            let value = fast[consumed];
            consumed += 1;

            // `block` is caller-zeroed (both call sites use `[0u16; 64]`), so
            // writing 0 into an already-zero slot on a run token is cheaper
            // than a taken/not-taken store branch when the mix is
            // unpredictable.
            let write_pos = position;
            let is_run_mask = decode_token_branchless(value, &mut position, &mut last_non_zero);
            block[write_pos] = value & !(is_run_mask as u16);
        }

        ac.advance(consumed);
    } else {
        while position < 64 {
            let value = ac.next().ok_or_else(|| Error::invalid("truncated DWA AC data"))?;

            if (value & 0xff00) == 0xff00 {
                // run of zeros - the block is pre-zeroed, just skip ahead
                let count = (value & 0xff) as usize;
                position += if count == 0 {
                    64
                } else {
                    count
                };
            } else {
                last_non_zero = position;
                block[position] = value;
                position += 1;
            }
        }
    }

    Ok(last_non_zero)
}

/// One decode step for `un_rle_ac`'s fast path: classifies `value` as
/// run-vs-literal, advances `position`, and folds it into `last_non_zero` --
/// all without a content-dependent branch (see `un_rle_ac`'s comment for
/// why). Returns an all-1s (run) / all-0s (literal) `usize` mask so the
/// caller can select its own masked store.
#[inline(always)]
pub(super) fn decode_token_branchless(
    value: u16,
    position: &mut usize,
    last_non_zero: &mut usize,
) -> usize {
    let is_run = (value & 0xff00) == 0xff00;
    let count = (value & 0xff) as usize;
    // count == 0 means EOB (+64); else the low byte is the run length.
    let run_advance = count + (((count == 0) as usize) << 6);
    let is_run_mask = (is_run as usize).wrapping_neg();
    let advance = (run_advance & is_run_mask) | (1 & !is_run_mask);

    let literal_mask = !is_run_mask;
    *last_non_zero = (*position & literal_mask) | (*last_non_zero & is_run_mask);
    *position += advance;

    is_run_mask
}

#[cfg(test)]
mod test {
    use rand::{RngExt, SeedableRng};

    use super::*;

    const SEED: [u8; 32] = [
        250, 77, 33, 7, 42, 13, 200, 176, 22, 5, 66, 100, 19, 240, 8, 91, 3, 128, 9, 44, 201, 17,
        88, 6, 255, 61, 30, 11, 2, 121, 99, 1,
    ];

    /// Run-length-encode an AC block, decode it back, and require the AC
    /// coefficients (indices 1..64) to be recovered exactly. Index 0 (DC) is
    /// not part of the AC stream, so it is kept zero on both sides.
    fn assert_ac_roundtrips(block: [u16; 64]) {
        let mut ac = Vec::new();
        rle_ac(&block, &mut ac);

        let mut stream = PackedStream::new(&ac);
        let mut decoded = [0u16; 64];
        un_rle_ac(&mut stream, &mut decoded).unwrap();

        assert_eq!(decoded, block);
    }

    /// `zero_run_length_at` must match a naive byte-at-a-time zero scan for
    /// every start index that is itself zero (the only call sites in `rle_ac`).
    #[test]
    fn zero_run_length_at_matches_naive_scan() {
        fn naive(block: &[u16; 64], start: usize) -> usize {
            let mut len = 1;
            while start + len < 64 && block[start + len] == 0 {
                len += 1;
            }
            len
        }

        let mut random = rand::rngs::StdRng::from_seed(SEED);
        for _ in 0..512 {
            let mut block = [0u16; 64];
            for slot in block.iter_mut().skip(1) {
                *slot = if random.random_bool(0.35) {
                    0
                } else {
                    random.random_range(1..=0xfeff)
                };
            }
            for start in 1..64 {
                if block[start] != 0 {
                    continue;
                }
                assert_eq!(
                    zero_run_length_at(&block, start),
                    naive(&block, start),
                    "start={start}"
                );
            }
        }
    }

    #[test]
    fn ac_run_length_roundtrip_hardcoded() {
        // All-zero AC (immediate end-of-block).
        assert_ac_roundtrips([0u16; 64]);

        // No zeros at all: every AC coefficient is a literal.
        let mut dense = [0u16; 64];
        for (index, slot) in dense.iter_mut().enumerate().skip(1) {
            *slot = index as u16;
        }
        assert_ac_roundtrips(dense);

        // A mix of literals, an interior zero run, a single isolated zero, and
        // a trailing zero run that ends the block.
        let mut mixed = [0u16; 64];
        mixed[1] = 5;
        // mixed[2..10] stay zero -> interior run
        mixed[10] = 7;
        mixed[11] = 0; // isolated single zero
        mixed[12] = 9;
        // mixed[13..64] stay zero -> trailing run to end
        assert_ac_roundtrips(mixed);
    }

    /// `assert_ac_roundtrips` alone rarely reaches `un_rle_ac`'s branchless
    /// fast path: a single block's own tokens are almost always fewer than
    /// `MAX_TOKENS_PER_BLOCK` (63), so `ac.remaining()` stays under the fast
    /// path's threshold and only the (unchanged) slow tail loop runs. Pad
    /// with trailing end-of-block tokens so `remaining() >= 63` and the fast
    /// path is what actually gets exercised.
    fn assert_ac_roundtrips_fast_path(block: [u16; 64]) {
        let mut ac = Vec::new();
        rle_ac(&block, &mut ac);
        for _ in 0..64 {
            ac.push(0xff00);
        }

        let mut stream = PackedStream::new(&ac);
        let mut decoded = [0u16; 64];
        un_rle_ac(&mut stream, &mut decoded).unwrap();

        assert_eq!(decoded, block);
    }

    #[test]
    fn ac_run_length_roundtrip_fast_path_random() {
        // Covers the exact-63-token literal boundary (nonzero_probability =
        // 1.0: 63 literals fill positions 1..64 with no run tokens at all)
        // and the single-run all-zero special case (0.0: one "rest of
        // block" token), plus a spread of realistic in-between densities.
        let mut random = rand::rngs::StdRng::from_seed(SEED);
        for &nonzero_probability in &[0.0, 0.05, 0.3, 0.7, 1.0] {
            for _ in 0..200 {
                let mut block = [0u16; 64];
                for slot in block.iter_mut().skip(1) {
                    *slot = if random.random_bool(nonzero_probability) {
                        random.random_range(1..=0xfeff)
                    } else {
                        0
                    };
                }
                assert_ac_roundtrips_fast_path(block);
            }
        }
    }

    #[test]
    fn ac_run_length_roundtrip_seeded() {
        let mut random = rand::rngs::StdRng::from_seed(SEED);

        for _ in 0..64 {
            let mut block = [0u16; 64];
            for slot in block.iter_mut().skip(1) {
                // ~30% zeros to exercise runs; non-zero literals must stay out
                // of the 0xff00..=0xffff token range the format reserves for
                // zero-run markers.
                *slot = if random.random_bool(0.3) {
                    0
                } else {
                    random.random_range(1..=0xfeff)
                };
            }
            assert_ac_roundtrips(block);
        }
    }
}
