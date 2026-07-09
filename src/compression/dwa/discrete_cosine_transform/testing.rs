// Test/bench support for the DWA DCT kernels.
//
// public only for benchmarking: benches/dct.rs is a separate crate and reaches
// this through the public API, so the module stays `pub` and
// `simd-benches`-gated; the in-crate tier tests pick it up via `test`.

use super::{dct_forward_8x8_batch, dct_inverse_8x8_batch};

// Only the in-crate tier tests use this, so it need not be `pub`.
#[allow(unused)]
pub(super) fn assert_blocks_match(
    label: &str,
    autovectorized: fn(&mut [f32; 64]),
    kernel: impl Fn(&mut [f32; 64]),
) {
    for mut expected in pseudo_random_blocks(64) {
        let mut actual = expected;
        autovectorized(&mut expected);
        kernel(&mut actual);

        for (e, a) in expected.iter().zip(actual.iter()) {
            let tolerance = 1e-2 * e.abs().max(1.0);
            assert!(
                (e - a).abs() <= tolerance,
                "{label}: expected {e}, got {a} (diff {})",
                (e - a).abs()
            );
        }
    }
}

// Deterministic blocks in the ballpark of half-precision DCT coefficients
// (xorshift64, no `rand` dependency. Shared by the correctness tests below
// and by the forced-tier benchmark in benches/dct.rs.
// Stays `pub` (unlike `assert_blocks_match`) only because the benchmark needs
// it.
#[allow(unused)]
pub fn pseudo_random_blocks(count: usize) -> Vec<[f32; 64]> {
    let mut state: u64 = 0x9e3779b97f4a7c15;

    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (((state >> 40) as i32 as f32) / (i32::MAX as f32)) * 1024.0
    };

    (0..count).map(|_| std::array::from_fn(|_| next())).collect()
}

#[allow(unused)]
fn dct_forward_8x8(data: &mut [f32; 64]) {
    dct_forward_8x8_batch(std::iter::once(data));
}

#[allow(unused)]
fn dct_inverse_8x8(data: &mut [f32; 64]) {
    dct_inverse_8x8_batch(std::iter::once(data));
}
