//! Nonlinear transform for perceptually uniform quantization.
//!
//! DWAA/DWAB use a perceptual color space to ensure quantization errors
//! are distributed evenly in terms of human perception rather than
//! linear light values.

use half::f16;

/// Forward nonlinear transform (linear to perceptual space)
/// For compression: converts linear light values to perceptually uniform space
///
/// - For values <= 1.0: Uses power function (gamma 2.2)
/// - For values > 1.0: Uses logarithmic function
/// - Smooth transition at value = 1.0
#[inline]
pub fn to_nonlinear(linear: f32) -> f32 {
    if linear <= 1.0 {
        // Gamma 2.2 for values <= 1.0
        linear.powf(1.0 / 2.2)
    } else {
        // Logarithmic encoding for values > 1.0
        linear.ln() / 2.2f32.ln() + 1.0
    }
}

/// Inverse nonlinear transform (perceptual space to linear)
/// For decompression: converts from perceptual space back to linear light
///
/// - For values <= 1.0: Uses power function (inverse of gamma 2.2)
/// - For values > 1.0: Uses exponential function
#[inline]
pub fn from_nonlinear(nonlinear: f32) -> f32 {
    if nonlinear <= 1.0 {
        // Inverse gamma 2.2
        nonlinear.powf(2.2)
    } else {
        // Exponential (inverse of log)
        (2.2f32.ln() * (nonlinear - 1.0)).exp()
    }
}

/// Lookup table for fast inverse nonlinear transform
/// Uses f16 bit pattern as index (65536 entries)
pub struct InverseNonlinearLut {
    table: Vec<f32>,
}

impl InverseNonlinearLut {
    /// Create a new lookup table
    pub fn new() -> Self {
        let mut table = Vec::with_capacity(65536);

        // Generate lookup table for all possible f16 values
        for bits in 0..65536u16 {
            let half = f16::from_bits(bits);
            let float_value = half.to_f32();

            // Apply inverse nonlinear transform
            let linear_value = if float_value.is_nan() || float_value < 0.0 {
                // Handle NaN and negative values
                0.0
            } else {
                from_nonlinear(float_value)
            };

            table.push(linear_value);
        }

        Self { table }
    }

    /// Look up the linear value for a given half-float
    #[inline]
    pub fn lookup(&self, half: f16) -> f32 {
        let index = half.to_bits() as usize;
        debug_assert!(index < self.table.len());
        // Safe indexing: index is always < 65536 due to u16 range
        self.table[index]
    }

    /// Look up the linear value for a given half-float bits
    #[inline]
    pub fn lookup_bits(&self, bits: u16) -> f32 {
        let index = bits as usize;
        debug_assert!(index < self.table.len());
        // Safe indexing: index is always < 65536 due to u16 range
        self.table[index]
    }
}

impl Default for InverseNonlinearLut {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonlinear_roundtrip() {
        let test_values = [
            0.0, 0.1, 0.5, 0.9, 1.0, 1.1, 2.0, 5.0, 10.0, 100.0,
        ];

        for &value in &test_values {
            let nonlinear = to_nonlinear(value);
            let recovered = from_nonlinear(nonlinear);

            // Allow small floating point error
            let relative_error = ((recovered - value) / value.max(1e-6)).abs();
            assert!(
                relative_error < 1e-5,
                "Roundtrip failed for {}: got {}, relative error {}",
                value,
                recovered,
                relative_error
            );
        }
    }

    #[test]
    fn test_nonlinear_monotonic() {
        // Verify that the transform is monotonically increasing
        let mut prev_linear = 0.0f32;
        let mut prev_nonlinear = to_nonlinear(prev_linear);

        for i in 1..1000 {
            let linear = i as f32 / 10.0;
            let nonlinear = to_nonlinear(linear);

            assert!(
                nonlinear >= prev_nonlinear,
                "Transform is not monotonic at {}: {} -> {}, {} -> {}",
                linear,
                prev_linear,
                prev_nonlinear,
                linear,
                nonlinear
            );

            prev_linear = linear;
            prev_nonlinear = nonlinear;
        }
    }

    #[test]
    fn test_inverse_nonlinear_lut() {
        let lut = InverseNonlinearLut::new();

        // Test some known values
        let test_values = [0.0f32, 0.5, 1.0, 2.0, 10.0];

        for &linear in &test_values {
            let nonlinear_f32 = to_nonlinear(linear);
            let nonlinear_f16 = f16::from_f32(nonlinear_f32);

            // Look up through the table
            let recovered = lut.lookup(nonlinear_f16);

            // Allow for f16 precision loss
            let expected = from_nonlinear(nonlinear_f16.to_f32());
            let error = (recovered - expected).abs();

            assert!(
                error < 1e-6,
                "LUT lookup failed for linear={}, nonlinear={:?}: got {}, expected {}",
                linear,
                nonlinear_f16,
                recovered,
                expected
            );
        }
    }

    #[test]
    fn test_transition_at_one() {
        // Test smooth transition at value = 1.0
        let below = to_nonlinear(0.999);
        let at = to_nonlinear(1.0);
        let above = to_nonlinear(1.001);

        // All should be close to 1.0
        assert!((at - 1.0).abs() < 1e-6);

        // Differences should be small
        assert!((at - below).abs() < 0.01);
        assert!((above - at).abs() < 0.01);
    }
}
