//! Runtime x86 SIMD dispatch for DWA DCT: Avx, else Sse, else scalar.
//! Both tiers share the lazily-built `forward_basis` cosine table.
//!
//! Kernel modules are `#[doc(hidden)]` for benches. Names match miraculix
//! tokens (`avx` / `sse`): f32 arithmetic only, no AVX2 int ops. `avx512dq`
//! is an inverse-only prototype (needs `Avx512f` + `Avx` + `Avx512Dq`); not
//! selected by the batch `try_*` below.

use crate::compression::simd_detect::x86::miraculix_x86::{avx, sse};

/// Bench/test entry; `Avx` token (f32 only).
#[doc(hidden)]
pub mod avx;

/// Inverse pair/quad prototype; not selected by `try_*` below.
/// Named for `Avx512Dq`, the tightest of the three tokens it needs.
#[doc(hidden)]
pub mod avx512dq;

/// Bench/test entry; `Sse` token (f32 only).
#[doc(hidden)]
pub mod sse;

pub(super) fn try_dct_forward_8x8_batch<'a, I>(blocks: &mut I) -> bool
where
    I: Iterator<Item = &'a mut [f32; 64]>,
{
    if let Some(avx_token) = avx() {
        self::avx::dct_forward_8x8_batch(avx_token, blocks);
        return true;
    }
    if let Some(sse_token) = sse() {
        for data in blocks {
            self::sse::dct_forward_8x8(sse_token, data);
        }
        return true;
    }
    false
}

pub(super) fn try_dct_inverse_8x8_batch<'a, I>(blocks: &mut I) -> bool
where
    I: Iterator<Item = &'a mut [f32; 64]>,
{
    if let Some(avx_token) = avx() {
        self::avx::dct_inverse_8x8_batch(avx_token, blocks);
        return true;
    }
    if let Some(sse_token) = sse() {
        for data in blocks {
            self::sse::dct_inverse_8x8(sse_token, data);
        }
        return true;
    }
    false
}
