use exr::compression::simd_test_support::{
    assert_dispatch_picks_sse2, assert_dispatch_picks_sse2_for_forward,
    assert_sse2_close_to_scalar_reference, assert_sse2_forward_close_to_scalar_reference,
    expect_sse2, expect_sse2_without_avx2,
};

#[test]
fn sse2_idct_matches_scalar_reference() {
    assert_sse2_close_to_scalar_reference(expect_sse2());
}

#[test]
fn sse2_fdct_matches_scalar_reference() {
    assert_sse2_forward_close_to_scalar_reference(expect_sse2());
}

#[test]
fn dispatch_picks_sse2_when_avx2_is_unavailable() {
    assert_dispatch_picks_sse2(expect_sse2_without_avx2());
}

#[test]
fn dispatch_picks_sse2_for_forward_dct_when_avx2_is_unavailable() {
    assert_dispatch_picks_sse2_for_forward(expect_sse2_without_avx2());
}
