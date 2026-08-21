// AVX-512: DCT/CSC does 2 spatial blocks per 512-bit register;
// zigzag/DC-only/odd tail stay on AVX2 kernels. Write widens:
// `write_pair_block` covers an adjacent pair with one 16-lane convert instead
// of two 8-lane ones. Mixed DC-only in a pair, or a trailing odd block, falls
// back to the AVX2 single-block fused body.

use miraculix::x86::ops::{
    avx::{avx::Avx, f16c::F16c},
    avx512::{avx512dq::Avx512Dq, avx512f::Avx512f},
    sse::{sse2::Sse2, sse41::Sse41, ssse3::Ssse3},
};

use super::{
    super::{PackedStream, ScanlineTarget},
    ROUND_TO_NEAREST,
    avx2::{linearize_lanes, write_block, zigzag_block},
};
use crate::{
    compression::dwa::{
        color_space_conversion,
        discrete_cosine_transform::x86::{avx as dct_avx2, avx512dq as dct_avx512},
    },
    error::{Error, Result as ExrResult},
    meta::attribute::SampleType,
};

/// AVX-512 analog of `write_block` for a horizontally-adjacent pair of
/// already-inverted blocks (block B immediately right of block A, same block
/// row): each row-of-8 from A and row-of-8 from B land in one contiguous
/// 16-sample span of `out`, so one native `vcvtps2ph`/`vcvtph2ps` (AVX-512F,
/// no separate F16C needed) and one store cover both blocks at once instead
/// of two 8-wide passes. Callers must only take this path when the pair is
/// actually contiguous and block B is full-width: see the eligibility
/// check in `step_pair_or_single`.
#[inline(always)]
fn write_pair_block(
    avx512f: Avx512f,
    block_x0: usize,
    block_y: usize,
    y_count: usize,
    to_linear: Option<&[u16; 65536]>,
    dct_a: &[[f32; 64]; 3],
    dct_b: &[[f32; 64]; 3],
    targets: &mut [ScanlineTarget<'_>],
    out: &mut [u8],
) -> Option<Error> {
    for (component, target) in targets.iter_mut().enumerate() {
        let block_a = &dct_a[component];
        let block_b = &dct_b[component];
        let bytes_per_sample = target.sample_type.bytes_per_sample();
        for dy in 0..y_count {
            let y = block_y * 8 + dy;
            let row_a = &block_a[dy * 8..dy * 8 + 8];
            let row_b = &block_b[dy * 8..dy * 8 + 8];
            let offset = target.row_offsets[y] + block_x0 * 8 * bytes_per_sample;
            let out_row = &mut out[offset..][..16 * bytes_per_sample];

            let handled = match target.sample_type {
                SampleType::F16 => write_row16_f16(avx512f, row_a, row_b, to_linear, out_row),
                SampleType::F32 => write_row16_f32(avx512f, row_a, row_b, to_linear, out_row),
                SampleType::U32 => false,
            };
            if handled {
                continue;
            }

            // Unreachable in practice (DWA lossy DCT never carries U32
            // channels, mirrored from `write_block`'s same fallback), kept
            // for defensive symmetry rather than an `unreachable!()`.
            return Some(Error::unsupported("DWA lossy DCT compression of u32 channels"));
        }
    }
    None
}

// RGB component dual-port is **opt-in** (`dwa-avx512-rgb-comp-quad`): when R
// and G both need pair-iDCT, `inverse_quad(R/G)` then needs-match B. Microbench
// is a real win; whole-pipeline A/B regressed `lossy_dct` slightly : default
// stays the sequential 3x `inverse_pair` loop. Profile counters (under
// `dwa-profile`) record pair-step / quad-hit rate when the feature is on.
// Wrapped in `miraculix::avx512_fn!` (not a plain function): this body
// composes DCT's 8x8x2 register-transpose (unpack + shuffle + `Avx::
// permute2f128`/`Avx512Dq::extract`/`insert` recombine) with CSC's
// elementwise pass, dozens of chained token-method calls that need a shared
// `#[target_feature]` context to inline into real `zmm` code.
miraculix::avx512_fn! {
    #[allow(clippy::too_many_arguments)]
    fn decode_pair_dct_csc(
        avx512f: Avx512f,
        avx: Avx,
        avx512dq: Avx512Dq,
        components: usize,
        needs_a: [bool; 3],
        needs_b: [bool; 3],
        dct_a: &mut [[f32; 64]; 3],
        dct_b: &mut [[f32; 64]; 3],
    ) {
        let coef2 = dct_avx2::Coefficients::new(avx);
        let coef4 = dct_avx512::Coefficients::new(avx512f);

        #[cfg(feature = "dwa-profile")]
        if components == 3 {
            crate::compression::dwa::profile::DCT_RGB_PAIR_STEPS
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        #[cfg(feature = "dwa-avx512-rgb-comp-quad")]
        let use_rgb_comp_quad = components == 3
            && needs_a[0]
            && needs_b[0]
            && needs_a[1]
            && needs_b[1];
        #[cfg(not(feature = "dwa-avx512-rgb-comp-quad"))]
        let use_rgb_comp_quad = false;

        if use_rgb_comp_quad {
            #[cfg(feature = "dwa-profile")]
            crate::compression::dwa::profile::DCT_RGB_COMP_QUAD
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            let (a_r, a_rest) = dct_a.split_at_mut(1);
            let (a_g, a_b) = a_rest.split_at_mut(1);
            let (b_r, b_rest) = dct_b.split_at_mut(1);
            let (b_g, b_b) = b_rest.split_at_mut(1);
            dct_avx512::inverse_quad(
                avx512f,
                avx,
                avx512dq,
                &coef4,
                &mut a_r[0],
                &mut b_r[0],
                &mut a_g[0],
                &mut b_g[0],
            );
            match (needs_a[2], needs_b[2]) {
                (true, true) => {
                    dct_avx512::inverse_pair(
                        avx512f, avx, avx512dq, &coef4, &mut a_b[0], &mut b_b[0],
                    );
                }
                (true, false) => {
                    dct_avx2::inverse_one(avx, &coef2, &mut a_b[0]);
                }
                (false, true) => {
                    dct_avx2::inverse_one(avx, &coef2, &mut b_b[0]);
                }
                (false, false) => {}
            }
        } else {
            for component in 0..components {
                match (needs_a[component], needs_b[component]) {
                    (true, true) => {
                        dct_avx512::inverse_pair(
                            avx512f,
                            avx,
                            avx512dq,
                            &coef4,
                            &mut dct_a[component],
                            &mut dct_b[component],
                        );
                    }
                    (true, false) => {
                        dct_avx2::inverse_one(avx, &coef2, &mut dct_a[component]);
                    }
                    (false, true) => {
                        dct_avx2::inverse_one(avx, &coef2, &mut dct_b[component]);
                    }
                    (false, false) => {}
                }
            }
        }

        if components == 3 {
            color_space_conversion::x86::avx512f::inverse_pair(avx512f, dct_a, dct_b);
        }
    }
}

/// One step of the fused loop: a pair (2 blocks) via the 2-block DCT+CSC
/// kernel, or: if only one block remains: a single block via the AVX2
/// single-block kernel. Shared by `decode_group_fused` (every step) so the
/// pair path and its odd-block tail cannot diverge. Returns the next
/// `block_index` (advanced by 1 or 2) and any error.
#[inline]
#[allow(clippy::too_many_arguments)]
fn step_pair_or_single(
    sse2: Sse2,
    ssse3: Ssse3,
    sse41: Sse41,
    f16c: F16c,
    avx: Avx,
    avx512f: Avx512f,
    avx512dq: Avx512Dq,
    ac: &mut PackedStream<'_>,
    dc: &mut PackedStream<'_>,
    block_index: usize,
    block_count: usize,
    blocks_x: usize,
    width: usize,
    height: usize,
    components: usize,
    to_linear: Option<&[u16; 65536]>,
    targets: &mut [ScanlineTarget<'_>],
    out: &mut [u8],
) -> (usize, Option<Error>) {
    let mut dct_a = [[0.0f32; 64]; 3];
    let mut dct_b = [[0.0f32; 64]; 3];
    let mut needs_a = [false; 3];
    let mut needs_b = [false; 3];

    let block_x0 = block_index % blocks_x;
    let block_y0 = block_index / blocks_x;
    let x_count0 = 8.min(width - block_x0 * 8);
    let y_count0 = 8.min(height - block_y0 * 8);

    if let Err(e) = zigzag_block(
        sse2,
        ssse3,
        sse41,
        f16c,
        ac,
        dc,
        components,
        block_count,
        block_index,
        &mut dct_a,
        &mut needs_a,
    ) {
        return (block_index, Some(e));
    }

    let next_index = block_index + 1;
    if next_index < block_count {
        let block_x1 = next_index % blocks_x;
        let block_y1 = next_index / blocks_x;
        let x_count1 = 8.min(width - block_x1 * 8);
        let y_count1 = 8.min(height - block_y1 * 8);

        if let Err(e) = zigzag_block(
            sse2,
            ssse3,
            sse41,
            f16c,
            ac,
            dc,
            components,
            block_count,
            next_index,
            &mut dct_b,
            &mut needs_b,
        ) {
            return (next_index, Some(e));
        }

        decode_pair_dct_csc(
            avx512f, avx, avx512dq, components, needs_a, needs_b, &mut dct_a, &mut dct_b,
        );

        // Pair-write is only valid when B sits immediately right of A in the
        // same block row (so their output spans are contiguous) and B is
        // full-width (A is then necessarily full-width too, since only the
        // last column in a row can be a partial block).
        let pair_contiguous = block_y0 == block_y1 && block_x1 == block_x0 + 1 && x_count1 == 8;
        let write_err = if pair_contiguous {
            write_pair_block(
                avx512f, block_x0, block_y0, y_count0, to_linear, &dct_a, &dct_b, targets, out,
            )
        } else {
            let mut write_err = write_block(
                f16c, block_x0, block_y0, x_count0, y_count0, to_linear, &dct_a, targets, out,
            );
            if write_err.is_none() {
                write_err = write_block(
                    f16c, block_x1, block_y1, x_count1, y_count1, to_linear, &dct_b, targets, out,
                );
            }
            write_err
        };

        (block_index + 2, write_err)
    } else {
        // Odd trailing block: not worth a 2-block kernel for one block.
        let write_err = decode_single_dct_csc(
            f16c, avx, components, needs_a, &mut dct_a, block_x0, block_y0, x_count0, y_count0,
            to_linear, targets, out,
        );

        (block_index + 1, write_err)
    }
}

// The odd-trailing-block path's iDCT + optional CSC + write, extracted so
// `miraculix::avx_fn!` can wrap the whole thing: composes `dct_avx2::
// inverse_one`'s register transpose with CSC's elementwise pass, needing a
// shared `#[target_feature]` context.
miraculix::avx_fn! {
    #[allow(clippy::too_many_arguments)]
    fn decode_single_dct_csc(
        f16c: F16c,
        avx: Avx,
        components: usize,
        needs_a: [bool; 3],
        dct_a: &mut [[f32; 64]; 3],
        block_x0: usize,
        block_y0: usize,
        x_count0: usize,
        y_count0: usize,
        to_linear: Option<&[u16; 65536]>,
        targets: &mut [ScanlineTarget<'_>],
        out: &mut [u8],
    ) -> Option<Error> {
        let coef = dct_avx2::Coefficients::new(avx);
        for component in 0..components {
            if needs_a[component] {
                dct_avx2::inverse_one(avx, &coef, &mut dct_a[component]);
            }
        }

        if components == 3 {
            color_space_conversion::x86::avx::inverse_one(avx, dct_a);
        }

        write_block(f16c, block_x0, block_y0, x_count0, y_count0, to_linear, dct_a, targets, out)
    }
}

// Spatial 4-block `inverse_quad` fused decode (zigzagx4 then DCTx4) was A/B'd
// 2026-07-27 and reverted: isolated DCT +6.4% did not survive whole-pipeline
// (noise only) because the doubled zigzag-before-DCT working set ate the win.
// Component-level R/G dual-port is a different idea (same spatial pair, no
// extra zigzag): micro +6-11%, pipeline flat-to-worse across sessions -> opt-in
// only via `dwa-avx512-rgb-comp-quad`. Production default is sequential 3x
// inverse_pair.

/// AVX-512 analog of `avx2::decode_group_fused`: same fused shape, but the
/// DCT/CSC step processes 2 spatial blocks at once (one 512-bit register per
/// step). Zigzag and write stay on the single-block AVX2+F16C kernels, except
/// write widens to 16 lanes for horizontally-adjacent pairs
/// (`write_pair_block`).
#[allow(clippy::too_many_arguments)]
pub(super) fn decode_group_fused(
    sse2: Sse2,
    ssse3: Ssse3,
    sse41: Sse41,
    f16c: F16c,
    avx: Avx,
    avx512f: Avx512f,
    avx512dq: Avx512Dq,
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

    let mut block_index = 0usize;
    while block_index < block_count {
        let (next, err) = step_pair_or_single(
            sse2,
            ssse3,
            sse41,
            f16c,
            avx,
            avx512f,
            avx512dq,
            ac,
            dc,
            block_index,
            block_count,
            blocks_x,
            width,
            height,
            components,
            to_linear,
            targets,
            out,
        );
        if let Some(e) = err {
            return Err(e);
        }
        block_index = next;
    }

    dc.advance(components * block_count);
    Ok(())
}

// 16-lane analog of `avx2::write_row_f16`: block A's row in lanes 0-7,
// block B's row in lanes 8-15. `f32_to_f16x16`/`f16_to_f32x16` are AVX-512F's
// own native half conversion (unlike the AVX2 path, no separate F16C
// capability check needed). The `to_linear` gather is `linearize_lanes`
// unchanged at 16 lanes: a miraculix register is a plain array, so unlike
// the pre-port pulp code (which needed a real gather instruction or a
// GPR extract/lookup/insert roundtrip it didn't have below AVX-512), this is
// just per-lane indexing at whatever width the caller passes.
miraculix::avx512f_fn! {
    fn write_row16_f16(
        avx512f: Avx512f,
        row_a: &[f32],
        row_b: &[f32],
        to_linear: Option<&[u16; 65536]>,
        out_row: &mut [u8],
    ) -> bool {
        let Ok(&a) = std::convert::TryInto::<&[f32; 8]>::try_into(row_a) else {
            return false;
        };
        let Ok(&b) = std::convert::TryInto::<&[f32; 8]>::try_into(row_b) else {
            return false;
        };
        if out_row.len() != 32 {
            return false;
        }

        let combined: [f32; 16] = std::array::from_fn(|i| if i < 8 { a[i] } else { b[i - 8] });
        let nonlinear = avx512f.f32_to_f16x16::<ROUND_TO_NEAREST>(combined);

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

// Same as `write_row16_f16`, but widens the linearized halves back to f32
// via `f16_to_f32x16` (again native AVX-512F, one 16-lane instruction
// covering both blocks), for F32-sample-type channels.
miraculix::avx512f_fn! {
    fn write_row16_f32(
        avx512f: Avx512f,
        row_a: &[f32],
        row_b: &[f32],
        to_linear: Option<&[u16; 65536]>,
        out_row: &mut [u8],
    ) -> bool {
        let Ok(&a) = std::convert::TryInto::<&[f32; 8]>::try_into(row_a) else {
            return false;
        };
        let Ok(&b) = std::convert::TryInto::<&[f32; 8]>::try_into(row_b) else {
            return false;
        };
        if out_row.len() != 64 {
            return false;
        }

        let combined: [f32; 16] = std::array::from_fn(|i| if i < 8 { a[i] } else { b[i - 8] });
        let nonlinear = avx512f.f32_to_f16x16::<ROUND_TO_NEAREST>(combined);

        let linear = match to_linear {
            Some(table) => linearize_lanes(nonlinear, table),
            None => nonlinear,
        };

        let widened = avx512f.f16_to_f32x16(linear);
        for (chunk, &value) in out_row.chunks_exact_mut(4).zip(widened.iter()) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        true
    }
}

// AVX-512 write-pair correctness tests. Opt-in via `avx512-tests`, same
// convention as the DCT/CSC AVX-512 test modules (`expect_avx512` panics
// rather than skipping: this project's dev/bench host always has AVX-512).
#[cfg(all(test, feature = "avx512-tests"))]
mod test {
    use half::f16;
    use miraculix::x86::{
        detect_features,
        ops::{avx::f16c::F16c, avx512::avx512f::Avx512f},
    };

    use super::{
        super::super::{ScanlineTarget, transfer_curve::to_linear_table},
        write_block, write_pair_block, write_row16_f16, write_row16_f32,
    };
    use crate::meta::attribute::SampleType;

    fn expect_avx512() -> Avx512f {
        Avx512f::from_features(detect_features())
            .expect("AVX-512 SIMD mode requested, but the AVX-512 tier is unavailable")
    }

    fn scalar_linear_bits(value: f32, to_linear: Option<&[u16; 65536]>) -> u16 {
        let nonlinear = f16::from_f32(value);
        match to_linear {
            Some(table) => table[nonlinear.to_bits() as usize],
            None => nonlinear.to_bits(),
        }
    }

    /// Same sweep shape as the AVX2 8-lane tests, but two independent rows
    /// (block A's, block B's) at once, offset by a stride coprime with the
    /// sweep length so A and B cover uncorrelated value regions together.
    fn sweep_row_pairs() -> impl Iterator<Item = ([f32; 8], [f32; 8])> {
        let special = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            65504.0,
            65520.0,
            -65504.0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::from_bits(0x7fc00001),
            f32::from_bits(0x00000001),
            f32::from_bits(0x0000ffff),
            6.104_f32.exp2() * f32::EPSILON,
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
            std::array::from_fn(|lane| f32::from_bits(base.wrapping_add(lane as u32 * 0x1000_0001)))
        });

        let rows_a: Vec<[f32; 8]> = special_rows.into_iter().chain(swept_rows).collect();
        let len = rows_a.len();
        let rows_b: Vec<[f32; 8]> = (0..len).map(|i| rows_a[(i + 6151) % len]).collect();
        rows_a.into_iter().zip(rows_b)
    }

    fn assert_bits_match(a: u16, b: u16, context: &str) {
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
    fn write_row16_f16_matches_scalar() {
        let avx512f = expect_avx512();
        for to_linear in [None, Some(to_linear_table())] {
            for (row_a, row_b) in sweep_row_pairs() {
                let mut simd = [0u8; 32];
                assert!(write_row16_f16(avx512f, &row_a, &row_b, to_linear, &mut simd));

                for (lane, &value) in row_a.iter().chain(row_b.iter()).enumerate() {
                    let expected = scalar_linear_bits(value, to_linear);
                    let actual = u16::from_le_bytes([simd[lane * 2], simd[lane * 2 + 1]]);
                    assert_bits_match(
                        actual,
                        expected,
                        &format!("f16 lane {lane}, value {value:e}"),
                    );
                }
            }
        }
    }

    #[test]
    fn write_row16_f32_matches_scalar() {
        let avx512f = expect_avx512();
        for to_linear in [None, Some(to_linear_table())] {
            for (row_a, row_b) in sweep_row_pairs() {
                let mut simd = [0u8; 64];
                assert!(write_row16_f32(avx512f, &row_a, &row_b, to_linear, &mut simd));

                for (lane, &value) in row_a.iter().chain(row_b.iter()).enumerate() {
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
                            "f32 lane {lane}, value {value:e}"
                        );
                    }
                }
            }
        }
    }

    /// End-to-end: `write_pair_block` (the function actually wired into the
    /// fused AVX-512 decode path) must produce byte-identical output to
    /// calling the single-block `write_block` twice: proves the pair-write
    /// eligibility wiring in `step_pair_or_single` slices rows and offsets
    /// the same way, on top of the row-level bit-exactness above.
    #[test]
    fn write_pair_block_matches_two_write_block_calls() {
        let avx512f = expect_avx512();
        let f16c = F16c::from_features(detect_features()).expect("F16C requested but unavailable");

        let width = 16usize;
        let height = 8usize;
        let bytes_per_sample = SampleType::F16.bytes_per_sample();
        let row_offsets: Vec<usize> = (0..height).map(|y| y * width * bytes_per_sample).collect();

        let mut dct_a = [[0.0f32; 64]; 3];
        let mut dct_b = [[0.0f32; 64]; 3];
        for i in 0..64 {
            dct_a[0][i] = f32::from_bits(0x3f00_0000u32.wrapping_add(i as u32 * 0x0010_0000));
            dct_b[0][i] = f32::from_bits(0xbf00_0000u32.wrapping_add(i as u32 * 0x0020_0000));
        }

        for to_linear in [None, Some(to_linear_table())] {
            let mut out_pair = vec![0u8; height * width * bytes_per_sample];
            let mut out_two = vec![0u8; height * width * bytes_per_sample];

            let mut targets_pair = [ScanlineTarget {
                sample_type: SampleType::F16,
                row_offsets: &row_offsets,
            }];
            let err = write_pair_block(
                avx512f,
                0,
                0,
                height,
                to_linear,
                &dct_a,
                &dct_b,
                &mut targets_pair,
                &mut out_pair,
            );
            assert!(err.is_none());

            let mut targets_two = [ScanlineTarget {
                sample_type: SampleType::F16,
                row_offsets: &row_offsets,
            }];
            let err = write_block(
                f16c,
                0,
                0,
                8,
                height,
                to_linear,
                &dct_a,
                &mut targets_two,
                &mut out_two,
            );
            assert!(err.is_none());
            let mut targets_two = [ScanlineTarget {
                sample_type: SampleType::F16,
                row_offsets: &row_offsets,
            }];
            let err = write_block(
                f16c,
                1,
                0,
                8,
                height,
                to_linear,
                &dct_b,
                &mut targets_two,
                &mut out_two,
            );
            assert!(err.is_none());

            assert_eq!(out_pair, out_two, "to_linear={}", to_linear.is_some());
        }
    }
}
