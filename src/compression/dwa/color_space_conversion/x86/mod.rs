// Runtime x86 SIMD dispatch for the DWA CSC transform: `try_csc709_*_8x8_batch`
// select the AVX2 tier when available and fall back to the SSE2 tier,
// otherwise let the caller use the scalar autovectorized path.

use pulp::x86::{V1, V3};

// public only for benchmarking
#[doc(hidden)]
pub mod avx2;

// public only for benchmarking
#[doc(hidden)]
pub mod sse2;

pub(super) fn try_csc709_forward_8x8_batch<'a, I>(blocks: &mut I) -> bool
where
    I: Iterator<Item = &'a mut [[f32; 64]; 3]>,
{
    if let Some(v3) = V3::try_new() {
        avx2::csc709_forward_8x8_batch(v3, blocks);
        return true;
    }
    if let Some(v1) = V1::try_new() {
        sse2::csc709_forward_8x8_batch(v1, blocks);
        return true;
    }
    false
}

pub(super) fn try_csc709_inverse_8x8_batch<'a, I>(blocks: &mut I) -> bool
where
    I: Iterator<Item = &'a mut [[f32; 64]; 3]>,
{
    if let Some(v3) = V3::try_new() {
        avx2::csc709_inverse_8x8_batch(v3, blocks);
        return true;
    }
    if let Some(v1) = V1::try_new() {
        sse2::csc709_inverse_8x8_batch(v1, blocks);
        return true;
    }
    false
}
