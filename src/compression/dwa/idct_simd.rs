// x86_64 SSE2 port of OpenEXR's dctInverse8x8_sse2 (internal_dwa_simd.h,
// zeroedRows == 0, the only case this decoder needs). Not just a constants
// swap: OpenEXR's SIMD groups the row/column sums differently than its
// scalar path (tree reduction vs. left-to-right chain), and float addition
// isn't associative, so exact instruction order - not just precision -
// determines the result (see `dct_inverse_8x8_scalar` in idct.rs).
//
// Gated behind the `dwa_simd_identical` feature (Cargo.toml): the only
// unsafe code in this crate.

use core::arch::x86_64::*;

#[target_feature(enable = "sse2")]
unsafe fn dct_inverse_8x8_sse2_inner(data: &mut [f32; 64]) {
    let a = _mm_set1_ps(3.535536e-1);
    let b = _mm_set1_ps(4.903927e-1);
    let c = _mm_set1_ps(4.619398e-1);
    let d = _mm_set1_ps(4.157349e-1);
    let e = _mm_set1_ps(2.777855e-1);
    let f = _mm_set1_ps(1.913422e-1);
    let g = _mm_set1_ps(9.754573e-2);

    let c0 = _mm_set1_ps(3.535536e-1);
    let c1 = _mm_setr_ps(4.619398e-1, 1.913422e-1, -1.913422e-1, -4.619398e-1);
    let c2 = _mm_setr_ps(3.535536e-1, -3.535536e-1, -3.535536e-1, 3.535536e-1);
    let c3 = _mm_setr_ps(1.913422e-1, -4.619398e-1, 4.619398e-1, -1.913422e-1);

    let c4 = _mm_setr_ps(4.903927e-1, 4.157349e-1, 2.777855e-1, 9.754573e-2);
    let c5 = _mm_setr_ps(4.157349e-1, -9.754573e-2, -4.903927e-1, -2.777855e-1);
    let c6 = _mm_setr_ps(2.777855e-1, -4.903927e-1, 9.754573e-2, 4.157349e-1);
    let c7 = _mm_setr_ps(9.754573e-2, -2.777855e-1, 4.157349e-1, -4.903927e-1);

    // `data` (a stack-local [f32; 64]) is not guaranteed to be 16-byte
    // aligned, unlike OpenEXR's heap-allocated, SSE-aligned rowBlockHandle -
    // so loads/stores are unaligned. This affects only which instruction is
    // used to fetch/store memory, not any computed value.
    let ptr = data.as_mut_ptr();
    let mut src_vec: [__m128; 16] = core::array::from_fn(|k| _mm_loadu_ps(ptr.add(k * 4)));

    // Rows.
    for i in 0..8 {
        let v0 = src_vec[2 * i];
        let v1 = src_vec[2 * i + 1];

        let x0 = _mm_mul_ps(_mm_shuffle_ps::<0x00>(v0, v0), c0);
        let x2 = _mm_mul_ps(_mm_shuffle_ps::<0xaa>(v0, v0), c1);
        let x4 = _mm_mul_ps(_mm_shuffle_ps::<0x00>(v1, v1), c2);
        let x6 = _mm_mul_ps(_mm_shuffle_ps::<0xaa>(v1, v1), c3);

        let x1 = _mm_mul_ps(_mm_shuffle_ps::<0x55>(v0, v0), c4);
        let x3 = _mm_mul_ps(_mm_shuffle_ps::<0xff>(v0, v0), c5);
        let x5 = _mm_mul_ps(_mm_shuffle_ps::<0x55>(v1, v1), c6);
        let x7 = _mm_mul_ps(_mm_shuffle_ps::<0xff>(v1, v1), c7);

        // Matches the C macro's strictly sequential evenSum += x0; += x2; += x4; += x6;
        let even_sum = _mm_add_ps(_mm_add_ps(_mm_add_ps(_mm_setzero_ps(), x0), x2), x4);
        let even_sum = _mm_add_ps(even_sum, x6);

        let odd_sum = _mm_add_ps(_mm_add_ps(_mm_add_ps(_mm_setzero_ps(), x1), x3), x5);
        let odd_sum = _mm_add_ps(odd_sum, x7);

        src_vec[2 * i] = _mm_add_ps(even_sum, odd_sum);
        let hi = _mm_sub_ps(even_sum, odd_sum);
        src_vec[2 * i + 1] = _mm_shuffle_ps::<0x1b>(hi, hi);
    }

    // Columns, processed in two batches of 4 (col = 0 handles columns 0-3,
    // col = 1 handles columns 4-7).
    for col in 0..2 {
        let in0 = src_vec[col];
        let in1 = src_vec[2 + col];
        let in2 = src_vec[4 + col];
        let in3 = src_vec[6 + col];
        let in4 = src_vec[8 + col];
        let in5 = src_vec[10 + col];
        let in6 = src_vec[12 + col];
        let in7 = src_vec[14 + col];

        let alpha0 = _mm_mul_ps(c, in2);
        let alpha1 = _mm_mul_ps(f, in2);
        let alpha2 = _mm_mul_ps(c, in6);
        let alpha3 = _mm_mul_ps(f, in6);

        let beta0 = _mm_add_ps(
            _mm_add_ps(_mm_mul_ps(in1, b), _mm_mul_ps(in3, d)),
            _mm_add_ps(_mm_mul_ps(in5, e), _mm_mul_ps(in7, g)),
        );
        let beta1 = _mm_sub_ps(
            _mm_sub_ps(_mm_mul_ps(in1, d), _mm_mul_ps(in3, g)),
            _mm_add_ps(_mm_mul_ps(in5, b), _mm_mul_ps(in7, e)),
        );
        let beta2 = _mm_add_ps(
            _mm_sub_ps(_mm_mul_ps(in1, e), _mm_mul_ps(in3, b)),
            _mm_add_ps(_mm_mul_ps(in5, g), _mm_mul_ps(in7, d)),
        );
        let beta3 = _mm_add_ps(
            _mm_sub_ps(_mm_mul_ps(in1, g), _mm_mul_ps(in3, e)),
            _mm_sub_ps(_mm_mul_ps(in5, d), _mm_mul_ps(in7, b)),
        );

        let theta0 = _mm_mul_ps(a, _mm_add_ps(in0, in4));
        let theta3 = _mm_mul_ps(a, _mm_sub_ps(in0, in4));
        let theta1 = _mm_add_ps(alpha0, alpha3);
        let theta2 = _mm_sub_ps(alpha1, alpha2);

        let gamma0 = _mm_add_ps(theta0, theta1);
        let gamma1 = _mm_add_ps(theta3, theta2);
        let gamma2 = _mm_sub_ps(theta3, theta2);
        let gamma3 = _mm_sub_ps(theta0, theta1);

        src_vec[col] = _mm_add_ps(gamma0, beta0);
        src_vec[2 + col] = _mm_add_ps(gamma1, beta1);
        src_vec[4 + col] = _mm_add_ps(gamma2, beta2);
        src_vec[6 + col] = _mm_add_ps(gamma3, beta3);
        src_vec[8 + col] = _mm_sub_ps(gamma3, beta3);
        src_vec[10 + col] = _mm_sub_ps(gamma2, beta2);
        src_vec[12 + col] = _mm_sub_ps(gamma1, beta1);
        src_vec[14 + col] = _mm_sub_ps(gamma0, beta0);
    }

    for (k, v) in src_vec.iter().enumerate() {
        _mm_storeu_ps(ptr.add(k * 4), *v);
    }
}

// AVX port of OpenEXR's dctInverse8x8_avx_0 - OpenEXR's own hand-written GCC
// inline assembly (IDCT_AVX_BODY in internal_dwa_simd.h), not compiler
// auto-vectorization. It restructures the 8x8 transform around 256-bit
// registers holding pairs of rows (then, after a mid-function transpose via
// vperm2f128, columns). This is what real OpenEXR actually dispatches to on
// any AVX-capable CPU (avx > sse2 > scalar priority in `initializeFuncs`),
// so `dct_inverse_8x8_sse2_inner` above is not what a typical real build
// uses. No FMA is involved anywhere (verified via disassembly).
//
// Transcribed 1:1 from IDCT_AVX_BODY, preserving exact operand order
// (including non-commutative subtractions) - float addition isn't
// associative, so a semantically-equivalent but reordered translation would
// reintroduce the same rounding gap this port exists to close.
#[target_feature(enable = "avx")]
unsafe fn dct_inverse_8x8_avx_inner(data: &mut [f32; 64]) {
    // sAvxCoef (internal_dwa_simd.h): column-major M1 then M2.
    //   M1 = [ a  c  a  f ]      M2 = [ b  d  e  g ]
    //        [ a  f -a -c ]           [ d -g -b -e ]
    //        [ a -f -a  c ]           [ e -b  g  d ]
    //        [ a -c  a -f ]           [ g -e  d -b ]
    const COEF: [f32; 32] = [
        3.535536e-1,
        3.535536e-1,
        3.535536e-1,
        3.535536e-1,
        4.619398e-1,
        1.913422e-1,
        -1.913422e-1,
        -4.619398e-1,
        3.535536e-1,
        -3.535536e-1,
        -3.535536e-1,
        3.535536e-1,
        1.913422e-1,
        -4.619398e-1,
        4.619398e-1,
        -1.913422e-1,
        4.903927e-1,
        4.157349e-1,
        2.777855e-1,
        9.754573e-2,
        4.157349e-1,
        -9.754573e-2,
        -4.903927e-1,
        -2.777855e-1,
        2.777855e-1,
        -4.903927e-1,
        9.754573e-2,
        4.157349e-1,
        9.754573e-2,
        -2.777855e-1,
        4.157349e-1,
        -4.903927e-1,
    ];

    let ptr = data.as_ptr();

    // --- Row 1D DCT setup (IDCT_AVX_SETUP_2_ROWS): load each pair of rows,
    // producing one register holding the even-indexed columns of both rows
    // and one holding the odd-indexed columns (low 128 bits = first row,
    // high 128 bits = second row).
    let r0_00 = _mm_loadu_ps(ptr.add(0));
    let r0_01 = _mm_loadu_ps(ptr.add(4));
    let r0_t0 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r0_00), _mm_loadu_ps(ptr.add(8)));
    let r0_t1 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r0_01), _mm_loadu_ps(ptr.add(12)));
    let r0_d0 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r0_t0), _mm256_castps_pd(r0_t1)));
    let r0_d1 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r0_t0), _mm256_castps_pd(r0_t1)));
    let r0_u0 = _mm256_unpacklo_ps(r0_d0, r0_d1);
    let r0_u1 = _mm256_unpackhi_ps(r0_d0, r0_d1);
    let mut y0 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r0_u0), _mm256_castps_pd(r0_u1)));
    let mut y4 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r0_u0), _mm256_castps_pd(r0_u1)));

    let r1_00 = _mm_loadu_ps(ptr.add(16));
    let r1_01 = _mm_loadu_ps(ptr.add(20));
    let r1_t0 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r1_00), _mm_loadu_ps(ptr.add(24)));
    let r1_t1 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r1_01), _mm_loadu_ps(ptr.add(28)));
    let r1_d0 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r1_t0), _mm256_castps_pd(r1_t1)));
    let r1_d1 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r1_t0), _mm256_castps_pd(r1_t1)));
    let r1_u0 = _mm256_unpacklo_ps(r1_d0, r1_d1);
    let r1_u1 = _mm256_unpackhi_ps(r1_d0, r1_d1);
    let mut y1 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r1_u0), _mm256_castps_pd(r1_u1)));
    let mut y5 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r1_u0), _mm256_castps_pd(r1_u1)));

    let r2_00 = _mm_loadu_ps(ptr.add(32));
    let r2_01 = _mm_loadu_ps(ptr.add(36));
    let r2_t0 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r2_00), _mm_loadu_ps(ptr.add(40)));
    let r2_t1 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r2_01), _mm_loadu_ps(ptr.add(44)));
    let r2_d0 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r2_t0), _mm256_castps_pd(r2_t1)));
    let r2_d1 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r2_t0), _mm256_castps_pd(r2_t1)));
    let r2_u0 = _mm256_unpacklo_ps(r2_d0, r2_d1);
    let r2_u1 = _mm256_unpackhi_ps(r2_d0, r2_d1);
    let mut y2 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r2_u0), _mm256_castps_pd(r2_u1)));
    let mut y6 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r2_u0), _mm256_castps_pd(r2_u1)));

    let r3_00 = _mm_loadu_ps(ptr.add(48));
    let r3_01 = _mm_loadu_ps(ptr.add(52));
    let r3_t0 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r3_00), _mm_loadu_ps(ptr.add(56)));
    let r3_t1 = _mm256_insertf128_ps::<1>(_mm256_castps128_ps256(r3_01), _mm_loadu_ps(ptr.add(60)));
    let r3_d0 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r3_t0), _mm256_castps_pd(r3_t1)));
    let r3_d1 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r3_t0), _mm256_castps_pd(r3_t1)));
    let r3_u0 = _mm256_unpacklo_ps(r3_d0, r3_d1);
    let r3_u1 = _mm256_unpackhi_ps(r3_d0, r3_d1);
    let mut y3 =
        _mm256_castpd_ps(_mm256_unpacklo_pd(_mm256_castps_pd(r3_u0), _mm256_castps_pd(r3_u1)));
    let mut y7 =
        _mm256_castpd_ps(_mm256_unpackhi_pd(_mm256_castps_pd(r3_u0), _mm256_castps_pd(r3_u1)));

    // Row transform: even columns (y0..y3) against M1.
    let c0 = _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr()), _mm_loadu_ps(COEF.as_ptr()));
    let c1 =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(4)), _mm_loadu_ps(COEF.as_ptr().add(4)));
    let c2 =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(8)), _mm_loadu_ps(COEF.as_ptr().add(8)));
    let c3 =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(12)), _mm_loadu_ps(COEF.as_ptr().add(12)));

    y0 = idct_avx_mmult(y0, c0, c1, c2, c3);
    y1 = idct_avx_mmult(y1, c0, c1, c2, c3);
    y2 = idct_avx_mmult(y2, c0, c1, c2, c3);
    y3 = idct_avx_mmult(y3, c0, c1, c2, c3);

    // Row transform: odd columns (y4..y7) against M2.
    let c4 =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(16)), _mm_loadu_ps(COEF.as_ptr().add(16)));
    let c5 =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(20)), _mm_loadu_ps(COEF.as_ptr().add(20)));
    let c6 =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(24)), _mm_loadu_ps(COEF.as_ptr().add(24)));
    let c7 =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(28)), _mm_loadu_ps(COEF.as_ptr().add(28)));

    y4 = idct_avx_mmult(y4, c4, c5, c6, c7);
    y5 = idct_avx_mmult(y5, c4, c5, c6, c7);
    y6 = idct_avx_mmult(y6, c4, c5, c6, c7);
    y7 = idct_avx_mmult(y7, c4, c5, c6, c7);

    // EO_TO_ROW_HALVES: front = even+odd, back = reverse(even-odd).
    let sub04 = _mm256_sub_ps(y0, y4);
    let front0 = _mm256_add_ps(y0, y4);
    let back12 = _mm256_permute_ps::<0x1b>(sub04);

    let sub15 = _mm256_sub_ps(y1, y5);
    let front1 = _mm256_add_ps(y1, y5);
    let back13 = _mm256_permute_ps::<0x1b>(sub15);

    let sub26 = _mm256_sub_ps(y2, y6);
    let front2 = _mm256_add_ps(y2, y6);
    let back14 = _mm256_permute_ps::<0x1b>(sub26);

    let sub37 = _mm256_sub_ps(y3, y7);
    let front3 = _mm256_add_ps(y3, y7);
    let back15 = _mm256_permute_ps::<0x1b>(sub37);

    // Reassemble row-halves into column-major registers for the column pass.
    let cp7 = _mm256_permute2f128_ps::<0x13>(back15, front3);
    let cp6 = _mm256_permute2f128_ps::<0x02>(back15, front3);
    let cp5 = _mm256_permute2f128_ps::<0x13>(back14, front2);
    let cp4 = _mm256_permute2f128_ps::<0x02>(back14, front2);
    let cp3 = _mm256_permute2f128_ps::<0x13>(back13, front1);
    let cp2 = _mm256_permute2f128_ps::<0x02>(back13, front1);
    let cp1 = _mm256_permute2f128_ps::<0x13>(back12, front0);
    let cp0 = _mm256_permute2f128_ps::<0x02>(back12, front0);

    // Column pass: M2 broadcasts (b, d, e, g) reused directly from c4, which
    // still holds M2's first column (matching the real asm, which leaves the
    // last-loaded M2 broadcast sitting in %ymm8 and extracts from it instead
    // of reloading from memory).
    let cb = _mm256_permute_ps::<0x00>(c4);
    let cd = _mm256_permute_ps::<0x55>(c4);
    let ce = _mm256_permute_ps::<0xaa>(c4);
    let cg = _mm256_permute_ps::<0xff>(c4);

    // beta0 = (g*in7 + e*in5) + (d*in3 + b*in1)
    let m_b1 = _mm256_mul_ps(cb, cp1);
    let m_d3 = _mm256_mul_ps(cd, cp3);
    let m_e5 = _mm256_mul_ps(ce, cp5);
    let m_g7 = _mm256_mul_ps(cg, cp7);
    let s_d3b1 = _mm256_add_ps(m_d3, m_b1);
    let s_g7e5 = _mm256_add_ps(m_g7, m_e5);
    let beta0 = _mm256_add_ps(s_g7e5, s_d3b1);

    // beta1 = (d*in1 - (b*in5 + g*in3)) - e*in7
    let m_d1 = _mm256_mul_ps(cd, cp1);
    let m_g3 = _mm256_mul_ps(cg, cp3);
    let m_b5 = _mm256_mul_ps(cb, cp5);
    let s_b5g3 = _mm256_add_ps(m_b5, m_g3);
    let t_beta1 = _mm256_sub_ps(m_d1, s_b5g3);
    let m_e7 = _mm256_mul_ps(ce, cp7);
    let beta1 = _mm256_sub_ps(t_beta1, m_e7);

    // beta2 = ((e*in1 - b*in3) + g*in5) + d*in7
    let m_e1 = _mm256_mul_ps(ce, cp1);
    let m_b3 = _mm256_mul_ps(cb, cp3);
    let t1_beta2 = _mm256_sub_ps(m_e1, m_b3);
    let m_g5 = _mm256_mul_ps(cg, cp5);
    let t2_beta2 = _mm256_add_ps(m_g5, t1_beta2);
    let m_d7 = _mm256_mul_ps(cd, cp7);
    let beta2 = _mm256_add_ps(m_d7, t2_beta2);

    // beta3 = (d*in5 + g*in1) - (b*in7 + e*in3)
    let m_g1 = _mm256_mul_ps(cg, cp1);
    let m_e3 = _mm256_mul_ps(ce, cp3);
    let m_d5 = _mm256_mul_ps(cd, cp5);
    let m_b7 = _mm256_mul_ps(cb, cp7);
    let s_d5g1 = _mm256_add_ps(m_d5, m_g1);
    let s_b7e3 = _mm256_add_ps(m_b7, m_e3);
    let beta3 = _mm256_sub_ps(s_d5g1, s_b7e3);

    // Reload the M1 broadcast source: bytes 8..24 of COEF = elements [2..6) = {a, a, c, f}.
    let m1_bcast =
        _mm256_set_m128(_mm_loadu_ps(COEF.as_ptr().add(2)), _mm_loadu_ps(COEF.as_ptr().add(2)));
    let cf = _mm256_permute_ps::<0xff>(m1_bcast);
    let cc = _mm256_permute_ps::<0xaa>(m1_bcast);
    let ca = _mm256_permute_ps::<0x00>(m1_bcast);

    // theta0 = a*in4 + a*in0, theta3 = a*in0 - a*in4
    let a_in0 = _mm256_mul_ps(ca, cp0);
    let a_in4 = _mm256_mul_ps(ca, cp4);
    let theta0 = _mm256_add_ps(a_in4, a_in0);
    let theta3 = _mm256_sub_ps(a_in0, a_in4);

    // theta1 = f*in6 + c*in2; gamma0 = theta1+theta0; gamma3 = theta0-theta1
    let c_in2 = _mm256_mul_ps(cc, cp2);
    let f_in6 = _mm256_mul_ps(cf, cp6);
    let theta1 = _mm256_add_ps(f_in6, c_in2);
    let gamma0 = _mm256_add_ps(theta1, theta0);
    let gamma3 = _mm256_sub_ps(theta0, theta1);

    // theta2 = f*in2 - c*in6; gamma1 = theta3+theta2; gamma2 = theta3-theta2
    let f_in2 = _mm256_mul_ps(cf, cp2);
    let c_in6 = _mm256_mul_ps(cc, cp6);
    let theta2 = _mm256_sub_ps(f_in2, c_in6);
    let gamma1 = _mm256_add_ps(theta3, theta2);
    let gamma2 = _mm256_sub_ps(theta3, theta2);

    let out0 = _mm256_add_ps(gamma0, beta0);
    let out1 = _mm256_add_ps(gamma1, beta1);
    let out2 = _mm256_add_ps(gamma2, beta2);
    let out3 = _mm256_add_ps(gamma3, beta3);
    let out7 = _mm256_sub_ps(gamma0, beta0);
    let out6 = _mm256_sub_ps(gamma1, beta1);
    let out5 = _mm256_sub_ps(gamma2, beta2);
    let out4 = _mm256_sub_ps(gamma3, beta3);

    let out_ptr = data.as_mut_ptr();
    _mm256_storeu_ps(out_ptr, out0);
    _mm256_storeu_ps(out_ptr.add(8), out1);
    _mm256_storeu_ps(out_ptr.add(16), out2);
    _mm256_storeu_ps(out_ptr.add(24), out3);
    _mm256_storeu_ps(out_ptr.add(32), out4);
    _mm256_storeu_ps(out_ptr.add(40), out5);
    _mm256_storeu_ps(out_ptr.add(48), out6);
    _mm256_storeu_ps(out_ptr.add(56), out7);
}

/// IDCT_AVX_MMULT_ROWS: broadcast `src`'s 4 lanes (independently per 128-bit
/// half) into 4 vectors, multiply each by the corresponding coefficient
/// column, and sum: `c8*src[0] + c9*src[1] + c10*src[2] + c11*src[3]`
/// (grouped as `(c10*src[2]+c11*src[3]) + (c8*src[0]+c9*src[1])`, matching
/// the exact instruction order in the real assembly).
#[target_feature(enable = "avx")]
unsafe fn idct_avx_mmult(src: __m256, c8: __m256, c9: __m256, c10: __m256, c11: __m256) -> __m256 {
    let p0 = _mm256_permute_ps::<0x00>(src);
    let p1 = _mm256_permute_ps::<0x55>(src);
    let p2 = _mm256_permute_ps::<0xaa>(src);
    let p3 = _mm256_permute_ps::<0xff>(src);
    let m0 = _mm256_mul_ps(p0, c8);
    let m1 = _mm256_mul_ps(p1, c9);
    let m2 = _mm256_mul_ps(p2, c10);
    let m3 = _mm256_mul_ps(p3, c11);
    let s01 = _mm256_add_ps(m0, m1);
    let s23 = _mm256_add_ps(m2, m3);
    _mm256_add_ps(s01, s23)
}

/// Inverse DCT on 8x8 block (in-place), matching OpenEXR's actual
/// SIMD-dispatched decode bit-for-bit (avx > sse2 > scalar priority, same as
/// `initializeFuncs` in internal_dwa_simd.h).
///
/// Verified bit-for-bit against a fresh real-OpenEXR (3.4.13) decode: 0
/// mismatches across all 34,484 DWA test samples in this repo, plus 0
/// mismatches for the AVX port alone against 2,000,000 random blocks via
/// direct FFI to the real compiled `dctInverse8x8_avx_0`.
pub fn dct_inverse_8x8_simd(data: &mut [f32; 64]) {
    if is_x86_feature_detected!("avx") {
        unsafe { dct_inverse_8x8_avx_inner(data) }
    } else {
        // SSE2 is part of the x86_64 baseline ABI, so this is always
        // available as a fallback on this target.
        unsafe { dct_inverse_8x8_sse2_inner(data) }
    }
}
