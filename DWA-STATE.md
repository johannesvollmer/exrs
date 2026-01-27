# DWA Codec Status

* The DWAA/B decoder is functionally complete but still not bit‑exact with the OpenEXR reference: the last scanline differs by ±1 half‑float ULP in a few pixels under certain images.
* Zigzag unpacking, IDCT constants, CSC math, and float↔half conversions mirror OpenEXR's scalar pipeline. Remaining differences stem from the surrounding `LossyDctDecoder` loop (block ordering, constant block shortcut, rounding order).
* SIMD paths (SSE2/AVX/NEON) from OpenEXR are not ported. All math runs on scalar f32 and Rust’s `half` conversions, so performance and rounding differ from the C implementation.
* We do not implement OpenEXR's `convertFloatToHalf64_*` batching or its aligned row‑block staging. Our decode loop writes scanlines directly, which changes the reduction order and rounding.
* A DWAA/B encoder now exists and can roundtrip existing non‑lossy inputs, but it has not been validated against OpenEXR bitstreams beyond functional testing.
* Improvements needed for parity:
  - Port `LossyDctDecoder_execute` verbatim (including `DctCoderChannelData`, constant block detection, row block allocation, and planar writeback).
  - Reuse OpenEXR’s half conversion helpers (`float_to_half_int`) everywhere and consider calling the SIMD paths via intrinsics to match rounding and throughput.
  - Add conformance tests that compare against the OpenEXR decoder/encoder for multiple images and codecs (DWAA/DWAB, F16/F32) to guard against regressions and ensure bitstream compatibility.
