//! Run-Length Encoding for AC coefficients in DWAA/DWAB compression.

use super::constants::{rle_markers, AC_COUNT};
use crate::error::Result;

/// Decode RLE-compressed AC coefficients
///
/// The AC coefficients are stored in zigzag order with run-length encoding:
/// - Non-zero values are stored directly
/// - Runs of zeros are encoded with markers (0xff00 | run_length)
/// - End of block is marked with 0xff00
///
/// # Arguments
/// * `encoded` - RLE-encoded AC coefficients (in zigzag order, excluding DC)
///
/// # Returns
/// 63 AC coefficients in zigzag order, or an error if decoding fails
pub fn decode_ac_coefficients(encoded: &[u16]) -> Result<[u16; AC_COUNT]> {
    let mut decoded = [0u16; AC_COUNT];
    let mut output_idx = 0;
    let mut input_idx = 0;

    while output_idx < AC_COUNT && input_idx < encoded.len() {
        let value = encoded[input_idx];
        input_idx += 1;

        if rle_markers::is_end_of_block(value) {
            // Rest of the block is zeros
            break;
        } else if rle_markers::is_zero_run(value) {
            // Run of zeros
            let run_length = rle_markers::get_run_length(value);

            // Bounds check
            if output_idx + run_length > AC_COUNT {
                return Err(crate::error::Error::invalid(
                    "RLE decode: zero run exceeds block size"
                ));
            }

            // Zeros are already in the array (initialized to 0)
            output_idx += run_length;
        } else {
            // Regular value
            if output_idx >= AC_COUNT {
                return Err(crate::error::Error::invalid(
                    "RLE decode: output index exceeds block size"
                ));
            }

            decoded[output_idx] = value;
            output_idx += 1;
        }
    }

    // Rest of the coefficients remain zero (already initialized)
    Ok(decoded)
}

/// Find the index of the last non-zero coefficient in zigzag order
///
/// Returns 0 if all AC coefficients are zero (DC-only block)
///
/// # Arguments
/// * `coeffs` - AC coefficients in zigzag order
///
/// # Returns
/// Index of the last non-zero coefficient (0-62), or 0 if all are zero
#[inline]
pub fn find_last_non_zero(coeffs: &[u16; AC_COUNT]) -> usize {
    for i in (0..AC_COUNT).rev() {
        if coeffs[i] != 0 {
            return i + 1; // +1 because we're looking at AC coefficients (DC is index 0)
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_empty_block() {
        // End of block marker only
        let encoded = [rle_markers::END_OF_BLOCK];
        let decoded = decode_ac_coefficients(&encoded).unwrap();

        // All should be zero
        for &coeff in &decoded {
            assert_eq!(coeff, 0);
        }
    }

    #[test]
    fn test_decode_single_value() {
        // One non-zero value followed by end marker
        let encoded = [42u16, rle_markers::END_OF_BLOCK];
        let decoded = decode_ac_coefficients(&encoded).unwrap();

        assert_eq!(decoded[0], 42);
        for &coeff in &decoded[1..] {
            assert_eq!(coeff, 0);
        }
    }

    #[test]
    fn test_decode_zero_run() {
        // Value, then 10 zeros, then another value
        let encoded = [
            100u16,
            rle_markers::make_zero_run(10),
            200u16,
            rle_markers::END_OF_BLOCK,
        ];
        let decoded = decode_ac_coefficients(&encoded).unwrap();

        assert_eq!(decoded[0], 100);
        for i in 1..=10 {
            assert_eq!(decoded[i], 0, "Index {} should be 0", i);
        }
        assert_eq!(decoded[11], 200);
    }

    #[test]
    fn test_decode_multiple_values() {
        // Several non-zero values
        let encoded = [1u16, 2, 3, 4, 5, rle_markers::END_OF_BLOCK];
        let decoded = decode_ac_coefficients(&encoded).unwrap();

        assert_eq!(decoded[0], 1);
        assert_eq!(decoded[1], 2);
        assert_eq!(decoded[2], 3);
        assert_eq!(decoded[3], 4);
        assert_eq!(decoded[4], 5);
    }

    #[test]
    fn test_decode_without_end_marker() {
        // If there's no end marker, rest should be zero
        let encoded = [42u16, 43];
        let decoded = decode_ac_coefficients(&encoded).unwrap();

        assert_eq!(decoded[0], 42);
        assert_eq!(decoded[1], 43);
        for &coeff in &decoded[2..] {
            assert_eq!(coeff, 0);
        }
    }

    #[test]
    fn test_decode_error_zero_run_too_long() {
        // Zero run that would exceed block size
        let encoded = [rle_markers::make_zero_run(100)];
        let result = decode_ac_coefficients(&encoded);

        assert!(result.is_err());
    }

    #[test]
    fn test_find_last_non_zero_empty() {
        let coeffs = [0u16; AC_COUNT];
        assert_eq!(find_last_non_zero(&coeffs), 0);
    }

    #[test]
    fn test_find_last_non_zero_first() {
        let mut coeffs = [0u16; AC_COUNT];
        coeffs[0] = 42;
        assert_eq!(find_last_non_zero(&coeffs), 1);
    }

    #[test]
    fn test_find_last_non_zero_last() {
        let mut coeffs = [0u16; AC_COUNT];
        coeffs[AC_COUNT - 1] = 42;
        assert_eq!(find_last_non_zero(&coeffs), AC_COUNT);
    }

    #[test]
    fn test_find_last_non_zero_middle() {
        let mut coeffs = [0u16; AC_COUNT];
        coeffs[10] = 42;
        assert_eq!(find_last_non_zero(&coeffs), 11);
    }

    #[test]
    fn test_rle_markers() {
        // Test marker creation and identification
        assert!(rle_markers::is_end_of_block(rle_markers::END_OF_BLOCK));
        assert!(!rle_markers::is_zero_run(rle_markers::END_OF_BLOCK));

        let run_marker = rle_markers::make_zero_run(42);
        assert!(!rle_markers::is_end_of_block(run_marker));
        assert!(rle_markers::is_zero_run(run_marker));
        assert_eq!(rle_markers::get_run_length(run_marker), 42);
    }
}
