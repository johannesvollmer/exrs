// SSE2 V1 tier: same per-element linear combination as the avx2 tier, just
// 4-wide instead of 8-wide (16 chunks of the 64-element block instead of 8).

use pulp::{f32x4, x86::V1};

#[inline(always)]
fn load(array: &[f32; 64], base: usize) -> f32x4 {
    f32x4(array[base], array[base + 1], array[base + 2], array[base + 3])
}

#[inline(always)]
fn store(array: &mut [f32; 64], base: usize, value: f32x4) {
    array[base] = value.0;
    array[base + 1] = value.1;
    array[base + 2] = value.2;
    array[base + 3] = value.3;
}

#[cfg(any(feature = "sse2-tests", feature = "simd-benches"))]
pub fn csc709_forward_8x8(v1: V1, block: &mut [[f32; 64]; 3]) {
    csc709_forward_8x8_batch(v1, std::iter::once(block));
}

pub fn csc709_forward_8x8_batch<'a>(v1: V1, blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
    let c_r = v1.splat_f32x4(0.2126);
    let c_g = v1.splat_f32x4(0.7152);
    let c_b = v1.splat_f32x4(0.0722);
    let inv_by = v1.splat_f32x4(1.0 / 1.8556);
    let inv_ry = v1.splat_f32x4(1.0 / 1.5747);

    let mul = |a, b| v1.mul_f32x4(a, b);
    let add = |a, b| v1.add_f32x4(a, b);
    let sub = |a, b| v1.sub_f32x4(a, b);

    for block in blocks {
        let [r, g, b] = block;
        for chunk in 0..16 {
            let base = chunk * 4;
            let rv = load(r, base);
            let gv = load(g, base);
            let bv = load(b, base);

            let y = add(add(mul(rv, c_r), mul(gv, c_g)), mul(bv, c_b));
            let by = mul(sub(bv, y), inv_by);
            let ry = mul(sub(rv, y), inv_ry);

            store(r, base, y);
            store(g, base, by);
            store(b, base, ry);
        }
    }
}

#[cfg(any(feature = "sse2-tests", feature = "simd-benches"))]
pub fn csc709_inverse_8x8(v1: V1, block: &mut [[f32; 64]; 3]) {
    csc709_inverse_8x8_batch(v1, std::iter::once(block));
}

pub fn csc709_inverse_8x8_batch<'a>(v1: V1, blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
    let c_ry = v1.splat_f32x4(1.5747);
    let c_by_g = v1.splat_f32x4(0.1873);
    let c_ry_g = v1.splat_f32x4(0.4682);
    let c_by = v1.splat_f32x4(1.8556);

    let mul = |a, b| v1.mul_f32x4(a, b);
    let add = |a, b| v1.add_f32x4(a, b);
    let sub = |a, b| v1.sub_f32x4(a, b);

    for block in blocks {
        let [comp0, comp1, comp2] = block;
        for chunk in 0..16 {
            let base = chunk * 4;
            let y = load(comp0, base);
            let by = load(comp1, base);
            let ry = load(comp2, base);

            let r = add(y, mul(ry, c_ry));
            let g = sub(sub(y, mul(by, c_by_g)), mul(ry, c_ry_g));
            let b = add(y, mul(by, c_by));

            store(comp0, base, r);
            store(comp1, base, g);
            store(comp2, base, b);
        }
    }
}
