//! Constants and lookup tables for DWAA/DWAB compression.
//!
//! Based on the OpenEXR reference implementation:
//! https://github.com/AcademySoftwareFoundation/openexr

/// DCT block size (8x8 pixels)
pub const BLOCK_SIZE: usize = 8;
pub const BLOCK_AREA: usize = BLOCK_SIZE * BLOCK_SIZE;

/// Number of AC coefficients (excluding DC)
pub const AC_COUNT: usize = BLOCK_AREA - 1;

/// Default compression level (45.0 is the standard default)
pub const DEFAULT_COMPRESSION_LEVEL: f32 = 45.0;

/// Normalized JPEG quantization table for Y (luminance) channel
/// Based on JPEG standard, normalized by dividing by the minimum value (10)
pub const QUANT_TABLE_Y: [f32; BLOCK_AREA] = [
    1.6, 1.1, 1.0, 1.1, 1.4, 1.6, 2.3, 3.0,
    1.1, 1.2, 1.2, 1.4, 1.6, 2.3, 3.0, 3.4,
    1.0, 1.2, 1.5, 1.6, 2.3, 3.0, 3.4, 3.0,
    1.1, 1.4, 1.6, 2.3, 3.0, 3.4, 3.0, 2.5,
    1.4, 1.6, 2.3, 3.0, 3.4, 3.0, 2.5, 2.1,
    1.6, 2.3, 3.0, 3.4, 3.0, 2.5, 2.1, 1.7,
    2.3, 3.0, 3.4, 3.0, 2.5, 2.1, 1.7, 1.5,
    3.0, 3.4, 3.0, 2.5, 2.1, 1.7, 1.5, 1.3,
];

/// Normalized JPEG quantization table for CbCr (chrominance) channels
/// Based on JPEG standard, normalized by dividing by the minimum value (17)
pub const QUANT_TABLE_CBCR: [f32; BLOCK_AREA] = [
    1.0, 1.0, 1.0, 2.0, 3.5, 3.5, 3.5, 3.5,
    1.0, 1.0, 1.2, 2.6, 3.5, 3.5, 3.5, 3.5,
    1.0, 1.2, 2.2, 3.5, 3.5, 3.5, 3.5, 3.5,
    2.0, 2.6, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5,
    3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5,
    3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5,
    3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5,
    3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5,
];

/// Zigzag order for 8x8 DCT coefficients used by DWAA/DWAB.
/// This matches OpenEXR's `inv_remap` table from `quantizeCoeffAndZigXDR`.
/// Maps row-major index -> zigzag index.
pub const ZIGZAG_ORDER: [usize; BLOCK_AREA] = [
     0,  1,  5,  6, 14, 15, 27, 28,
     2,  4,  7, 13, 16, 26, 29, 42,
     3,  8, 12, 17, 25, 30, 41, 43,
     9, 11, 18, 24, 31, 40, 44, 53,
    10, 19, 23, 32, 39, 45, 52, 54,
    20, 22, 33, 38, 46, 51, 55, 60,
    21, 34, 37, 47, 50, 56, 59, 61,
    35, 36, 48, 49, 57, 58, 62, 63,
];

/// Inverse zigzag order (zigzag-ordered index to normal order)
pub const INVERSE_ZIGZAG_ORDER: [usize; BLOCK_AREA] = {
    let mut inv = [0; BLOCK_AREA];
    let mut i = 0;
    while i < BLOCK_AREA {
        inv[ZIGZAG_ORDER[i]] = i;
        i += 1;
    }
    inv
};

/// ITU-R BT.709 color space conversion matrices
/// Used for RGB to Y'CbCr conversion

/// Forward transform coefficients (RGB to Y'CbCr)
pub mod csc_forward {
    /// Y' coefficients
    pub const Y_R: f32 = 0.2126;
    pub const Y_G: f32 = 0.7152;
    pub const Y_B: f32 = 0.0722;

    /// Cb coefficients
    pub const CB_R: f32 = -0.1146;
    pub const CB_G: f32 = -0.3854;
    pub const CB_B: f32 = 0.5;

    /// Cr coefficients
    pub const CR_R: f32 = 0.5;
    pub const CR_G: f32 = -0.4542;
    pub const CR_B: f32 = -0.0458;
}

/// Inverse transform coefficients (Y'CbCr to RGB)
pub mod csc_inverse {
    /// R' coefficients
    pub const R_Y: f32 = 1.0;
    pub const R_CB: f32 = 0.0;
    pub const R_CR: f32 = 1.5747;

    /// G' coefficients
    pub const G_Y: f32 = 1.0;
    pub const G_CB: f32 = -0.1873;
    pub const G_CR: f32 = -0.4682;

    /// B' coefficients
    pub const B_Y: f32 = 1.0;
    pub const B_CB: f32 = 1.8556;
    pub const B_CR: f32 = 0.0;
}

/// Special RLE markers for AC coefficients
pub mod rle_markers {
    /// End of block marker
    pub const END_OF_BLOCK: u16 = 0xff00;

    /// Check if a value is an end-of-block marker
    #[inline]
    pub fn is_end_of_block(value: u16) -> bool {
        value == END_OF_BLOCK
    }

    /// Check if a value is a zero-run marker
    #[inline]
    pub fn is_zero_run(value: u16) -> bool {
        value >= END_OF_BLOCK && value != END_OF_BLOCK
    }

    /// Extract run length from a zero-run marker
    #[inline]
    pub fn get_run_length(marker: u16) -> usize {
        (marker & 0xff) as usize
    }

    /// Create a zero-run marker
    #[inline]
    pub fn make_zero_run(length: usize) -> u16 {
        END_OF_BLOCK | (length as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zigzag_inverse_zigzag() {
        // Verify that inverse zigzag is correct
        for i in 0..BLOCK_AREA {
            let zigzag_idx = ZIGZAG_ORDER[i];
            let original_idx = INVERSE_ZIGZAG_ORDER[zigzag_idx];
            assert_eq!(original_idx, i);
        }
    }

    #[test]
    fn test_rle_markers() {
        use rle_markers::*;

        assert!(is_end_of_block(END_OF_BLOCK));
        assert!(!is_zero_run(END_OF_BLOCK));

        let run_marker = make_zero_run(42);
        assert!(is_zero_run(run_marker));
        assert_eq!(get_run_length(run_marker), 42);
    }
}
