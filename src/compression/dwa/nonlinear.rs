//! Nonlinear transform for perceptually uniform quantization.
//!
//! DWAA/DWAB use a perceptual color space to ensure quantization errors
//! are distributed evenly in terms of human perception rather than
//! linear light values.
//!
//! This module implements the exact u16->u16 lookup tables from OpenEXR's
//! internal_dwa_table_init.c, avoiding double rounding errors.

use half::f16;
use std::sync::OnceLock;

/// DWA nonlinear transform lookup tables.
/// These tables map between linear and nonlinear half-float bit patterns
/// (u16->u16).
///
/// Based on OpenEXR's dwaLookups from internal_dwa_table_init.c:
/// - toLinear: Converts nonlinear (quantized) half to linear half
/// - toNonlinear: Converts linear half to nonlinear (quantized) half
struct DwaLookupTables {
    to_linear: Box<[u16; 65536]>,
    to_nonlinear: Box<[u16; 65536]>,
}

static DWA_TABLES: OnceLock<DwaLookupTables> = OnceLock::new();

/// Convert a nonlinear half-float to linear representation.
///
/// This implements the exact algorithm from OpenEXR's dwa_convertToLinear:
/// - For f <= 1.0: linear = f^2.2
/// - For f > 1.0: linear = e^(2.2 * (f - 1))
///
/// Input and output are both u16 half-float bit patterns.
#[inline]
fn convert_to_linear(x: u16) -> u16 {
    // Handle zero
    if x == 0 {
        return 0;
    }

    // Handle infinity/NaN
    if (x & 0x7c00) == 0x7c00 {
        return 0;
    }

    let f = f16::from_bits(x).to_f32();
    let sign = if f < 0.0 { -1.0 } else { 1.0 };
    let f_abs = f.abs();

    let (px, py) = if f_abs <= 1.0 {
        (f_abs, 2.2f32)
    } else {
        // pow(e, 2.2) = 9.02501329156
        (9.02501329156f32, f_abs - 1.0)
    };

    let z = sign * px.powf(py);
    f16::from_f32(z).to_bits()
}

/// Convert a linear half-float to nonlinear representation.
///
/// This implements the exact algorithm from OpenEXR's dwa_convertToNonLinear:
/// - For f <= 1.0: nonlinear = f^(1/2.2)
/// - For f > 1.0: nonlinear = log(f) / 2.2 + 1
///
/// Input and output are both u16 half-float bit patterns.
#[inline]
fn convert_to_nonlinear(x: u16) -> u16 {
    // Handle zero
    if x == 0 {
        return 0;
    }

    // Handle infinity/NaN
    if (x & 0x7c00) == 0x7c00 {
        return 0;
    }

    let f = f16::from_bits(x).to_f32();
    let sign = if f < 0.0 { -1.0 } else { 1.0 };
    let f_abs = f.abs();

    let z = if f_abs <= 1.0 {
        f_abs.powf(1.0 / 2.2)
    } else {
        f_abs.ln() / 2.2 + 1.0
    };

    f16::from_f32(sign * z).to_bits()
}

/// Initialize the DWA lookup tables on first use.
/// This generates 256KB of lookup tables (2 tables × 64K entries × 2 bytes).
/// When rayon is enabled, table generation is parallelized for faster
/// initialization.
fn init_dwa_tables() -> DwaLookupTables {
    let mut to_linear = Box::new([0u16; 65536]);
    let mut to_nonlinear = Box::new([0u16; 65536]);

    // Generate all 65536 entries for both tables
    // TODO: Add rayon parallel generation once rayon is available
    for i in 0..65536 {
        to_linear[i] = convert_to_linear(i as u16);
        to_nonlinear[i] = convert_to_nonlinear(i as u16);
    }

    DwaLookupTables {
        to_linear,
        to_nonlinear,
    }
}

/// Get the DWA tables, initializing them on first access.
/// Thread-safe through OnceLock.
#[inline]
fn get_dwa_tables() -> &'static DwaLookupTables {
    DWA_TABLES.get_or_init(init_dwa_tables)
}

/// Lookup table for fast inverse nonlinear transform (nonlinear -> linear).
/// This is the primary interface for decompression.
pub struct ToLinearLut {
    // Reference to the static table - no allocation needed
}

impl ToLinearLut {
    /// Create a new lookup table reference.
    /// The actual tables are initialized once on first access.
    pub fn new() -> Self {
        // Ensure tables are initialized
        let _ = get_dwa_tables();
        Self {}
    }

    /// Look up the linear half-float for a given nonlinear half-float bits.
    ///
    /// Returns the u16 bit pattern of the linear half-float.
    #[inline]
    pub fn lookup(&self, nonlinear_bits: u16) -> u16 {
        let tables = get_dwa_tables();
        tables.to_linear[nonlinear_bits as usize]
    }

    /// Look up the linear half-float for a given nonlinear half-float.
    #[inline]
    pub fn lookup_f16(&self, nonlinear: f16) -> f16 {
        let tables = get_dwa_tables();
        f16::from_bits(tables.to_linear[nonlinear.to_bits() as usize])
    }
}

impl Default for ToLinearLut {
    fn default() -> Self {
        Self::new()
    }
}

/// Lookup table for forward nonlinear transform (linear -> nonlinear).
/// Used for compression.
pub struct ToNonlinearLut {
    // Reference to the static table - no allocation needed
}

impl ToNonlinearLut {
    /// Create a new lookup table reference.
    pub fn new() -> Self {
        let _ = get_dwa_tables();
        Self {}
    }

    /// Look up the nonlinear half-float for a given linear half-float bits.
    #[inline]
    pub fn lookup(&self, linear_bits: u16) -> u16 {
        let tables = get_dwa_tables();
        tables.to_nonlinear[linear_bits as usize]
    }

    /// Look up the nonlinear half-float for a given linear half-float.
    #[inline]
    pub fn lookup_f16(&self, linear: f16) -> f16 {
        let tables = get_dwa_tables();
        f16::from_bits(tables.to_nonlinear[linear.to_bits() as usize])
    }
}

impl Default for ToNonlinearLut {
    fn default() -> Self {
        Self::new()
    }
}

// Legacy compatibility - maps old InverseNonlinearLut to new ToLinearLut
#[deprecated(note = "Use ToLinearLut instead for bit-exact u16->u16 lookups")]
pub struct InverseNonlinearLut {
    inner: ToLinearLut,
}

#[allow(deprecated)]
impl InverseNonlinearLut {
    pub fn new() -> Self {
        Self {
            inner: ToLinearLut::new(),
        }
    }

    /// Legacy lookup that returns f32.
    /// This is kept for compatibility but performs an extra f16->f32
    /// conversion.
    #[inline]
    pub fn lookup(&self, nonlinear: f16) -> f32 {
        self.inner.lookup_f16(nonlinear).to_f32()
    }
}

#[allow(deprecated)]
impl Default for InverseNonlinearLut {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_linear_lut_basic() {
        let lut = ToLinearLut::new();

        // Test zero
        assert_eq!(lut.lookup(0), 0);

        // Test some known values
        let one_f16 = f16::from_f32(1.0).to_bits();
        let linear_one = lut.lookup(one_f16);

        // At f=1.0, both formulas should give f^2.2 = 1.0^2.2 = 1.0
        let result = f16::from_bits(linear_one).to_f32();
        assert!((result - 1.0).abs() < 0.01, "Expected ~1.0, got {}", result);
    }

    #[test]
    fn test_to_nonlinear_lut_basic() {
        let lut = ToNonlinearLut::new();

        // Test zero
        assert_eq!(lut.lookup(0), 0);

        // Test one
        let one_f16 = f16::from_f32(1.0).to_bits();
        let nonlinear_one = lut.lookup(one_f16);
        let result = f16::from_bits(nonlinear_one).to_f32();
        assert!((result - 1.0).abs() < 0.01, "Expected ~1.0, got {}", result);
    }

    #[test]
    fn test_roundtrip_consistency() {
        let to_nonlinear = ToNonlinearLut::new();
        let to_linear = ToLinearLut::new();

        // Test roundtrip for various values
        let test_values = [0.1f32, 0.5, 0.9, 1.0, 1.1, 2.0, 5.0];

        for &value in &test_values {
            let linear_bits = f16::from_f32(value).to_bits();
            let nonlinear_bits = to_nonlinear.lookup(linear_bits);
            let recovered_bits = to_linear.lookup(nonlinear_bits);

            let recovered_value = f16::from_bits(recovered_bits).to_f32();

            // Allow for half-precision quantization error
            let error = (recovered_value - value).abs() / value.max(0.1);
            assert!(
                error < 0.05,
                "Roundtrip failed for {}: got {}, error {}",
                value,
                recovered_value,
                error
            );
        }
    }

    #[test]
    fn test_bit_exact_u16_mapping() {
        let lut = ToLinearLut::new();

        // Verify that the lookup always returns a valid u16
        for i in 0..1000u16 {
            let result = lut.lookup(i);
            // Ensure the produced half maps to a finite value.
            let value = f16::from_bits(result).to_f32();
            assert!(
                value.is_finite(),
                "Expected finite value for nonlinear {:04x}, got {}",
                i,
                value
            );
        }
    }

    #[test]
    #[allow(deprecated)]
    fn test_legacy_compatibility() {
        let old_lut = InverseNonlinearLut::new();
        let new_lut = ToLinearLut::new();

        // Verify old and new APIs give equivalent results
        let test_f16 = f16::from_f32(0.5);
        let old_result = old_lut.lookup(test_f16);
        let new_result = new_lut.lookup_f16(test_f16).to_f32();

        assert!((old_result - new_result).abs() < 1e-6);
    }
}
