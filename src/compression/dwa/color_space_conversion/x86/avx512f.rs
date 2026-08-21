// AVX-512F CSC: pure elementwise widen to 2 blocks/register (A lanes 0-7, B
// 8-15).
use miraculix::x86::ops::avx512::avx512f::Avx512f;

#[inline(always)]
fn load_pair(a: &[f32; 64], b: &[f32; 64], base: usize) -> [f32; 16] {
    [
        a[base],
        a[base + 1],
        a[base + 2],
        a[base + 3],
        a[base + 4],
        a[base + 5],
        a[base + 6],
        a[base + 7],
        b[base],
        b[base + 1],
        b[base + 2],
        b[base + 3],
        b[base + 4],
        b[base + 5],
        b[base + 6],
        b[base + 7],
    ]
}

#[inline(always)]
fn store_pair(a: &mut [f32; 64], b: &mut [f32; 64], base: usize, value: [f32; 16]) {
    a[base..base + 8].copy_from_slice(&value[0..8]);
    b[base..base + 8].copy_from_slice(&value[8..16]);
}

// Wrapped in `miraculix::avx512f_fn!`: the composed `add_f32x16`/
// `mul_f32x16`/`sub_f32x16` chain per chunk needs a shared
// `#[target_feature]` context to inline into real `zmm` code instead of a
// `callq` chain
miraculix::avx512f_fn! {
    #[cfg(any(feature = "avx512-tests", feature = "simd-benches"))]
    pub fn csc709_forward_8x8_pair(avx512f: Avx512f, a: &mut [[f32; 64]; 3], b: &mut [[f32; 64]; 3]) {
        forward_pair(avx512f, a, b);
    }
}

#[inline(always)]
pub(crate) fn forward_pair(avx512f: Avx512f, a: &mut [[f32; 64]; 3], b: &mut [[f32; 64]; 3]) {
    let c_r = [0.2126f32; 16];
    let c_g = [0.7152f32; 16];
    let c_b = [0.0722f32; 16];
    let inv_by = [1.0f32 / 1.8556; 16];
    let inv_ry = [1.0f32 / 1.5747; 16];

    let [ar, ag, ab] = a;
    let [br, bg, bb] = b;
    for chunk in 0..8 {
        let base = chunk * 8;
        let rv = load_pair(ar, br, base);
        let gv = load_pair(ag, bg, base);
        let bv = load_pair(ab, bb, base);

        let y = avx512f.add_f32x16(
            avx512f.add_f32x16(avx512f.mul_f32x16(rv, c_r), avx512f.mul_f32x16(gv, c_g)),
            avx512f.mul_f32x16(bv, c_b),
        );
        let by = avx512f.mul_f32x16(avx512f.sub_f32x16(bv, y), inv_by);
        let ry = avx512f.mul_f32x16(avx512f.sub_f32x16(rv, y), inv_ry);

        store_pair(ar, br, base, y);
        store_pair(ag, bg, base, by);
        store_pair(ab, bb, base, ry);
    }
}

miraculix::avx512f_fn! {
    #[cfg(any(feature = "avx512-tests", feature = "simd-benches"))]
    pub fn csc709_inverse_8x8_pair(avx512f: Avx512f, a: &mut [[f32; 64]; 3], b: &mut [[f32; 64]; 3]) {
        inverse_pair(avx512f, a, b);
    }
}

/// One 8x8 inverse CSC for each of two blocks at once.
#[inline(always)]
pub(crate) fn inverse_pair(avx512f: Avx512f, a: &mut [[f32; 64]; 3], b: &mut [[f32; 64]; 3]) {
    let c_ry = [1.5747f32; 16];
    let c_by_g = [0.1873f32; 16];
    let c_ry_g = [0.4682f32; 16];
    let c_by = [1.8556f32; 16];

    let [a0, a1, a2] = a;
    let [b0, b1, b2] = b;
    for chunk in 0..8 {
        let base = chunk * 8;
        let y = load_pair(a0, b0, base);
        let by = load_pair(a1, b1, base);
        let ry = load_pair(a2, b2, base);

        let r = avx512f.add_f32x16(y, avx512f.mul_f32x16(ry, c_ry));
        let g = avx512f.sub_f32x16(
            avx512f.sub_f32x16(y, avx512f.mul_f32x16(by, c_by_g)),
            avx512f.mul_f32x16(ry, c_ry_g),
        );
        let b_out = avx512f.add_f32x16(y, avx512f.mul_f32x16(by, c_by));

        store_pair(a0, b0, base, r);
        store_pair(a1, b1, base, g);
        store_pair(a2, b2, base, b_out);
    }
}

// Wrapped in `miraculix::avx512f_fn!`
miraculix::avx512f_fn! {
    pub fn csc709_inverse_8x8_batch<'a>(
        avx512f: Avx512f,
        blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>,
    ) {
        let mut iter = blocks;
        loop {
            let Some(first) = iter.next() else { break };
            match iter.next() {
                Some(second) => inverse_pair(avx512f, first, second),
                None => {
                    super::super::csc709_inverse_8x8_autovectorized(first);
                    break;
                }
            }
        }
    }
}

miraculix::avx512f_fn! {
    pub fn csc709_forward_8x8_batch<'a>(
        avx512f: Avx512f,
        blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>,
    ) {
        let mut iter = blocks;
        loop {
            let Some(first) = iter.next() else { break };
            match iter.next() {
                Some(second) => forward_pair(avx512f, first, second),
                None => {
                    super::super::csc709_forward_8x8_autovectorized(first);
                    break;
                }
            }
        }
    }
}
