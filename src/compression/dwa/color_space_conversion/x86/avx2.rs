// AVX2 V3 tier: the CSC transform is a fixed per-element linear combination
// of three same-length arrays (no cross-lane shuffles needed), so each 8-wide
// chunk of the 64-element block is loaded, combined, and stored independently.
use pulp::{f32x8, x86::V3};

#[inline(always)]
fn load(v3: V3, array: &[f32; 64], base: usize) -> f32x8 {
    let _ = v3;
    f32x8(
        array[base],
        array[base + 1],
        array[base + 2],
        array[base + 3],
        array[base + 4],
        array[base + 5],
        array[base + 6],
        array[base + 7],
    )
}

#[inline(always)]
fn store(array: &mut [f32; 64], base: usize, value: f32x8) {
    array[base] = value.0;
    array[base + 1] = value.1;
    array[base + 2] = value.2;
    array[base + 3] = value.3;
    array[base + 4] = value.4;
    array[base + 5] = value.5;
    array[base + 6] = value.6;
    array[base + 7] = value.7;
}

#[cfg(any(feature = "avx2-tests", feature = "simd-benches"))]
pub fn csc709_forward_8x8(v3: V3, block: &mut [[f32; 64]; 3]) {
    csc709_forward_8x8_batch(v3, std::iter::once(block));
}

pub fn csc709_forward_8x8_batch<'a>(v3: V3, blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
    v3.vectorize(move || {
        // OpenEXR's modified 709 coefficients (zero-centered chroma).
        let c_r = v3.splat_f32x8(0.2126);
        let c_g = v3.splat_f32x8(0.7152);
        let c_b = v3.splat_f32x8(0.0722);
        let inv_by = v3.splat_f32x8(1.0 / 1.8556);
        let inv_ry = v3.splat_f32x8(1.0 / 1.5747);

        let mul = |a, b| v3.mul_f32x8(a, b);
        let add = |a, b| v3.add_f32x8(a, b);
        let sub = |a, b| v3.sub_f32x8(a, b);

        for block in blocks {
            let [r, g, b] = block;
            for chunk in 0..8 {
                let base = chunk * 8;
                let rv = load(v3, r, base);
                let gv = load(v3, g, base);
                let bv = load(v3, b, base);

                let y = add(add(mul(rv, c_r), mul(gv, c_g)), mul(bv, c_b));
                let by = mul(sub(bv, y), inv_by);
                let ry = mul(sub(rv, y), inv_ry);

                store(r, base, y);
                store(g, base, by);
                store(b, base, ry);
            }
        }
    });
}

#[cfg(any(feature = "avx2-tests", feature = "simd-benches"))]
pub fn csc709_inverse_8x8(v3: V3, block: &mut [[f32; 64]; 3]) {
    csc709_inverse_8x8_batch(v3, std::iter::once(block));
}

pub fn csc709_inverse_8x8_batch<'a>(v3: V3, blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
    v3.vectorize(move || {
        let c_ry = v3.splat_f32x8(1.5747);
        let c_by_g = v3.splat_f32x8(0.1873);
        let c_ry_g = v3.splat_f32x8(0.4682);
        let c_by = v3.splat_f32x8(1.8556);

        let mul = |a, b| v3.mul_f32x8(a, b);
        let add = |a, b| v3.add_f32x8(a, b);
        let sub = |a, b| v3.sub_f32x8(a, b);

        for block in blocks {
            let [comp0, comp1, comp2] = block;
            for chunk in 0..8 {
                let base = chunk * 8;
                let y = load(v3, comp0, base);
                let by = load(v3, comp1, base);
                let ry = load(v3, comp2, base);

                let r = add(y, mul(ry, c_ry));
                let g = sub(sub(y, mul(by, c_by_g)), mul(ry, c_ry_g));
                let b = add(y, mul(by, c_by));

                store(comp0, base, r);
                store(comp1, base, g);
                store(comp2, base, b);
            }
        }
    });
}
