// The DWA perceptual transfer curve and its inverse, plus the two 64K-entry
// half-float lookup tables the encoder and decoder use to apply them: linear
// values are stored nonlinearly before the DCT and converted back afterwards.

use std::sync::OnceLock;

use half::f16;

pub(super) fn to_nonlinear_table() -> &'static [u16; 65536] {
    static TABLE: OnceLock<[u16; 65536]> = OnceLock::new();

    TABLE.get_or_init(|| {
        // Build the forward transfer table lazily so the hot path stays cheap
        // once the table is initialized.
        let mut table = [0u16; 65536];
        for (bits, slot) in table.iter_mut().enumerate() {
            *slot = dwa_convert_to_nonlinear(f16::from_bits(bits as u16)).to_bits();
        }
        table
    })
}

fn dwa_convert_to_nonlinear(x: f16) -> f16 {
    // Inverse of the decoder's nonlinear -> linear transfer.
    // Values <= 1 use a power curve; values above 1 follow the exponential tail.
    let value = x.to_f32();
    if !value.is_finite() {
        return f16::ZERO;
    }

    let sign = if value < 0.0 {
        -1.0
    } else {
        1.0
    };
    let value = value.abs();

    let nonlinear = if value <= 1.0 {
        value.powf(1.0 / 2.2)
    } else {
        1.0 + value.ln() / 9.02501329156_f32.ln()
    };

    f16::from_f32(sign * nonlinear)
}

/// The stored nonlinear --> linear lookup table for all half bit patterns
pub(super) fn to_linear_table() -> &'static [u16; 65536] {
    static TABLE: OnceLock<[u16; 65536]> = OnceLock::new();

    TABLE.get_or_init(|| {
        // Build the nonlinear -> linear transfer table lazily and reuse it
        // across every chunk.
        let mut table = [0u16; 65536];
        for (bits, slot) in table.iter_mut().enumerate() {
            *slot = dwa_convert_to_linear(f16::from_bits(bits as u16)).to_bits();
        }
        table
    })
}

fn dwa_convert_to_linear(x: f16) -> f16 {
    // Inverse of the encoder's nonlinear transfer.
    let value = x.to_f32();
    if !value.is_finite() {
        return f16::ZERO;
    }

    let sign = if value < 0.0 {
        -1.0
    } else {
        1.0
    };
    let value = value.abs();

    let linear = if value <= 1.0 {
        value.powf(2.2)
    } else {
        // exp(2.2) ^ (value - 1) == exp(2.2 * (value - 1))
        (9.02501329156_f32).powf(value - 1.0)
    };

    f16::from_f32(sign * linear)
}
