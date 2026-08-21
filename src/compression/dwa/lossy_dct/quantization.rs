// Coefficient (de)quantization for the lossy DCT: the JPEG-derived tolerance
// tables, the quantize-and-scatter-to-zig-zag encode step, and the inverse
// zig-zag gather. The zig-zag scatter (`INV_REMAP`) and gather
// (`SRC_INDICES`) permutations are exact inverses of each other. AC
// run-length (de)coding lives in the sibling `ac_rle` module.

use half::f16;

use super::half_float_quantizer::algo_quantize;

/// JPEG-style zig-zag order for an 8x8 DCT block: index `i` is the zig-zag
/// position of DCT-order coefficient `i` (and, symmetrically, the DCT-order
/// position of zig-zag coefficient `i`) — this permutation is used as both a
/// scatter (encode) and gather (decode) table.
pub(super) const ZIGZAG_ORDER: [usize; 64] = [
    0, 1, 5, 6, 14, 15, 27, 28, 2, 4, 7, 13, 16, 26, 29, 42, 3, 8, 12, 17, 25, 30, 41, 43, 9, 11,
    18, 24, 31, 40, 44, 53, 10, 19, 23, 32, 39, 45, 52, 54, 20, 22, 33, 38, 46, 51, 55, 60, 21, 34,
    37, 47, 50, 56, 59, 61, 35, 36, 48, 49, 57, 58, 62, 63,
];

pub(super) struct QuantTables {
    pub(super) y: [f32; 64],
    pub(super) half_y: [u16; 64],
    pub(super) cbcr: [f32; 64],
    pub(super) half_cbcr: [u16; 64],
}

impl QuantTables {
    pub(super) fn new(quant_base_error: f32) -> Self {
        // JPEG-style tables, normalized by their minimum entry and scaled by
        // the configured DWA base error.
        const JPEG_Y: [i32; 64] = [
            16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57,
            69, 56, 14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55,
            64, 81, 104, 113, 92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100,
            103, 99,
        ];
        const JPEG_CBCR: [i32; 64] = [
            17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99,
            99, 99, 47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
            99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
        ];

        let quant_base_error = quant_base_error.max(0.0);
        let mut y = [0.0; 64];
        let mut half_y = [0; 64];
        let mut cbcr = [0.0; 64];
        let mut half_cbcr = [0; 64];

        for index in 0..64 {
            y[index] = quant_base_error * JPEG_Y[index] as f32 / 10.0;
            half_y[index] = f16::from_f32(y[index]).to_bits();
            cbcr[index] = quant_base_error * JPEG_CBCR[index] as f32 / 17.0;
            half_cbcr[index] = f16::from_f32(cbcr[index]).to_bits();
        }

        Self {
            y,
            half_y,
            cbcr,
            half_cbcr,
        }
    }
}

pub(super) fn quantize_coefficients_to_zigzag(
    dct_values: &[f32; 64],
    tolerances: &[f32; 64],
    half_tolerances: &[u16; 64],
) -> [u16; 64] {
    // Quantize in DCT order, then scatter into the stored zig-zag layout.
    let mut half_zig = [0u16; 64];
    for i in 0..64 {
        let src = f16::from_f32(dct_values[i]).to_bits();
        let quantized = algo_quantize(
            src as u32,
            half_tolerances[i] as u32,
            tolerances[i],
            f16::from_bits(src).to_f32(),
        );
        half_zig[ZIGZAG_ORDER[i]] = quantized as u16;
    }
    half_zig
}

/// Undo the zig-zag coefficient order (C "fromHalfZigZag_scalar"),
/// converting half bits to f32.
pub(super) fn from_half_zigzag(zig_zag: &[u16; 64], dst: &mut [f32; 64]) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if super::x86::try_from_half_zigzag(zig_zag, dst) {
        return;
    }

    from_half_zigzag_scalar(zig_zag, dst);
}

/// Scalar half->f32 zig-zag undo, with no SIMD tier dispatch. This is what
/// `from_half_zigzag` falls back to when no faster tier is available, and is
/// also used directly wherever a comparison needs to be independent of which
/// SIMD tier the host happens to have
pub(super) fn from_half_zigzag_scalar(zig_zag: &[u16; 64], dst: &mut [f32; 64]) {
    // The encoder stores coefficients in zig-zag order; the inverse DCT needs
    // normal 8x8 raster order.
    for (slot, &src_index) in dst.iter_mut().zip(ZIGZAG_ORDER.iter()) {
        *slot = f16::from_bits(zig_zag[src_index]).to_f32();
    }
}
