// SSE tier: same CSC as avx, 4-wide instead of 8-wide. Base `Sse` only (f32
// arithmetic).

use miraculix::x86::ops::sse::sse::Sse;

#[inline(always)]
fn load(array: &[f32; 64], base: usize) -> [f32; 4] {
    [array[base], array[base + 1], array[base + 2], array[base + 3]]
}

#[inline(always)]
fn store(array: &mut [f32; 64], base: usize, value: [f32; 4]) {
    array[base..base + 4].copy_from_slice(&value);
}

#[cfg(any(feature = "sse2-tests", feature = "simd-benches"))]
pub fn csc709_forward_8x8(sse: Sse, block: &mut [[f32; 64]; 3]) {
    csc709_forward_8x8_batch(sse, std::iter::once(block));
}

pub fn csc709_forward_8x8_batch<'a>(
    sse: Sse,
    blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>,
) {
    // OpenEXR's modified 709 coefficients (zero-centered chroma).
    let c_r = [0.2126f32; 4];
    let c_g = [0.7152f32; 4];
    let c_b = [0.0722f32; 4];
    let inv_by = [1.0f32 / 1.8556; 4];
    let inv_ry = [1.0f32 / 1.5747; 4];

    for block in blocks {
        let [r, g, b] = block;
        for chunk in 0..16 {
            let base = chunk * 4;
            let rv = load(r, base);
            let gv = load(g, base);
            let bv = load(b, base);

            let y = sse.add_f32x4(
                sse.add_f32x4(sse.mul_f32x4(rv, c_r), sse.mul_f32x4(gv, c_g)),
                sse.mul_f32x4(bv, c_b),
            );
            let by = sse.mul_f32x4(sse.sub_f32x4(bv, y), inv_by);
            let ry = sse.mul_f32x4(sse.sub_f32x4(rv, y), inv_ry);

            store(r, base, y);
            store(g, base, by);
            store(b, base, ry);
        }
    }
}

#[cfg(any(feature = "sse2-tests", feature = "simd-benches"))]
pub fn csc709_inverse_8x8(sse: Sse, block: &mut [[f32; 64]; 3]) {
    csc709_inverse_8x8_batch(sse, std::iter::once(block));
}

pub fn csc709_inverse_8x8_batch<'a>(
    sse: Sse,
    blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>,
) {
    for block in blocks {
        inverse_one(sse, block);
    }
}

/// One 8x8 inverse CSC. Extracted from `csc709_inverse_8x8_batch` (mirrors
/// `avx2::inverse_one`) so the SSE fused decode path can call it per spatial
/// block while the three component buffers are still L1-hot.
#[inline(always)]
pub(crate) fn inverse_one(sse: Sse, block: &mut [[f32; 64]; 3]) {
    let c_ry = [1.5747f32; 4];
    let c_by_g = [0.1873f32; 4];
    let c_ry_g = [0.4682f32; 4];
    let c_by = [1.8556f32; 4];

    let [comp0, comp1, comp2] = block;
    for chunk in 0..16 {
        let base = chunk * 4;
        let y = load(comp0, base);
        let by = load(comp1, base);
        let ry = load(comp2, base);

        let r = sse.add_f32x4(y, sse.mul_f32x4(ry, c_ry));
        let g =
            sse.sub_f32x4(sse.sub_f32x4(y, sse.mul_f32x4(by, c_by_g)), sse.mul_f32x4(ry, c_ry_g));
        let b = sse.add_f32x4(y, sse.mul_f32x4(by, c_by));

        store(comp0, base, r);
        store(comp1, base, g);
        store(comp2, base, b);
    }
}
