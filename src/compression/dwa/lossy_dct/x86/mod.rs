//! Runtime x86 SIMD dispatch for DWA fused lossy-DCT decode.
//!
//! Tier order: AVX-512 fused (2 blocks/reg, 16-lane pair write) ->
//! AVX+F16C fused (file `avx2`) -> strip-tile with F16C then SSE2 write-row.
//!
//! File `avx2` names the host tier; it does not use an `Avx2` token.
//! Zigzag is SSE2/SSSE3/SSE4.1+F16C; DCT/CSC use `Avx` / `Avx512f`+`Dq`.
//! This module picks the tier and owns the shared write-row rounding constant.

mod avx2;
mod avx512;
mod sse2;

use super::{PackedStream, ScanlineTarget};
use crate::{compression::simd_detect::x86::miraculix_x86, error::Result as ExrResult};

/// Rounding immediate for `vcvtps2ph`: `_MM_FROUND_TO_NEAREST_INT` (0).
/// Matches `half::f16::from_f32`'s F16C path so both converts are
/// bit-identical.
pub(super) const ROUND_TO_NEAREST: i32 = 0;

pub(super) fn try_from_half_zigzag(zig_zag: &[u16; 64], dst: &mut [f32; 64]) -> bool {
    let (Some(sse2), Some(ssse3), Some(sse41), Some(f16c)) = (
        miraculix_x86::sse2(),
        miraculix_x86::ssse3(),
        miraculix_x86::sse41(),
        miraculix_x86::f16c(),
    ) else {
        return false;
    };
    avx2::from_half_zigzag(sse2, ssse3, sse41, f16c, zig_zag, dst);
    true
}

/// F16C 8-lane write-row for a full-width F16 output row.
pub(super) fn try_write_row_f16(
    row: &[f32],
    to_linear: Option<&[u16; 65536]>,
    out_row: &mut [u8],
) -> bool {
    let Some(f16c) = miraculix_x86::f16c() else {
        return false;
    };
    avx2::write_row_f16(f16c, row, to_linear, out_row)
}

/// Like `try_write_row_f16`, then widen linearized halves back to f32 for F32
/// channels.
pub(super) fn try_write_row_f32(
    row: &[f32],
    to_linear: Option<&[u16; 65536]>,
    out_row: &mut [u8],
) -> bool {
    let Some(f16c) = miraculix_x86::f16c() else {
        return false;
    };
    avx2::write_row_f32(f16c, row, to_linear, out_row)
}

/// Soft f32<->f16 write-row when F16C is unavailable.
pub(super) fn try_write_row_f16_sse2(
    row: &[f32],
    to_linear: Option<&[u16; 65536]>,
    out_row: &mut [u8],
) -> bool {
    let (Some(sse), Some(sse2)) = (miraculix_x86::sse(), miraculix_x86::sse2()) else {
        return false;
    };
    sse2::write_row_f16(sse, sse2, row, to_linear, out_row)
}

/// Soft write-row for F32 channels; see `try_write_row_f16_sse2`.
pub(super) fn try_write_row_f32_sse2(
    row: &[f32],
    to_linear: Option<&[u16; 65536]>,
    out_row: &mut [u8],
) -> bool {
    let (Some(sse), Some(sse2)) = (miraculix_x86::sse(), miraculix_x86::sse2()) else {
        return false;
    };
    sse2::write_row_f32(sse, sse2, row, to_linear, out_row)
}

/// Per spatial 8x8: unRLE -> zigzag -> iDCT -> CSC -> write (AVX+F16C fused).
/// `None` without F16C (+ SSE4.1 family); caller falls back to strip-tile.
pub(super) fn try_decode_group_fused(
    ac: &mut PackedStream<'_>,
    dc: &mut PackedStream<'_>,
    width: usize,
    height: usize,
    to_linear: Option<&[u16; 65536]>,
    targets: &mut [ScanlineTarget<'_>],
    out: &mut [u8],
) -> Option<ExrResult<()>> {
    let (Some(sse2), Some(ssse3), Some(sse41), Some(f16c)) = (
        miraculix_x86::sse2(),
        miraculix_x86::ssse3(),
        miraculix_x86::sse41(),
        miraculix_x86::f16c(),
    ) else {
        return None;
    };
    // F16C gates this path (implies AVX on shipping CPUs); DCT/CSC need Avx.
    let avx = miraculix_x86::avx().expect("AVX confirmed available by the F16C tier gate above");
    Some(avx2::decode_group_fused(
        sse2, ssse3, sse41, f16c, avx, ac, dc, width, height, to_linear, targets, out,
    ))
}

/// AVX-512 fused analog: DCT/CSC on 2 blocks per register, 16-lane pair write.
/// Zigzag stays per-block on the F16C helpers. `None` without AVX-512F+F16C;
/// caller then tries `try_decode_group_fused`, then strip-tile.
pub(super) fn try_decode_group_fused_avx512(
    ac: &mut PackedStream<'_>,
    dc: &mut PackedStream<'_>,
    width: usize,
    height: usize,
    to_linear: Option<&[u16; 65536]>,
    targets: &mut [ScanlineTarget<'_>],
    out: &mut [u8],
) -> Option<ExrResult<()>> {
    let Some(avx512f) = miraculix_x86::avx512f() else {
        return None;
    };
    let (Some(sse2), Some(ssse3), Some(sse41), Some(f16c)) = (
        miraculix_x86::sse2(),
        miraculix_x86::ssse3(),
        miraculix_x86::sse41(),
        miraculix_x86::f16c(),
    ) else {
        return None;
    };
    // AVX-512F implies base AVX and AVX-512DQ here; DCT/CSC need all three.
    let avx = miraculix_x86::avx().expect("AVX confirmed available by the AVX-512 tier gate above");
    let avx512dq = miraculix_x86::avx512dq()
        .expect("AVX-512DQ confirmed available by the AVX-512 tier gate above");
    Some(avx512::decode_group_fused(
        sse2, ssse3, sse41, f16c, avx, avx512f, avx512dq, ac, dc, width, height, to_linear,
        targets, out,
    ))
}
