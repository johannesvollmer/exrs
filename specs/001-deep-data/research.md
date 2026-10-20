# Research: Deep Data

## Decisions

### Decision: High-level deep API, with block access as an escape hatch

**Chosen**: Adopt the `DeepChannels` / `DeepImage` / `DeepLayersImage` API from
PR #292. Read with `read().deep_data()`, write with `image.write()`, and keep
`UncompressedDeepBlock` public for callers that need raw chunks.

**Reason**: The existing flat image builder can be extended with a deep layer
type instead of forcing every caller through variable-length blocks. PR #292
already implements this, is based on current `master`, and passes the OpenEXR
cross-checks. Keeping the block API avoids closing the door on custom pipelines.

**Rejected Alternatives**

- *Block-only API*: forces every consumer to reassemble layers and channel
  storage; it was the shape of the older branch and is strictly less usable.
- *Two independent APIs*: maintenance cost and drift; one image type with a
  block escape hatch is enough.

### Decision: `composited.exr` is a valid oracle with a one-pixel tie

**Chosen**: Keep the reference-image test with its documented tolerance.

**Reason**: Comparing OpenEXR 3.1.11's `DeepCompositing` result with the
reference shows agreement within half-float rounding for all but one pixel,
`(716, 262)`. There, two samples have almost equal depths and the reference
flatten orders them differently, producing a difference of about `0.0326`.
PR #292's test allows one such pixel per channel. Exact equality is the wrong
oracle because half rounding alone produces ~129k sub-`2e-3` differences.

**Rejected Alternatives**

- *Exact equality*: fails on half rounding and on the documented tie.
- *Discard the reference*: loses the only real-image compositing check.

### Decision: `deepImageState` is an unconditional enum

**Chosen**: Keep `DeepImageState` compiled without the `deep` feature and type
`LayerAttributes::deep_image_state` as `Option<DeepImageState>`.

**Reason**: The attribute is defined unconditionally in OpenEXR and is one byte
(`ImfDeepImageState.h`). Feature-gating the type forced the field to use
`Rational`, which writes/read the wrong wire format (8 bytes).

**Rejected Alternatives**

- *Feature-gate the field type*: changes the public struct shape per feature;
  callers cannot write the attribute without enabling deep.
- *Keep `Rational`*: fails OpenEXR readers for the attribute.

### Decision: `maxSamplesPerPixel == -1` means unknown

**Chosen**: Map negative values to `None` and accept them when validating.

**Reason**: The OpenEXR header comment states `-1` is written while a deep file
is open and is replaced on close. A reader must not reject such a file.

**Rejected Alternatives**

- *Error on negative*: rejects files OpenEXR accepts.

### Decision: ZIP deep preprocessing mirrors `ImfZip`

**Chosen**: Reorder bytes into even/odd halves, apply the wrapping delta
predictor with `+128`, then zlib; reverse on decode. Store raw bytes when
compression does not shrink the payload.

**Reason**: `ImfZip::compress`/`uncompress` is used for both deep sample data and
the sample-count table. The same preprocessing is applied to both.

**Rejected Alternatives**

- *Treat deep bytes as opaque*: does not decode OpenEXR files.

## References

- `openexr/src/lib/OpenEXR/ImfDeepScanLineOutputFile.cpp`
- `openexr/src/lib/OpenEXR/ImfDeepScanLineInputFile.cpp`
- `openexr/src/lib/OpenEXR/ImfDeepTiledOutputFile.cpp`
- `openexr/src/lib/OpenEXR/ImfDeepCompositing.cpp`
- `openexr/src/lib/OpenEXR/ImfCompositeDeepScanLine.cpp`
- `openexr/src/lib/OpenEXR/ImfZip.cpp`
- `openexr/src/lib/OpenEXR/ImfDeepImageState.h`
- OpenEXR file format specification: deep data sections.
