#![cfg(feature = "sse2-tests")]

use pulp::x86::{V1, V3};
use exr::compression::dwa::idct::*;
use exr::compression::dwa::idct::x86::*;


#[test]
pub fn assert_sse2_close_to_scalar_reference() {
    testing::assert_blocks_match(
        "SSE2 inverse DCT",
        dct_inverse_8x8_scalar,
        |data| sse2::dct_inverse_8x8(expect_sse2_without_avx2(), data),
    );
}

#[test]
pub fn assert_sse2_forward_close_to_scalar_reference() {
    testing::assert_blocks_match(
        "SSE2 forward DCT",
        dct_forward_8x8_scalar,
        |data| sse2::dct_forward_8x8(expect_sse2_without_avx2(), data),
    );
}



fn expect_sse2() -> V1 {
    V1::try_new().expect("SSE2 SIMD mode requested, but the SSE2 tier is unavailable")
}

fn expect_sse2_without_avx2() -> V1 {
    assert!(V3::try_new().is_none(), "SSE2 dispatch fallback test must run with AVX2 hidden");
    expect_sse2()
}

