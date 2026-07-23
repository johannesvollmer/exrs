// Y'CbCr <-> R'G'B' color-space conversion for DWA lossy channel groups,
// using the modified 709 coefficients OpenEXR's DWA codec uses (zero-centered
// chroma instead of the usual 0.5 offset).
//
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[doc(hidden)]
pub mod x86;

// public only for benchmarking (benches reach `test::pseudo_random_triplets`)
#[cfg(any(test, feature = "simd-benches"))]
#[doc(hidden)]
pub mod test;

/// R'G'B' -> Y'CbCr forward conversion for one pixel. The component order
/// matches OpenEXR's channel-group storage: Y, BY, RY.
#[inline]
fn csc709_forward(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let by = (b - y) / 1.8556;
    let ry = (r - y) / 1.5747;
    (y, by, ry)
}

/// Y'CbCr -> R'G'B' inverse conversion for one pixel. Input comp0/1/2 are
/// Y, BY, RY; output is R, G, B.
#[inline]
fn csc709_inverse(comp0: f32, comp1: f32, comp2: f32) -> (f32, f32, f32) {
    let r = comp0 + 1.5747 * comp2;
    let g = comp0 - 0.1873 * comp1 - 0.4682 * comp2;
    let b = comp0 + 1.8556 * comp1;
    (r, g, b)
}

/// Autovectorized forward CSC over a whole 8x8 block triplet: writes Y, BY,
/// RY back into the same three arrays (`block[0]`, `block[1]`, `block[2]`).
// public only for benchmarking (the in-crate dispatch and tests reach it directly)
#[doc(hidden)]
pub fn csc709_forward_8x8_autovectorized(block: &mut [[f32; 64]; 3]) {
    let [r, g, b] = block;
    for i in 0..64 {
        let (y, by, ry) = csc709_forward(r[i], g[i], b[i]);
        r[i] = y;
        g[i] = by;
        b[i] = ry;
    }
}

/// Autovectorized inverse CSC over a whole 8x8 block triplet: writes R, G, B
/// back into the same three arrays (`block[0]`, `block[1]`, `block[2]`).
// public only for benchmarking (the in-crate dispatch and tests reach it directly)
#[doc(hidden)]
pub fn csc709_inverse_8x8_autovectorized(block: &mut [[f32; 64]; 3]) {
    let [comp0, comp1, comp2] = block;
    for i in 0..64 {
        let (r, g, b) = csc709_inverse(comp0[i], comp1[i], comp2[i]);
        comp0[i] = r;
        comp1[i] = g;
        comp2[i] = b;
    }
}

/// Forward CSC on many 8x8 block triplets, dispatched once for the whole
/// batch rather than once per block. Prefer this over looping calls to
/// `csc709_forward_8x8_autovectorized`.
pub(crate) fn csc709_forward_8x8_batch<'a>(mut blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if x86::try_csc709_forward_8x8_batch(&mut blocks) {
        return;
    }

    for block in blocks {
        csc709_forward_8x8_autovectorized(block);
    }
}

/// Inverse CSC on many 8x8 block triplets, dispatched once for the whole
/// batch rather than once per block. Prefer this over looping calls to
/// `csc709_inverse_8x8_autovectorized`.
pub(crate) fn csc709_inverse_8x8_batch<'a>(mut blocks: impl Iterator<Item = &'a mut [[f32; 64]; 3]>) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if x86::try_csc709_inverse_8x8_batch(&mut blocks) {
        return;
    }

    for block in blocks {
        csc709_inverse_8x8_autovectorized(block);
    }
}

#[cfg(test)]
mod scalar_test {
    use rand::{Rng, SeedableRng};

    use super::{csc709_forward, csc709_inverse};
    use crate::image::validate_results::ValidateResult;

    const SEED: [u8; 32] = [
        66, 100, 19, 240, 8, 91, 3, 128, 9, 44, 201, 17, 88, 6, 255, 61, 30, 11, 2, 121, 99, 1, 250,
        77, 33, 7, 42, 13, 200, 176, 22, 5,
    ];

    /// The R'G'B' <-> Y'CbCr conversion pair must round-trip: converting to
    /// Y'CbCr and back must recover the original RGB triple (approximately,
    /// since the matrix coefficients are not exactly invertible in f32). The
    /// forward output tuple `(y, by, ry)` feeds the inverse positionally.
    fn assert_csc_roundtrips(r: f32, g: f32, b: f32) {
        let (y, by, ry) = csc709_forward(r, g, b);
        let (r2, g2, b2) = csc709_inverse(y, by, ry);
        vec![r, g, b].assert_approx_equals_result(&vec![r2, g2, b2]);
    }

    #[test]
    fn csc_roundtrip_hardcoded() {
        assert_csc_roundtrips(0.0, 0.0, 0.0);
        assert_csc_roundtrips(1.0, 1.0, 1.0);
        assert_csc_roundtrips(1.0, 0.0, 0.0);
        assert_csc_roundtrips(0.0, 1.0, 0.0);
        assert_csc_roundtrips(0.0, 0.0, 1.0);
        assert_csc_roundtrips(0.25, 0.5, 0.75);
    }

    #[test]
    fn csc_roundtrip_seeded() {
        let mut random = rand::rngs::StdRng::from_seed(SEED);
        for _ in 0..256 {
            let mut channel = || random.gen_range(-4.0f32..4.0);
            assert_csc_roundtrips(channel(), channel(), channel());
        }
    }
}
