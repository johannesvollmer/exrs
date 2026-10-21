# Feature Spec: Deep Data

## User Stories

### User Story 1: Read deep layers (P1)

As an image pipeline developer, I want to read every sample of a deep scan line
or deep tile layer, so that I can composite or inspect a deep render without
re-encoding it.

**Acceptance Criteria**

- Given a deep scan line file with `Uncompressed`, `RLE` or `ZIP1`
  compression, when I read it with `read().deep_data()`, then every layer yields
  channel samples whose values equal the values written by OpenEXR.
- Given the same file, when I read it with the block API, then every block has a
  block-wide cumulative pixel offset table and native-endian sample data.
- Given a deep tiled file, when I read it, then the tile indices and per-pixel
  sample counts match OpenEXR.
- Given a file with a negative `maxSamplesPerPixel` (-1, "unknown while
  writing"), when I read it, then reading succeeds and the value is treated as
  unknown.
- Given a file whose chunks overlap or whose offset table has duplicate entries,
  when I read it, then the reader returns an error instead of allocating
  unbounded memory.

### User Story 2: Write deep layers (P1)

As a renderer author, I want to write deep scan line and deep tile layers, so
that OpenEXR 3.x can open them.

**Acceptance Criteria**

- Given a header with `deep = true` and a supported compression, when I write
  blocks with `image.write()` (or the block API), then `exrinfo` and
  OpenEXR 3.1.11 report the matching part type, channel list, chunk count,
  version and sample counts.
- Given a written file, when OpenEXR reads it, then its samples equal the
  input samples bit for bit.
- Given a partial tile, when I write it, then the offset table padding matches
  OpenEXR's writer for the supported tile sizes.

### User Story 3: Metadata parity (P1)

As a user of deep images, I want the deep attributes typed correctly, so that
round trips preserve them and other tools accept them.

**Acceptance Criteria**

- Given `deepImageState`, when I read or write it, then it is a one-byte enum
  (`Messy`, `Sorted`, `NonOverlapping`, `Tidy`) as in `ImfDeepImageState.h`.
- Given a deep header, when I write it, then `type` is `deepscanline` or
  `deeptile`, `version` is 1, and `maxSamplesPerPixel` is present when known.
- Given `deep = false`, when I build without the `deep` feature, then the public
  API is unchanged and deep attributes still parse.

### User Story 4: Composite deep pixels (P2)

As a compositor author, I want front-to-back compositing that matches OpenEXR,
so that flattened output is portable.

**Acceptance Criteria**

- Given samples from several deep images, when I flatten them, then the result
  equals OpenEXR's `DeepCompositing::composite_pixel` for the same samples.
- Given more than one contributing source, when I composite, then samples are
  sorted by `Z`, then `ZBack`, then source order; with one source, stored order
  is preserved.
- Given an already opaque accumulation, when further samples are processed,
  then they are skipped.
- Given `ZBack` is absent, then `ZBack` is treated as `Z`.

### User Story 5: Feature gating (P2)

As a library consumer, I want deep code to cost nothing when unused.

**Acceptance Criteria**

- Given `--no-default-features`, when I build, then no deep codec, block or
  utility is compiled.
- Given `--features deep`, when I build, then deep I/O, metadata, compositing
  and flattening compile.

## Non-Goals

- Lossy deep codecs (DWA, PIZ, PXR24, B44) are not supported; deep data supports
  `Uncompressed`, `RLE` and `ZIP1` only.
- SIMD implementations of ZIP preprocessing or compositing.
- Splitting or merging overlapping volumetric samples is out of scope; they are
  composited in depth order without being split.

## Requirements

- R1: Deep chunk decoding must accept every chunk shape OpenEXR 3.x writes and
  reject malformed sizes without panicking.
- R2: Deep chunk encoding must write the wire layout defined by
  `ImfDeepScanLineOutputFile.cpp` and `ImfDeepTiledOutputFile.cpp`.
- R3: `deepImageState` is a one-byte attribute, not a rational.
- R4: `maxSamplesPerPixel == -1` means unknown and must be accepted.
- R5: ZIP and RLE preprocessing for deep sample data and offset tables must match
  `ImfZip.cpp` and `ImfRle.cpp`.
- R6: Flattening must follow the formula in `ImfDeepCompositing.cpp`; the
  one-pixel near-tie in the reference fixture is covered by a documented
  tolerance.
- R7: Size arithmetic and allocation derived from untrusted counts must be
  checked or fail with `Error`, never panic or wrap.
- R8: All deep code is behind the `deep` feature.

## Risks

- **One-pixel reference tie.** `v2/LowResLeftView/composited.exr` agrees with
  OpenEXR compositing within half-float rounding except at one pixel whose two
  nearest samples have almost equal depths. That pixel is allowed to differ by a
  documented tolerance; do not tighten the test to exact equality.
- **Large files.** A 1024x576 deep image has millions of samples; quadratic
  per-pixel block lookup in flattening makes compositing slow. Mitigation:
  document the block lookup and keep it linear in the number of blocks.
- **API break.** Changing `deepImageState` from `Rational` to the enum breaks
  code that used the wrong type. This is intentional and called out in the PR.
- **Feature-gated rot.** Deep code is not compiled by default; CI must run
  `--features deep`.

## Follow-Up Review Constraints

These constraints come from the maintainer review of PR #292.

- The block module reads flat and deep chunks through one runtime enum
  (`Flat`/`Deep`), not two duplicated reader/writer pipelines. Deep is not a
  feature gate inside the block layer.
- The high-level reader/writer keeps separate static types, but `pedantic`,
  `parallel`, and the `from_*` entry points live in one shared wrapper.
- The allocated deep channel type is named `AnyAnyDeepChannels` (and related
  types), leaving room for a later non-allocating variant.
- Compositing is a follow-up surface. It must not force allocations per pixel,
  must let the caller supply channel indices, must represent missing channels
  as `Option`, and must expose explicit `_or_zero` / `_or_transparent`
  conveniences.
- A sample is a single number in this crate. The RGB+depth+alpha value is
  `RGBADepthF32Premultiplied`, not `RGBADepthF32Premultiplied`.
- Duplicate channel detection belongs to `ChannelList`.
- A flat reader that encounters a deep layer returns `Invalid`, not
  `Unsupported`; `Unsupported` is reserved for missing library features.
