// Y'CbCr -> R'G'B' inverse conversion for DWA, using the modified 709
// coefficients OpenEXR's DWA encoder uses (from OpenEXRCore internal_dwa_simd.h).

/// Input comp0/1/2 are Y, RY, BY; output is R, G, B.
#[inline]
pub fn csc709_inverse(comp0: f32, comp1: f32, comp2: f32) -> (f32, f32, f32) {
    let r = comp0 + 1.5747 * comp2;
    let g = comp0 - 0.1873 * comp1 - 0.4682 * comp2;
    let b = comp0 + 1.8556 * comp1;
    (r, g, b)
}
