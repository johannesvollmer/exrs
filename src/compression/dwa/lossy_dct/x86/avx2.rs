// AVX2+F16C: one 8x8/step. Zigzag = OpenEXR `fromHalfZigZag_f16c` shuffle +
// F16C widen. `decode_group_fused` keeps unRLE->write L1-hot (~1 KiB/block).
// `zigzag_block`/`write_block` also serve avx512 (no AVX-512 zigzag;
// DC-only/odd tail fallback). SSE2 fusion was a ~1% regression vs strip-tile:
// unshipped.

use std::convert::TryInto;

use half::f16;
use miraculix::x86::ops::{
    avx::{avx::Avx, f16c::F16c},
    sse::{sse2::Sse2, sse41::Sse41, ssse3::Ssse3},
};

use super::{
    super::{PackedStream, ScanlineTarget, ac_rle::un_rle_ac},
    ROUND_TO_NEAREST,
};
use crate::{
    compression::dwa::{
        color_space_conversion,
        discrete_cosine_transform::{self, x86::avx as dct_avx2},
    },
    error::{Error, Result as ExrResult},
    meta::attribute::SampleType,
};

/// Un-RLE + un-zigzag one spatial block into `dct_blocks`. `needs_inverse`
/// tracks which components still need a real iDCT vs. already being filled
/// in by the DC-only fast path. Shared by the AVX2 and AVX-512 fused paths.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(super) fn zigzag_block(
    sse2: Sse2,
    ssse3: Ssse3,
    sse41: Sse41,
    f16c: F16c,
    ac: &mut PackedStream<'_>,
    dc: &mut PackedStream<'_>,
    components: usize,
    block_count: usize,
    block_index: usize,
    dct_blocks: &mut [[f32; 64]; 3],
    needs_inverse: &mut [bool; 3],
) -> ExrResult<()> {
    for component in 0..components {
        let mut zig_block = [0u16; 64];
        zig_block[0] = match dc.peek_at(component * block_count + block_index) {
            Some(v) => v,
            None => return Err(Error::invalid("truncated DWA DC data")),
        };

        let last_non_zero = un_rle_ac(ac, &mut zig_block)?;

        let dct_block = &mut dct_blocks[component];
        if last_non_zero == 0 {
            dct_block[0] = f16::from_bits(zig_block[0]).to_f32();
            discrete_cosine_transform::dct_inverse_8x8_dc_only(dct_block);
            needs_inverse[component] = false;
        } else {
            // Tokens already probed above; call the kernel directly.
            from_half_zigzag(sse2, ssse3, sse41, f16c, &zig_block, dct_block);
            needs_inverse[component] = true;
        }
    }
    Ok(())
}

/// Write one already-inverted (and, if 3 components, already CSC'd) spatial
/// block to its scanline target(s). Shared the same way as `zigzag_block`.
#[inline(always)]
pub(super) fn write_block(
    f16c: F16c,
    block_x: usize,
    block_y: usize,
    x_count: usize,
    y_count: usize,
    to_linear: Option<&[u16; 65536]>,
    dct_blocks: &[[f32; 64]; 3],
    targets: &mut [ScanlineTarget<'_>],
    out: &mut [u8],
) -> Option<Error> {
    for (component, target) in targets.iter_mut().enumerate() {
        let block = &dct_blocks[component];
        let bytes_per_sample = target.sample_type.bytes_per_sample();
        for dy in 0..y_count {
            let y = block_y * 8 + dy;
            let row = &block[dy * 8..dy * 8 + x_count];
            let offset = target.row_offsets[y] + block_x * 8 * bytes_per_sample;
            let out_row = &mut out[offset..][..x_count * bytes_per_sample];

            let handled = match target.sample_type {
                SampleType::F16 => write_row_f16(f16c, row, to_linear, out_row),
                SampleType::F32 => write_row_f32(f16c, row, to_linear, out_row),
                SampleType::U32 => false,
            };
            if handled {
                continue;
            }

            // Edge blocks (x_count < 8) or U32 -> scalar fallback.
            match target.sample_type {
                SampleType::F16 => {
                    for (chunk, &value) in out_row.chunks_exact_mut(2).zip(row) {
                        let linear = linearize_scalar(value, to_linear);
                        chunk.copy_from_slice(&linear.to_bits().to_le_bytes());
                    }
                }
                SampleType::F32 => {
                    for (chunk, &value) in out_row.chunks_exact_mut(4).zip(row) {
                        let linear = linearize_scalar(value, to_linear);
                        chunk.copy_from_slice(&linear.to_f32().to_le_bytes());
                    }
                }
                SampleType::U32 => {
                    return Some(Error::unsupported("DWA lossy DCT compression of u32 channels"));
                }
            }
        }
    }
    None
}

/// For each spatial 8x8, finish unRLE -> zigzag -> iDCT -> CSC -> scanline
/// write before touching the next block. Caller (`x86::mod`) has already
/// confirmed the tokens below are available.
#[allow(clippy::too_many_arguments)]
pub(super) fn decode_group_fused(
    sse2: Sse2,
    ssse3: Ssse3,
    sse41: Sse41,
    f16c: F16c,
    avx: Avx,
    ac: &mut PackedStream<'_>,
    dc: &mut PackedStream<'_>,
    width: usize,
    height: usize,
    to_linear: Option<&[u16; 65536]>,
    targets: &mut [ScanlineTarget<'_>],
    out: &mut [u8],
) -> ExrResult<()> {
    let components = targets.len();
    if components != 1 && components != 3 {
        return Err(Error::invalid("invalid DWA lossy component count"));
    }

    let blocks_x = (width + 7) / 8;
    let blocks_y = (height + 7) / 8;
    let block_count = blocks_x * blocks_y;

    // One spatial block's components. 3 x 256 B = 768 B -> stays in L1 for the
    // whole unRLE -> write pipeline of that block (OpenEXR's shape).
    let mut dct_blocks = [[0.0f32; 64]; 3];
    let mut needs_inverse = [false; 3];

    for block_y in 0..blocks_y {
        let y_count = 8.min(height - block_y * 8);
        for block_x in 0..blocks_x {
            let block_index = block_y * blocks_x + block_x;
            let x_count = 8.min(width - block_x * 8);

            zigzag_block(
                sse2,
                ssse3,
                sse41,
                f16c,
                ac,
                dc,
                components,
                block_count,
                block_index,
                &mut dct_blocks,
                &mut needs_inverse,
            )?;

            // iDCT every component that needs it, optional CSC, then write.
            // Keeps the 768 B block set hot end-to-end instead of reloading a
            // multi-block strip four times. Extracted into
            // `decode_one_block_dct_csc_write` so `miraculix::avx_fn!` can
            // wrap the whole thing: see that function's doc.
            if let Some(err) = decode_one_block_dct_csc_write(
                f16c,
                avx,
                components,
                &needs_inverse,
                &mut dct_blocks,
                block_x,
                block_y,
                x_count,
                y_count,
                to_linear,
                targets,
                out,
            ) {
                return Err(err);
            }
        }
    }

    dc.advance(components * block_count);
    Ok(())
}

// One spatial block's iDCT + optional CSC + write, extracted so
// `miraculix::avx_fn!` can wrap the whole thing: `inverse_one` alone
// composes 2 register transposes plus the row/column-pass butterfly
// (dozens of chained token-method calls) that need a shared
// `#[target_feature]` context to inline into real `ymm` code instead of a
// `callq` chain: see `discrete_cosine_transform::x86::avx::
// dct_inverse_8x8_batch`'s doc for the `llvm-objdump` finding that caught
// this.
miraculix::avx_fn! {
    #[allow(clippy::too_many_arguments)]
    fn decode_one_block_dct_csc_write(
        f16c: F16c,
        avx: Avx,
        components: usize,
        needs_inverse: &[bool; 3],
        dct_blocks: &mut [[f32; 64]; 3],
        block_x: usize,
        block_y: usize,
        x_count: usize,
        y_count: usize,
        to_linear: Option<&[u16; 65536]>,
        targets: &mut [ScanlineTarget<'_>],
        out: &mut [u8],
    ) -> Option<Error> {
        let coef = dct_avx2::Coefficients::new(avx);
        for component in 0..components {
            if needs_inverse[component] {
                dct_avx2::inverse_one(avx, &coef, &mut dct_blocks[component]);
            }
        }

        if components == 3 {
            color_space_conversion::x86::avx::inverse_one(avx, dct_blocks);
        }

        write_block(f16c, block_x, block_y, x_count, y_count, to_linear, dct_blocks, targets, out)
    }
}

#[inline(always)]
fn linearize_scalar(value: f32, to_linear: Option<&[u16; 65536]>) -> f16 {
    let nonlinear = f16::from_f32(value);
    match to_linear {
        Some(table) => f16::from_bits(table[nonlinear.to_bits() as usize]),
        None => nonlinear,
    }
}

/// Per-lane `to_linear` table gather. Unlike the pre-port pulp code (whose
/// `__m128i` register had no per-lane indexing and needed a GPR
/// extract/lookup/insert roundtrip), a miraculix register *is* a plain
/// `[u16; N]` array, so the gather is just array indexing: no SIMD op at
/// any width, shared unchanged by every write-row variant below (8- and
/// 16-lane alike).
#[inline(always)]
pub(super) fn linearize_lanes<const N: usize>(bits: [u16; N], table: &[u16; 65536]) -> [u16; N] {
    std::array::from_fn(|i| table[bits[i] as usize])
}

// Vectorized version of `decode_lossy_dct_group`'s F16-output write-row
// loop for a full 8-wide row.
miraculix::f16c_fn! {
    pub(super) fn write_row_f16(
        f16c: F16c,
        row: &[f32],
        to_linear: Option<&[u16; 65536]>,
        out_row: &mut [u8],
    ) -> bool {
        let Ok(&row) = TryInto::<&[f32; 8]>::try_into(row) else {
            return false;
        };
        if out_row.len() != 16 {
            return false;
        }
        let nonlinear = f16c.f32_to_f16x8::<ROUND_TO_NEAREST>(row);
        let linear = match to_linear {
            Some(table) => linearize_lanes(nonlinear, table),
            None => nonlinear,
        };
        for (chunk, &half) in out_row.chunks_exact_mut(2).zip(linear.iter()) {
            chunk.copy_from_slice(&half.to_le_bytes());
        }
        true
    }
}

// Same as `write_row_f16`, but widens the linearized halves back to f32
// (via a second `vcvtph2ps`) for F32-sample-type channels, matching the
// scalar path's `linear.to_f32()`.
miraculix::f16c_fn! {
    pub(super) fn write_row_f32(
        f16c: F16c,
        row: &[f32],
        to_linear: Option<&[u16; 65536]>,
        out_row: &mut [u8],
    ) -> bool {
        let Ok(&row) = TryInto::<&[f32; 8]>::try_into(row) else {
            return false;
        };
        if out_row.len() != 32 {
            return false;
        }
        let nonlinear = f16c.f32_to_f16x8::<ROUND_TO_NEAREST>(row);
        let linear = match to_linear {
            Some(table) => linearize_lanes(nonlinear, table),
            None => nonlinear,
        };
        let widened = f16c.f16_to_f32x8(linear);
        for (chunk, &value) in out_row.chunks_exact_mut(4).zip(widened.iter()) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        true
    }
}

/// Bit-reinterprets a 128-bit register's raw bytes at a different lane
/// width. Register-shuffle stages below move data between `i16x8`/`i32x4`/
/// `i64x2`/`u8x16` views of the *same* 16 bytes (x86 SIMD registers have no
/// fixed element type at the hardware level); these helpers make that
/// explicit and are the only non-`unsafe` way to express it without
/// violating this crate's `#![forbid(unsafe_code)]`.
#[inline(always)]
fn i16x8_from_u8x16(v: [u8; 16]) -> [i16; 8] {
    std::array::from_fn(|i| i16::from_le_bytes([v[i * 2], v[i * 2 + 1]]))
}

#[inline(always)]
fn u8x16_from_i16x8(v: [i16; 8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for i in 0..8 {
        out[i * 2..i * 2 + 2].copy_from_slice(&v[i].to_le_bytes());
    }
    out
}

#[inline(always)]
fn i32x4_from_i16x8(v: [i16; 8]) -> [i32; 4] {
    std::array::from_fn(|i| {
        let lo = v[i * 2] as u16 as u32;
        let hi = v[i * 2 + 1] as u16 as u32;
        (lo | (hi << 16)) as i32
    })
}

#[inline(always)]
fn i16x8_from_i32x4(v: [i32; 4]) -> [i16; 8] {
    std::array::from_fn(|i| {
        let word = v[i / 2] as u32;
        (if i % 2 == 0 {
            word
        } else {
            word >> 16
        }) as u16 as i16
    })
}

#[inline(always)]
fn i64x2_from_i32x4(v: [i32; 4]) -> [i64; 2] {
    std::array::from_fn(|i| {
        let lo = v[i * 2] as u32 as u64;
        let hi = v[i * 2 + 1] as u32 as u64;
        (lo | (hi << 32)) as i64
    })
}

#[inline(always)]
fn u8x16_from_i64x2(v: [i64; 2]) -> [u8; 16] {
    let mut out = [0u8; 16];
    for i in 0..2 {
        out[i * 8..i * 8 + 8].copy_from_slice(&v[i].to_le_bytes());
    }
    out
}

#[inline(always)]
fn u16x8_from_u8x16(v: [u8; 16]) -> [u16; 8] {
    std::array::from_fn(|i| u16::from_le_bytes([v[i * 2], v[i * 2 + 1]]))
}

// OpenEXR's `fromHalfZigZag_f16c`: unRLE has already produced 64 zigzag-order
// half-float bit patterns; this un-zigzags them into row-major order and
// widens to f32 in one pass, via a 3-stage (word/dword/qword) register
// transpose + a handful of `pshuflw`/`pshufhw`/`pshufd`/`palignr`/`pblendw`
// fixups, rather than 64 independent scalar loads. Ported 1:1
// intrinsic-for-intrinsic from the original SSE/SSSE3/SSE4.1/F16C sequence
// (see `test::zigzag_simd_matches_scalar` for the bit-exact oracle): not a
// place to redesign, the shuffle network's shape is exactly what OpenEXR
// measured to beat a plain gather.
miraculix::sse41_f16c_fn! {
    pub(super) fn from_half_zigzag(
        sse2: Sse2,
        ssse3: Ssse3,
        sse41: Sse41,
        f16c: F16c,
        src: &[u16; 64],
        dst: &mut [f32; 64],
    ) {
        let e = |i: usize| src[i] as i16;
        let setr = |a: usize, b: usize, c: usize, d: usize, f: usize, g: usize, h: usize, i: usize| -> [i16; 8] {
            [e(a), e(b), e(c), e(d), e(f), e(g), e(h), e(i)]
        };

        // x8 <- [0-7]; x6 <- [56-63]; x9 <- [21-28]; x7 <- [28-35]; x3 <- [6-9,54-57]
        let xmm8 = setr(0, 1, 2, 3, 4, 5, 6, 7);
        let xmm6_init = setr(56, 57, 58, 59, 60, 61, 62, 63);
        let xmm9 = setr(21, 22, 23, 24, 25, 26, 27, 28);
        let xmm7 = setr(28, 29, 30, 31, 32, 33, 34, 35);
        let xmm3 = setr(6, 7, 8, 9, 54, 55, 56, 57);

        let mem70 = setr(35, 36, 37, 38, 39, 40, 41, 42);
        let mem82 = setr(41, 42, 43, 44, 45, 46, 47, 48);
        let mem98 = setr(49, 50, 51, 52, 53, 54, 55, 56);
        let mem14 = setr(7, 8, 9, 10, 11, 12, 13, 14);
        let mem30 = setr(15, 16, 17, 18, 19, 20, 21, 22);

        // Setup rows 0-2 of A in xmm0-xmm2
        let xmm1 = i16x8_from_u8x16(sse2.srli_u8x16::<2>(u8x16_from_i16x8(xmm8)));
        let xmm2 = i16x8_from_u8x16(sse2.slli_u8x16::<4>(u8x16_from_i16x8(xmm8)));
        let xmm0 = i16x8_from_u8x16(ssse3.alignr_u8x16::<2>(u8x16_from_i16x8(xmm8), u8x16_from_i16x8(mem70)));
        let xmm1 = sse41.blend_i16x8::<0xfc>(xmm1, mem82);
        let xmm2 = sse41.blend_i16x8::<0x1f>(xmm2, mem98);

        // Setup rows 4-6 of A in xmm4-xmm6
        let xmm4 = i16x8_from_u8x16(sse2.srli_u8x16::<4>(u8x16_from_i16x8(xmm6_init)));
        let xmm5 = i16x8_from_u8x16(sse2.slli_u8x16::<2>(u8x16_from_i16x8(xmm6_init)));
        let xmm6 =
            i16x8_from_u8x16(ssse3.alignr_u8x16::<14>(u8x16_from_i16x8(xmm9), u8x16_from_i16x8(xmm6_init)));
        let xmm4 = sse41.blend_i16x8::<0xf8>(xmm4, mem14);
        let xmm5 = sse41.blend_i16x8::<0x3f>(xmm5, mem30);

        // Reverse the even rows (pshuflw+pshufhw+pshufd with 0x1b/0x1b/0x4e is a
        // full 8-lane reversal, confirmed by hand-tracing the immediate fields).
        let reverse = |v: [i16; 8]| -> [i16; 8] {
            let v = sse2.shufflelo_i16x8::<0x1b>(v);
            let v = sse2.shufflehi_i16x8::<0x1b>(v);
            i16x8_from_i32x4(sse2.shuffle_i32x4::<0x4e>(i32x4_from_i16x8(v)))
        };
        let xmm0 = reverse(xmm0);
        let xmm2 = reverse(xmm2);
        let xmm4 = reverse(xmm4);
        let xmm6 = reverse(xmm6);

        // Transpose xmm0-xmm7 into xmm8-xmm15 (word stage)
        let t8 = sse2.unpacklo_i16x8(xmm0, xmm1);
        let t9 = sse2.unpacklo_i16x8(xmm2, xmm3);
        let t10 = sse2.unpacklo_i16x8(xmm4, xmm5);
        let t11 = sse2.unpacklo_i16x8(xmm6, xmm7);
        let t12 = sse2.unpackhi_i16x8(xmm0, xmm1);
        let t13 = sse2.unpackhi_i16x8(xmm2, xmm3);
        let t14 = sse2.unpackhi_i16x8(xmm4, xmm5);
        let t15 = sse2.unpackhi_i16x8(xmm6, xmm7);

        // dword stage
        let u0 = sse2.unpacklo_i32x4(i32x4_from_i16x8(t8), i32x4_from_i16x8(t9));
        let u1 = sse2.unpacklo_i32x4(i32x4_from_i16x8(t10), i32x4_from_i16x8(t11));
        let u2 = sse2.unpackhi_i32x4(i32x4_from_i16x8(t8), i32x4_from_i16x8(t9));
        let u3 = sse2.unpackhi_i32x4(i32x4_from_i16x8(t10), i32x4_from_i16x8(t11));
        let u4 = sse2.unpacklo_i32x4(i32x4_from_i16x8(t12), i32x4_from_i16x8(t13));
        let u5 = sse2.unpacklo_i32x4(i32x4_from_i16x8(t14), i32x4_from_i16x8(t15));
        let u6 = sse2.unpackhi_i32x4(i32x4_from_i16x8(t12), i32x4_from_i16x8(t13));
        let u7 = sse2.unpackhi_i32x4(i32x4_from_i16x8(t14), i32x4_from_i16x8(t15));

        // qword stage
        let v8 = sse2.unpacklo_i64x2(i64x2_from_i32x4(u0), i64x2_from_i32x4(u1));
        let v9 = sse2.unpackhi_i64x2(i64x2_from_i32x4(u0), i64x2_from_i32x4(u1));
        let v10 = sse2.unpacklo_i64x2(i64x2_from_i32x4(u2), i64x2_from_i32x4(u3));
        let v11 = sse2.unpackhi_i64x2(i64x2_from_i32x4(u2), i64x2_from_i32x4(u3));
        let v12 = sse2.unpacklo_i64x2(i64x2_from_i32x4(u5), i64x2_from_i32x4(u4));
        let v13 = sse2.unpackhi_i64x2(i64x2_from_i32x4(u4), i64x2_from_i32x4(u5));
        let v14 = sse2.unpacklo_i64x2(i64x2_from_i32x4(u6), i64x2_from_i32x4(u7));
        let v15 = sse2.unpackhi_i64x2(i64x2_from_i32x4(u6), i64x2_from_i32x4(u7));

        // Rotate the rows to get the correct final order (v8, v12 need no rotation).
        let v9 = ssse3.alignr_u8x16::<2>(u8x16_from_i64x2(v9), u8x16_from_i64x2(v9));
        let v10 = ssse3.alignr_u8x16::<4>(u8x16_from_i64x2(v10), u8x16_from_i64x2(v10));
        let v11 = ssse3.alignr_u8x16::<6>(u8x16_from_i64x2(v11), u8x16_from_i64x2(v11));
        let v13 = ssse3.alignr_u8x16::<10>(u8x16_from_i64x2(v13), u8x16_from_i64x2(v13));
        let v14 = ssse3.alignr_u8x16::<12>(u8x16_from_i64x2(v14), u8x16_from_i64x2(v14));
        let v15 = ssse3.alignr_u8x16::<14>(u8x16_from_i64x2(v15), u8x16_from_i64x2(v15));

        // Widen each permuted row of 8 halves to 8 f32 with a single `vcvtph2ps`,
        // exactly as the OpenEXR original does.
        let store8 = |reg: [u16; 8], out: &mut [f32]| {
            out.copy_from_slice(&f16c.f16_to_f32x8(reg));
        };
        store8(u16x8_from_u8x16(u8x16_from_i64x2(v8)), &mut dst[0..8]);
        store8(u16x8_from_u8x16(v9), &mut dst[8..16]);
        store8(u16x8_from_u8x16(v10), &mut dst[16..24]);
        store8(u16x8_from_u8x16(v11), &mut dst[24..32]);
        store8(u16x8_from_u8x16(u8x16_from_i64x2(v12)), &mut dst[32..40]);
        store8(u16x8_from_u8x16(v13), &mut dst[40..48]);
        store8(u16x8_from_u8x16(v14), &mut dst[48..56]);
        store8(u16x8_from_u8x16(v15), &mut dst[56..64]);
    }
}

// Requires a host with AVX2 + F16C, hence gated behind the same opt-in feature
// as the DCT tier tests: `cargo test --lib --features avx2-tests -- zigzag`
#[cfg(all(test, feature = "avx2-tests"))]
mod test {
    use half::f16;
    use miraculix::x86::{
        detect_features,
        ops::{
            avx::f16c::F16c,
            sse::{sse2::Sse2, sse41::Sse41, ssse3::Ssse3},
        },
    };

    use super::{
        super::super::{quantization::ZIGZAG_ORDER, transfer_curve::to_linear_table},
        from_half_zigzag, write_row_f16, write_row_f32,
    };

    fn expect_avx2() -> (Sse2, Ssse3, Sse41, F16c) {
        let features = detect_features();
        (
            Sse2::from_features(features).expect("SSE2 SIMD mode requested, but unavailable"),
            Ssse3::from_features(features).expect("SSSE3 SIMD mode requested, but unavailable"),
            Sse41::from_features(features).expect("SSE4.1 SIMD mode requested, but unavailable"),
            F16c::from_features(features).expect("F16C requested, but unavailable"),
        )
    }

    /// The permuted+widened output must be bit-identical to the scalar gather,
    /// including NaN payloads `vcvtph2ps` and `half`'s conversion must not
    /// be allowed to disagree on any input. Every one of the 65536 possible
    /// half bit patterns is covered, in every one of the 64 lanes.
    #[test]
    fn zigzag_simd_matches_scalar() {
        let (sse2, ssse3, sse41, f16c) = expect_avx2();
        let mut simd = [0.0f32; 64];
        let mut scalar = [0.0f32; 64];

        for base in 0..=u16::MAX {
            let mut zig_zag = [0u16; 64];
            for (lane, slot) in zig_zag.iter_mut().enumerate() {
                // A stride coprime with 65536 walks every lane through every
                // bit pattern as `base` advances.
                *slot = base.wrapping_add(lane as u16 * 1013);
            }

            from_half_zigzag(sse2, ssse3, sse41, f16c, &zig_zag, &mut simd);
            for (slot, &src_index) in scalar.iter_mut().zip(ZIGZAG_ORDER.iter()) {
                *slot = f16::from_bits(zig_zag[src_index]).to_f32();
            }

            for (index, (&a, &b)) in simd.iter().zip(scalar.iter()).enumerate() {
                assert_eq!(a.to_bits(), b.to_bits(), "mismatch at index {index} for base {base}");
            }
        }
    }

    /// Scalar reference for the write-row SIMD path: exactly what
    /// `decode_lossy_dct_group`'s `write_row!` macro computes per pixel.
    fn scalar_linear_bits(value: f32, to_linear: Option<&[u16; 65536]>) -> u16 {
        let nonlinear = f16::from_f32(value);
        match to_linear {
            Some(table) => table[nonlinear.to_bits() as usize],
            None => nonlinear.to_bits(),
        }
    }

    /// A wide, pseudo-random sweep across the full f32 bit space
    fn sweep_rows() -> impl Iterator<Item = [f32; 8]> {
        let special = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            65504.0, // half::MAX
            65520.0, // rounds to infinity in half
            -65504.0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::from_bits(0x7fc00001),      // NaN, alternate payload
            f32::from_bits(0x00000001),      // smallest positive subnormal
            f32::from_bits(0x0000ffff),      // subnormal, near boundary
            6.104_f32.exp2() * f32::EPSILON, // arbitrary small-normal-ish value
        ];
        let special_rows: Vec<[f32; 8]> = special
            .chunks(8)
            .map(|chunk| {
                let mut row = [0.0f32; 8];
                row[..chunk.len()].copy_from_slice(chunk);
                row
            })
            .collect();

        let swept_rows = (0u32..=0xFFFF).map(|base| {
            std::array::from_fn(|lane| {
                // A large odd stride spreads `lane` across every exponent
                // range as `base` walks the low bits, unlike a plain +lane
                // which would only ever perturb the mantissa.
                f32::from_bits(base.wrapping_add(lane as u32 * 0x1000_0001))
            })
        });

        special_rows.into_iter().chain(swept_rows)
    }

    fn assert_bits_match(a: u16, b: u16, context: &str) {
        // NaN half bit patterns aren't unique (many payloads map to "NaN"),
        // so only require both sides agree on NaN-ness, matching `half`'s
        // own equality semantics; everything else must be bit-exact.
        let a_nan = f16::from_bits(a).is_nan();
        let b_nan = f16::from_bits(b).is_nan();
        if a_nan || b_nan {
            assert_eq!(
                a_nan, b_nan,
                "{}: one side is NaN, the other isn't (a=0x{:04x}, b=0x{:04x})",
                context, a, b
            );
        } else {
            assert_eq!(a, b, "{}: bit mismatch (a=0x{:04x}, b=0x{:04x})", context, a, b);
        }
    }

    #[test]
    fn write_row_f16_matches_scalar() {
        let (_, _, _, f16c) = expect_avx2();
        for to_linear in [None, Some(to_linear_table())] {
            for row in sweep_rows() {
                let mut simd = [0u8; 16];
                assert!(write_row_f16(f16c, &row, to_linear, &mut simd));

                for (lane, &value) in row.iter().enumerate() {
                    let expected = scalar_linear_bits(value, to_linear);
                    let actual = u16::from_le_bytes([simd[lane * 2], simd[lane * 2 + 1]]);
                    assert_bits_match(
                        actual,
                        expected,
                        &format!(
                            "f16 lane {lane}, value {value:e}, to_linear={}",
                            to_linear.is_some()
                        ),
                    );
                }
            }
        }
    }

    #[test]
    fn write_row_f32_matches_scalar() {
        let (_, _, _, f16c) = expect_avx2();
        for to_linear in [None, Some(to_linear_table())] {
            for row in sweep_rows() {
                let mut simd = [0u8; 32];
                assert!(write_row_f32(f16c, &row, to_linear, &mut simd));

                for (lane, &value) in row.iter().enumerate() {
                    let expected = f16::from_bits(scalar_linear_bits(value, to_linear)).to_f32();
                    let actual = f32::from_le_bytes([
                        simd[lane * 4],
                        simd[lane * 4 + 1],
                        simd[lane * 4 + 2],
                        simd[lane * 4 + 3],
                    ]);
                    if expected.is_nan() {
                        assert!(
                            actual.is_nan(),
                            "f32 lane {}, value {:e}: expected NaN, got {:e}",
                            lane,
                            value,
                            actual,
                        );
                    } else {
                        assert_eq!(
                            actual.to_bits(),
                            expected.to_bits(),
                            "f32 lane {}, value {:e}, to_linear={}",
                            lane,
                            value,
                            to_linear.is_some()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn write_row_falls_back_for_short_rows() {
        let (_, _, _, f16c) = expect_avx2();
        let row = [0.0f32; 7];
        let mut out16 = [0u8; 14];
        let mut out32 = [0u8; 28];
        assert!(!write_row_f16(f16c, &row, None, &mut out16));
        assert!(!write_row_f32(f16c, &row, None, &mut out32));
    }
}
