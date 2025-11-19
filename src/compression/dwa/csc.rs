//! Color Space Conversion (CSC) for DWAA/DWAB compression.
//!
//! Uses ITU-R BT.709 standard for RGB to Y'CbCr conversion.
//! The zero point is shifted so that Cb=Cr=0 for black (R=G=B=0),
//! rather than the traditional Cb=Cr=0.5.

use super::constants::csc_inverse;

/// Convert Y'CbCr to RGB (inverse transform)
/// Used during decompression.
///
/// # Arguments
/// * `y` - Y' (luminance) channel value
/// * `cb` - Cb (blue-difference) channel value
/// * `cr` - Cr (red-difference) channel value
///
/// # Returns
/// (R', G', B') tuple in perceptual space
#[inline]
pub fn ycbcr_to_rgb(y: f32, cb: f32, cr: f32) -> (f32, f32, f32) {
    let r = y + csc_inverse::R_CR * cr;
    let g = y + csc_inverse::G_CB * cb + csc_inverse::G_CR * cr;
    let b = y + csc_inverse::B_CB * cb;

    (r, g, b)
}

/// Convert Y'CbCr block to RGB block (inverse transform)
/// Processes 64 pixels (8x8 block) at once.
///
/// # Arguments
/// * `y_block` - Y' channel block (64 values)
/// * `cb_block` - Cb channel block (64 values)
/// * `cr_block` - Cr channel block (64 values)
///
/// # Returns
/// (R, G, B) blocks as separate arrays
pub fn ycbcr_block_to_rgb(
    y_block: &[f32; 64],
    cb_block: &[f32; 64],
    cr_block: &[f32; 64],
) -> ([f32; 64], [f32; 64], [f32; 64]) {
    let mut r_block = [0.0f32; 64];
    let mut g_block = [0.0f32; 64];
    let mut b_block = [0.0f32; 64];

    for i in 0..64 {
        let (r, g, b) = ycbcr_to_rgb(y_block[i], cb_block[i], cr_block[i]);
        r_block[i] = r;
        g_block[i] = g;
        b_block[i] = b;
    }

    (r_block, g_block, b_block)
}

#[cfg(test)]
mod tests {
    use super::{super::constants::csc_forward, *};

    /// Forward transform (RGB to Y'CbCr) - for testing only
    #[inline]
    fn rgb_to_ycbcr(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let y = csc_forward::Y_R * r + csc_forward::Y_G * g + csc_forward::Y_B * b;
        let cb = csc_forward::CB_R * r + csc_forward::CB_G * g + csc_forward::CB_B * b;
        let cr = csc_forward::CR_R * r + csc_forward::CR_G * g + csc_forward::CR_B * b;

        (y, cb, cr)
    }

    #[test]
    fn test_csc_roundtrip() {
        let test_colors = [
            (0.0, 0.0, 0.0), // Black
            (1.0, 1.0, 1.0), // White
            (1.0, 0.0, 0.0), // Red
            (0.0, 1.0, 0.0), // Green
            (0.0, 0.0, 1.0), // Blue
            (0.5, 0.5, 0.5), // Gray
            (1.0, 1.0, 0.0), // Yellow
            (1.0, 0.0, 1.0), // Magenta
            (0.0, 1.0, 1.0), // Cyan
        ];

        for &(r, g, b) in &test_colors {
            let (y, cb, cr) = rgb_to_ycbcr(r, g, b);
            let (r2, g2, b2) = ycbcr_to_rgb(y, cb, cr);

            let error_r = (r - r2).abs();
            let error_g = (g - g2).abs();
            let error_b = (b - b2).abs();

            assert!(
                error_r < 1e-4,
                "R roundtrip failed: {} -> {} (Y={}, Cb={}, Cr={})",
                r,
                r2,
                y,
                cb,
                cr
            );
            assert!(
                error_g < 1e-4,
                "G roundtrip failed: {} -> {} (Y={}, Cb={}, Cr={})",
                g,
                g2,
                y,
                cb,
                cr
            );
            assert!(
                error_b < 1e-4,
                "B roundtrip failed: {} -> {} (Y={}, Cb={}, Cr={})",
                b,
                b2,
                y,
                cb,
                cr
            );
        }
    }

    #[test]
    fn test_black_has_zero_chroma() {
        let (y, cb, cr) = rgb_to_ycbcr(0.0, 0.0, 0.0);

        assert_eq!(y, 0.0, "Black should have Y=0");
        assert_eq!(cb, 0.0, "Black should have Cb=0 (shifted zero point)");
        assert_eq!(cr, 0.0, "Black should have Cr=0 (shifted zero point)");
    }

    #[test]
    fn test_white_has_max_luminance() {
        let (y, cb, cr) = rgb_to_ycbcr(1.0, 1.0, 1.0);

        assert!((y - 1.0).abs() < 1e-5, "White should have Y≈1");
        assert!(cb.abs() < 1e-5, "White should have Cb≈0");
        assert!(cr.abs() < 1e-5, "White should have Cr≈0");
    }

    #[test]
    fn test_ycbcr_block_conversion() {
        // Create test blocks with a gradient
        let mut y_block = [0.0f32; 64];
        let mut cb_block = [0.0f32; 64];
        let mut cr_block = [0.0f32; 64];

        for i in 0..64 {
            let value = i as f32 / 63.0;
            y_block[i] = value;
            cb_block[i] = 0.0;
            cr_block[i] = 0.0;
        }

        let (r_block, g_block, b_block) = ycbcr_block_to_rgb(&y_block, &cb_block, &cr_block);

        // For Y with Cb=Cr=0, RGB should equal Y
        for i in 0..64 {
            let expected = y_block[i];
            assert!((r_block[i] - expected).abs() < 1e-5);
            assert!((g_block[i] - expected).abs() < 1e-5);
            assert!((b_block[i] - expected).abs() < 1e-5);
        }
    }

    #[test]
    fn test_bt709_coefficients() {
        // Verify that the BT.709 coefficients sum to 1.0 for Y
        let sum = csc_forward::Y_R + csc_forward::Y_G + csc_forward::Y_B;
        assert!((sum - 1.0).abs() < 1e-6, "Y coefficients should sum to 1.0");

        // Verify that Cb and Cr are zero for gray
        let gray_value = 0.5;
        let (y, cb, cr) = rgb_to_ycbcr(gray_value, gray_value, gray_value);

        assert!((y - gray_value).abs() < 1e-5, "Gray Y should equal input");
        assert!(cb.abs() < 1e-5, "Gray should have Cb=0");
        assert!(cr.abs() < 1e-5, "Gray should have Cr=0");
    }
}
