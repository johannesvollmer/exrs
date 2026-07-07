#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
compile_error!("SSE2 SIMD tests require an x86 or x86_64 target");

use exr::compression::simd_test_support::{
    assert_dispatch_picks_sse2_without_avx2, assert_sse2_available,
    assert_sse2_close_to_scalar_reference, selected_simd_tier,
};

fn require_sse2() {
    eprintln!("selected SIMD tier: {:?}", selected_simd_tier());
    assert_sse2_available();
}

#[test]
fn sse2_idct_matches_scalar_reference() {
    require_sse2();
    assert_sse2_close_to_scalar_reference();
}

#[test]
fn dispatch_picks_sse2_when_avx2_is_unavailable() {
    require_sse2();
    assert_dispatch_picks_sse2_without_avx2();
}
