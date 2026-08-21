//! Per-extension x86 SIMD token getters for the compression codecs.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(crate) mod x86 {
    pub(crate) mod miraculix_x86 {
        use miraculix::x86::{
            detect_features,
            ops::{
                avx::{avx::Avx, avx2::Avx2, f16c::F16c},
                avx512::{avx512bw::Avx512Bw, avx512dq::Avx512Dq, avx512f::Avx512f},
                sse::{sse::Sse, sse2::Sse2, sse41::Sse41, ssse3::Ssse3},
            },
        };

        /// Base SSE token (f32 ops only). `color_space_conversion`'s and
        /// `discrete_cosine_transform`'s SSE-tier kernels only ever touch
        /// `f32` arithmetic, so they need this rather than [`sse2`].
        #[inline(always)]
        pub(crate) fn sse() -> Option<Sse> {
            Sse::from_features(detect_features())
        }

        /// SSE2 baseline token. Pairs with [`ssse3`] for the SSE-tier kernels.
        #[inline(always)]
        pub(crate) fn sse2() -> Option<Sse2> {
            Sse2::from_features(detect_features())
        }

        /// SSSE3 token.
        #[inline(always)]
        pub(crate) fn ssse3() -> Option<Ssse3> {
            Ssse3::from_features(detect_features())
        }

        /// SSE4.1 token.
        #[inline(always)]
        pub(crate) fn sse41() -> Option<Sse41> {
            Sse41::from_features(detect_features())
        }

        /// F16C token.
        #[inline(always)]
        pub(crate) fn f16c() -> Option<F16c> {
            F16c::from_features(detect_features())
        }

        /// AVX2 token.
        // Not yet called: DWA's kernels only ever need `avx()`/`f16c()` at
        // this tier. Kept for `optimize_bytes` (ZIP/RLE SIMD), not ported yet.
        #[inline(always)]
        #[allow(dead_code)]
        pub(crate) fn avx2() -> Option<Avx2> {
            Avx2::from_features(detect_features())
        }

        /// Base AVX token (f32 ops only).
        #[inline(always)]
        pub(crate) fn avx() -> Option<Avx> {
            Avx::from_features(detect_features())
        }

        /// AVX-512F token. Pairs with [`avx512bw`]/[`avx512dq`] for the
        /// AVX-512-tier kernels.
        #[inline(always)]
        pub(crate) fn avx512f() -> Option<Avx512f> {
            Avx512f::from_features(detect_features())
        }

        /// AVX-512BW token.
        // Not yet called: DWA's kernels only ever need `avx512f()`/`avx512dq()`
        // at this tier. Kept for `optimize_bytes` (ZIP/RLE SIMD), not ported yet.
        #[inline(always)]
        #[allow(dead_code)]
        pub(crate) fn avx512bw() -> Option<Avx512Bw> {
            Avx512Bw::from_features(detect_features())
        }

        /// AVX-512DQ token. `discrete_cosine_transform`'s AVX-512 tier needs
        /// this for its `extract_f32x8_from_x16`/`insert_f32x8_into_x16`
        /// transpose recombine step (`vextractf32x8`/`vinsertf32x8`, DQ-only).
        #[inline(always)]
        pub(crate) fn avx512dq() -> Option<Avx512Dq> {
            Avx512Dq::from_features(detect_features())
        }
    }
}
