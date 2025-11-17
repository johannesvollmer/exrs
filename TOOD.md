• From surveying both repos side-by-side, here’s where our Rust implementation still lags the C++ OpenEXR library functionally (beyond the new runtime B44/DWA tables):

  1. Color space & chromaticity metadata
      - The C++ API exposes full Chromaticities, whiteLuminance, adoptedNeutral, lookModTransform, renderingTransform, etc., and uses them in conversions like RGBtoXYZ.
      - Our Rust meta::attribute::{ChannelList, ColorSpace} support is minimal—we can read/write the attributes but we don’t provide helpers for interpreting them or applying transforms. If we want feature
        parity, we’d add structs mirroring Chromaticities, ColorConversion, etc., and hook them into image::read/write.
  2. Color space conversions / halfFunction pipeline
      - OpenEXR’s C++ implementation has utilities for pre-exposure, color transform application, and conversions between color spaces (via ImfPreviewImage, ImfChromaticities, ImfLut).
      - The Rust crate doesn’t expose any of that—color data is treated as raw channel values. Implementing those helpers would bring us closer to parity for applications needing display transforms or
        LUTs.
  3. DWA compression (DWAA/DWAB)
      - OpenEXR’s core library supports DWA compression (with the new runtime tables).
      - In Rust Compression::DWAA/DWAB exist but trying to write with those methods returns “unimplemented compression method.” There is no decoder either. Bridging that gap requires porting the DWA codec
        (including the nonlinear lookup table generation and dwaCompressionLevel metadata wiring).
  4. HTJ2K (high-throughput JPEG 2000) compression
      - The C++ side has HTJ2K (HTJ2K32/256) via OpenEXRCore.
      - Our Compression enum has HTJ2K32/HTJ2K256 variants but their code paths still error out with “unimplemented compression method.” Implementing these codecs (possibly via binding to an existing
        JPEG2000 library) would match OpenEXR feature-wise.
  5. Metadata completeness
      - OpenEXR supports a wide range of standard attributes (time code, key code, multi-view, view channels, etc.) plus custom attributes. The Rust crate covers the basics but some standard attributes
        (e.g. multi-view/channel layout, deepImageState, previewImage, acesData) are stubbed or missing.
      - We should audit meta/attribute.rs against OpenEXRCore/openexr_attr.h to ensure every standard attribute type is represented, with correct read/write semantics.
  6. Deep image utilities
      - C++ has a robust deep compositing toolkit (tidying, flattening, merging, CompositeDeepScanLine, etc.) which we’re now trying to match, but there are still gaps: volume sample handling,
        DeepImageState, per-sample ZBack semantics, and performance optimizations like parallel compositing.
      - Continuing to port those helpers and ensuring metadata like deepImageState is set/propagated will close the gap.
  7. Color decision list (CDL)/ACES metadata
      - OpenEXR includes optional metadata for ACES workflows. The Rust side mostly ignores these. If parity is a goal, we’d need to model and expose that metadata (e.g. acesData, chromaticities, etc.).

  Next steps:

  - Implement lazy B44/DWA tables as discussed earlier.
  - Flesh out the metadata layer to cover every attribute defined in OpenEXRCore (including chromaticities, transforms, multi-view info, ACES/CDL tags).
  - Port missing compression codecs (DWAA/B, HTJ2K) or at least emit clear errors until they’re supported.
  - Add color-space conversion helpers analogous to ImfChromaticities utilities so users can interpret metadata, not just store it.

  That set of improvements will bring the Rust crate much closer to the capabilities of the reference C++ implementation.
