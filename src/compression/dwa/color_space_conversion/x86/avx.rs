// AVX tier: fixed per-element linear combo of three f32 arrays (no cross-lane
// shuffles).

use miraculix::x86::ops::avx::avx::Avx;

#[inline(always)]
fn load(array: &[f32; 64], base: usize) -> [f32; 8] {
    array[base..base + 8].try_into().unwrap()
}

#[inline(always)]
fn store(array: &mut [f32; 64], base: usize, value: [f32; 8]) {
    array[base..base + 8].copy_from_slice(&value);
}

#[cfg(any(feature = "avx2-tests", feature = "simd-benches"))]
pub fn csc709_forward_8x8(avx: Avx, block: &mut [[f32; 64]; 3]) {
    csc709_forward_8x8_batch(avx, std::iter::once(block));
}

// Wrapped in `miraculix::avx_fn!`: the composed `add_f32x8`/`mul_f32x8`/
// `sub_f32x8` chain per chunk needs a shared `#[target_feature]` context to
// inline into real `ymm` code instead of a `callq` chain
miraculix::avx_fn! {
    pub fn csc709_forward_8x8_batch<'a>(avx: Avx, blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
        // OpenEXR's modified 709 coefficients (zero-centered chroma).
        let c_r = [0.2126f32; 8];
        let c_g = [0.7152f32; 8];
        let c_b = [0.0722f32; 8];
        let inv_by = [1.0f32 / 1.8556; 8];
        let inv_ry = [1.0f32 / 1.5747; 8];

        for block in blocks {
            let [r, g, b] = block;
            for chunk in 0..8 {
                let base = chunk * 8;
                let rv = load(r, base);
                let gv = load(g, base);
                let bv = load(b, base);

                let y = avx.add_f32x8(
                    avx.add_f32x8(avx.mul_f32x8(rv, c_r), avx.mul_f32x8(gv, c_g)),
                    avx.mul_f32x8(bv, c_b),
                );
                let by = avx.mul_f32x8(avx.sub_f32x8(bv, y), inv_by);
                let ry = avx.mul_f32x8(avx.sub_f32x8(rv, y), inv_ry);

                store(r, base, y);
                store(g, base, by);
                store(b, base, ry);
            }
        }
    }
}

#[cfg(any(feature = "avx2-tests", feature = "simd-benches"))]
pub fn csc709_inverse_8x8(avx: Avx, block: &mut [[f32; 64]; 3]) {
    csc709_inverse_8x8_batch(avx, std::iter::once(block));
}

/// One 8x8 inverse CSC. The fused lossy-DCT decode path calls this per
/// spatial block while the three component buffers are still L1-hot.
#[inline(always)]
pub(crate) fn inverse_one(avx: Avx, block: &mut [[f32; 64]; 3]) {
    let c_ry = [1.5747f32; 8];
    let c_by_g = [0.1873f32; 8];
    let c_ry_g = [0.4682f32; 8];
    let c_by = [1.8556f32; 8];

    let [comp0, comp1, comp2] = block;
    for chunk in 0..8 {
        let base = chunk * 8;
        let y = load(comp0, base);
        let by = load(comp1, base);
        let ry = load(comp2, base);

        let r = avx.add_f32x8(y, avx.mul_f32x8(ry, c_ry));
        let g =
            avx.sub_f32x8(avx.sub_f32x8(y, avx.mul_f32x8(by, c_by_g)), avx.mul_f32x8(ry, c_ry_g));
        let b = avx.add_f32x8(y, avx.mul_f32x8(by, c_by));

        store(comp0, base, r);
        store(comp1, base, g);
        store(comp2, base, b);
    }
}

// Wrapped in `miraculix::avx_fn!`
miraculix::avx_fn! {
    pub fn csc709_inverse_8x8_batch<'a>(avx: Avx, blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
        for block in blocks {
            inverse_one(avx, block);
        }
    }
}
