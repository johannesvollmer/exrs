//! Runtime x86 SIMD dispatch for DWA CSC: Avx, else Sse, else scalar.
//!
//! Kernel modules are `#[doc(hidden)]` for benches. Names match miraculix
//! tokens (`avx` / `sse`, not avx2 / sse2): only f32 arithmetic. `avx512f`
//! is a pair-of-blocks prototype used by fused decode and benches, not by
//! the batch `try_*` below.

use crate::compression::simd_detect::x86::miraculix_x86::{avx, sse};

/// Bench/test entry; `Avx` token (f32 only).
#[doc(hidden)]
pub mod avx;

/// Pair-of-blocks prototype (`Avx512f`); not selected by `try_*` below.
#[doc(hidden)]
pub mod avx512f;

/// Bench/test entry; `Sse` token (f32 only).
#[doc(hidden)]
pub mod sse;

pub(super) fn try_csc709_forward_8x8_batch<'a, I>(blocks: &mut I) -> bool
where
    I: Iterator<Item = &'a mut [[f32; 64]; 3]>,
{
    if let Some(avx_token) = avx() {
        self::avx::csc709_forward_8x8_batch(avx_token, blocks);
        return true;
    }
    if let Some(sse_token) = sse() {
        self::sse::csc709_forward_8x8_batch(sse_token, blocks);
        return true;
    }
    false
}

pub(super) fn try_csc709_inverse_8x8_batch<'a, I>(blocks: &mut I) -> bool
where
    I: Iterator<Item = &'a mut [[f32; 64]; 3]>,
{
    if let Some(avx_token) = avx() {
        self::avx::csc709_inverse_8x8_batch(avx_token, blocks);
        return true;
    }
    if let Some(sse_token) = sse() {
        self::sse::csc709_inverse_8x8_batch(sse_token, blocks);
        return true;
    }
    false
}
